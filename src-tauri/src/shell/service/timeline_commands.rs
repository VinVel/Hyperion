/*
 * Copyright (c) 2026 VinVel
 *
 * SPDX-License-Identifier: AGPL-3.0-only
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, version 3 only.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 *
 * Project home: hyperion.velcore.net
 */

use matrix_sdk::{Room, ruma::EventId};

use crate::account::{AccountClientSnapshot, AccountManager};

use super::{
    super::sync::emit_shell_room_updated,
    DEFAULT_EVENT_CONTEXT_LIMIT, DEFAULT_TIMELINE_LIMIT, MAX_RESTORED_TIMELINE_ITEMS, ShellManager,
    caching::restored_timeline_limit,
    paging::{
        focused_timeline_page_token, load_live_room_timeline, load_paginated_room_timeline,
        parse_timeline_page_token,
    },
    read_state::mark_room_read_locally,
    room::{resolve_room, room_title},
};
use crate::shell::types::{GetRoomEventContextRequest, GetRoomTimelineRequest, RoomTimeline};

impl ShellManager {
    pub async fn get_room_timeline(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: GetRoomTimelineRequest,
    ) -> Result<RoomTimeline, String> {
        let Some(account) = account_manager.active_account_client(app).await? else {
            return Err(String::from("No active account is available"));
        };

        if let Some(cached_timeline) =
            self.cached_timeline_response(app, account_manager, &account, &request)
        {
            return Ok(cached_timeline);
        }

        self.sync_manager
            .ensure_started_for_manager(account_manager, app)
            .await?;

        let room = resolve_room(&account.client, &request.room_id)?;
        self.prepare_room_timeline_load(&account, &room);
        let (items, next_before) = self
            .load_room_timeline_items(app, &account, &room, &request)
            .await?;
        self.after_room_timeline_load(&account, &room, &request, &items, next_before.as_deref())
            .await?;

        Ok(RoomTimeline {
            room_id: room.room_id().to_string(),
            items,
            next_before,
            focused_event_id: None,
        })
    }

    pub async fn get_room_event_context(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: GetRoomEventContextRequest,
    ) -> Result<RoomTimeline, String> {
        self.sync_manager
            .ensure_started_for_manager(account_manager, app)
            .await?;

        let Some(account) = account_manager.active_account_client(app).await? else {
            return Err(String::from("No active account is available"));
        };
        let room = resolve_room(&account.client, &request.room_id)?;
        self.mark_room_focused(&account.account_key, room.room_id().as_str());
        let event_id = EventId::parse(&request.event_id)
            .map_err(|error| format!("Invalid event id: {error}"))?
            .clone();
        let context_limit = request.context_limit.unwrap_or(DEFAULT_EVENT_CONTEXT_LIMIT);
        let items = self
            .timeline_registry
            .focused_timeline_items(&account.account_key, &room, event_id.clone(), context_limit)
            .await?;
        let title = room_title(&room).await?;
        self.index_timeline_items(
            &account.account_key,
            &account.store_dir,
            room.room_id().as_str(),
            &title,
            &items,
        )
        .await;
        self.delete_focused_redacted_timeline_items(
            &account.account_key,
            &account.store_dir,
            &room,
            event_id.clone(),
            context_limit,
        )
        .await;

        Ok(RoomTimeline {
            room_id: room.room_id().to_string(),
            items,
            next_before: Some(focused_timeline_page_token(event_id.as_ref(), 1)),
            focused_event_id: Some(request.event_id),
        })
    }

    fn cached_timeline_response(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        account: &AccountClientSnapshot,
        request: &GetRoomTimelineRequest,
    ) -> Option<RoomTimeline> {
        if self.room_timeline_cache_was_served(&account.account_key, &request.room_id)
            && request.before.is_none()
        {
            return None;
        }

        let (items, next_before) =
            Self::cached_room_timeline(&account.account_key, &account.store_dir, &request.room_id)?;
        let (items, next_before) = cached_timeline_window(&items, next_before, request)?;

        self.mark_room_focused(&account.account_key, &request.room_id);
        if request.before.is_none() {
            self.mark_room_timeline_cache_served(&account.account_key, &request.room_id);
        }
        self.refresh_room_timeline_in_background(app, account_manager, request.clone());
        Some(RoomTimeline {
            room_id: request.room_id.clone(),
            items,
            next_before,
            focused_event_id: None,
        })
    }

    fn refresh_room_timeline_in_background(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: GetRoomTimelineRequest,
    ) {
        let shell_manager = self.clone();
        let app = app.clone();
        let account_manager = account_manager.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = shell_manager
                .refresh_room_timeline_after_cached_response(&app, &account_manager, request)
                .await
            {
                eprintln!("Failed to refresh cached room timeline in background: {error}");
            }
        });
    }

    async fn refresh_room_timeline_after_cached_response(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: GetRoomTimelineRequest,
    ) -> Result<(), String> {
        self.sync_manager
            .ensure_started_for_manager(account_manager, app)
            .await?;

        let Some(account) = account_manager.active_account_client(app).await? else {
            return Ok(());
        };
        let room = resolve_room(&account.client, &request.room_id)?;
        self.prepare_room_timeline_load(&account, &room);
        let (items, next_before) = self
            .load_room_timeline_items(app, &account, &room, &request)
            .await?;
        self.after_room_timeline_load(&account, &room, &request, &items, next_before.as_deref())
            .await?;
        emit_shell_room_updated(app, &account.account_key, room.room_id().as_str(), false);

        Ok(())
    }

    fn prepare_room_timeline_load(&self, account: &AccountClientSnapshot, room: &Room) {
        self.mark_room_focused(&account.account_key, room.room_id().as_str());
        self.schedule_room_timeline_warmup(
            account.client.clone(),
            &account.account_key,
            &account.store_dir,
            room.room_id().to_string(),
        );
    }

    async fn load_room_timeline_items(
        &self,
        app: &tauri::AppHandle,
        account: &AccountClientSnapshot,
        room: &Room,
        request: &GetRoomTimelineRequest,
    ) -> Result<(Vec<crate::shell::types::RoomTimelineItem>, Option<String>), String> {
        let page_limit = request.limit.unwrap_or(DEFAULT_TIMELINE_LIMIT);
        if request.before.is_none() {
            self.timeline_registry
                .subscribe_live_timeline_updates(app.clone(), &account.account_key, room)
                .await?;
            let remembered_count = Self::remembered_timeline_item_count(
                &account.account_key,
                &account.store_dir,
                room.room_id().as_str(),
            );
            let restored_limit =
                restored_timeline_limit(page_limit, remembered_count, MAX_RESTORED_TIMELINE_ITEMS);
            return load_live_room_timeline(
                &self.timeline_registry,
                &account.account_key,
                room,
                restored_limit,
                page_limit,
            )
            .await;
        }

        load_paginated_room_timeline(
            &self.timeline_registry,
            &account.account_key,
            room,
            page_limit,
            request.before.as_deref(),
        )
        .await
    }

    async fn after_room_timeline_load(
        &self,
        account: &AccountClientSnapshot,
        room: &Room,
        request: &GetRoomTimelineRequest,
        items: &[crate::shell::types::RoomTimelineItem],
        next_before: Option<&str>,
    ) -> Result<(), String> {
        Self::record_room_timeline_pagination(account, room, request, items, next_before);

        if request.before.is_none()
            && let Some(latest_item) = items.last()
        {
            self.timeline_registry
                .mark_live_timeline_as_read(&account.account_key, room)
                .await?;
            mark_room_read_locally(
                &self.locally_read_room_state,
                &account.account_key,
                room.room_id().as_str(),
                &latest_item.event_id,
            );
        }

        let title = room_title(room).await?;
        self.index_timeline_items(
            &account.account_key,
            &account.store_dir,
            room.room_id().as_str(),
            &title,
            items,
        )
        .await;
        self.delete_redacted_timeline_items(&account.account_key, &account.store_dir, room)
            .await;
        if request.before.is_none() {
            Self::remember_timeline(
                &account.account_key,
                &account.store_dir,
                room.room_id().as_str(),
                items,
                next_before,
            );
        }

        Ok(())
    }

    fn record_room_timeline_pagination(
        account: &AccountClientSnapshot,
        room: &Room,
        request: &GetRoomTimelineRequest,
        items: &[crate::shell::types::RoomTimelineItem],
        next_before: Option<&str>,
    ) {
        if request.before.is_none() {
            return;
        }

        Self::remember_timeline_item_count_after_pagination(
            &account.account_key,
            &account.store_dir,
            room.room_id().as_str(),
            request.before.as_deref(),
            request.limit.unwrap_or(DEFAULT_TIMELINE_LIMIT),
            items.len(),
        );
        Self::prepend_cached_timeline_items(
            &account.account_key,
            &account.store_dir,
            room.room_id().as_str(),
            items,
            next_before,
        );
    }
}

fn cached_timeline_window(
    items: &[crate::shell::types::RoomTimelineItem],
    next_before: Option<String>,
    request: &GetRoomTimelineRequest,
) -> Option<(Vec<crate::shell::types::RoomTimelineItem>, Option<String>)> {
    let Some(before) = request.before.as_deref() else {
        return Some((items.to_vec(), next_before));
    };
    let page_index = parse_timeline_page_token(before)?;
    let page_limit = usize::from(request.limit.unwrap_or(DEFAULT_TIMELINE_LIMIT));
    if page_limit == 0 {
        return None;
    }

    let visible_count = page_index.saturating_mul(page_limit);
    if items.len() <= visible_count {
        return None;
    }

    let older_end = items.len().saturating_sub(visible_count);
    let older_start = older_end.saturating_sub(page_limit);
    let cached_items = items[older_start..older_end].to_vec();
    if cached_items.is_empty() {
        return None;
    }

    let next_before = if older_start == 0 {
        None
    } else {
        Some(crate::shell::service::paging::timeline_page_token(
            page_index + 1,
        ))
    };

    Some((cached_items, next_before))
}
