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

use crate::{
    account::{AccountClientSnapshot, AccountManager, ActiveAccount},
    shell::{
        service::{
            DEFAULT_EVENT_CONTEXT_LIMIT, DEFAULT_TIMELINE_LIMIT, MAX_RESTORED_TIMELINE_ITEMS,
            ShellCacheState, ShellManager,
            caching::restored_timeline_limit,
            paging::{
                focused_timeline_page_token, parse_focused_timeline_page_token, timeline_page_token,
            },
            read_state::mark_room_read_locally,
        },
        types::{
            GetRoomEventContextRequest, GetRoomEventRawRequest, GetRoomTimelineRequest,
            PaginateRoomTimelineRequest, ResolveRoomReplyPreviewRequest, RoomTimeline,
            RoomTimelineItem, RoomTimelinePaginationResponse, RoomTimelineReplyPreview,
            RoomTimelineReplyPreviewState, apply_timeline_presentation,
        },
    },
};

use super::super::{resolve_room, room_title};

impl ShellManager {
    pub async fn get_room_event_raw(
        &self,
        active_account: &ActiveAccount,
        request: GetRoomEventRawRequest,
    ) -> Result<String, String> {
        let account = active_account.snapshot();
        let room = resolve_room(&account.client, &request.room_id)?;
        let event_id = EventId::parse(&request.event_id)
            .map_err(|error| format!("Invalid event id: {error}"))?;
        let event = room
            .event(&event_id, None)
            .await
            .map_err(|error| format!("Could not fetch raw event: {error}"))?;

        Ok(event.raw().json().get().to_owned())
    }

    pub async fn get_room_timeline(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        active_account: &ActiveAccount,
        request: GetRoomTimelineRequest,
    ) -> Result<RoomTimeline, String> {
        let account = active_account.snapshot();

        self.sync_coordinator
            .ensure_account_running(app, account_manager, account.clone())
            .await?;

        let room = resolve_room(&account.client, &request.room_id)?;
        if request.before.is_none() {
            self.mark_room_focused(app, &account.account_key, &room);
        }
        let focused_event = request
            .before
            .as_deref()
            .and_then(parse_focused_timeline_page_token)
            .map(|(event_id, _page_index)| {
                EventId::parse(event_id).map(|event_id| (event_id, DEFAULT_EVENT_CONTEXT_LIMIT))
            })
            .transpose()
            .map_err(|error| format!("Invalid focused event id: {error}"))?;
        let timeline = self
            .sync_coordinator
            .open_timeline_view(app, &account.account_key, &room, focused_event)
            .await?;
        self.prepare_room_timeline_load(account, &room);
        let (mut items, next_before) = self
            .load_room_timeline_items(app, account, &room, &request)
            .await?;
        apply_timeline_presentation(&mut items, room.room_id().as_str());
        self.after_room_timeline_load(account, &room, &request, &items)
            .await?;

        timeline.snapshot(next_before)
    }

    pub async fn paginate_room_timeline_backwards(
        &self,
        active_account: &ActiveAccount,
        request: PaginateRoomTimelineRequest,
    ) -> Result<RoomTimelinePaginationResponse, String> {
        let account = active_account.snapshot();
        if request.timeline_identity.account_key != account.account_key
            || request.timeline_identity.room_id != request.room_id
        {
            return Err(String::from("Timeline pagination request is obsolete"));
        }
        // A command performs one SDK page. Cursor-advancing empty pages are retried by
        // the focused UI with bounded backoff, rather than issuing a burst here.
        // Compatibility tokens and known_event_ids are not pagination authority.
        let (reached_start, snapshot) = self
            .sync_coordinator
            .paginate_visible(
                &request.timeline_identity,
                request.limit.unwrap_or(DEFAULT_TIMELINE_LIMIT).max(1),
            )
            .await?;
        let room = resolve_room(&account.client, &request.room_id)?;
        self.index_loaded_timeline_items(account, &room, &snapshot.items)
            .await?;
        Ok(pagination_response(
            request,
            reached_start,
            snapshot.revision,
        ))
    }

    pub async fn get_room_event_context(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        active_account: &ActiveAccount,
        request: GetRoomEventContextRequest,
    ) -> Result<RoomTimeline, String> {
        let account = active_account.snapshot();

        self.sync_coordinator
            .ensure_account_running(app, account_manager, account.clone())
            .await?;

        let room = resolve_room(&account.client, &request.room_id)?;
        let event_id = EventId::parse(&request.event_id)
            .map_err(|error| format!("Invalid event id: {error}"))?
            .clone();
        let context_limit = request.context_limit.unwrap_or(DEFAULT_EVENT_CONTEXT_LIMIT);
        let timeline = self
            .sync_coordinator
            .open_timeline_view(
                app,
                &account.account_key,
                &room,
                Some((event_id.clone(), context_limit)),
            )
            .await?;
        let mut items = self
            .sync_coordinator
            .focused_timeline_items(&account.account_key, &room, event_id.clone(), context_limit)
            .await?;
        apply_timeline_presentation(&mut items, room.room_id().as_str());
        let title = room_title(&room).await?;
        self.search_service
            .index_timeline_items(
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

        timeline.snapshot(Some(focused_timeline_page_token(event_id.as_ref(), 1)))
    }

    pub async fn resolve_room_reply_preview(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        active_account: &ActiveAccount,
        request: ResolveRoomReplyPreviewRequest,
    ) -> Result<RoomTimelineReplyPreview, String> {
        let account = active_account.snapshot();

        let event_id = match EventId::parse(&request.event_id) {
            Ok(event_id) => event_id.clone(),
            Err(_) => {
                return Ok(RoomTimelineReplyPreview {
                    event_id: request.event_id,
                    state: RoomTimelineReplyPreviewState::InvalidRelation,
                    sender_id: None,
                    sender_display_name: None,
                    body: None,
                    is_redacted: false,
                });
            }
        };

        if let Some(cached_item) = ShellCacheState::cached_room_timeline_item(
            &account.account_key,
            &account.store_dir,
            &request.room_id,
            event_id.as_str(),
        ) {
            return Ok(reply_preview_from_timeline_item(
                request.event_id,
                &cached_item,
            ));
        }

        self.sync_coordinator
            .ensure_account_running(app, account_manager, account.clone())
            .await?;

        let room = resolve_room(&account.client, &request.room_id)?;
        let focused_items = match self
            .sync_coordinator
            .focused_timeline_items(
                &account.account_key,
                &room,
                event_id,
                DEFAULT_EVENT_CONTEXT_LIMIT,
            )
            .await
        {
            Ok(items) => items,
            Err(error) => {
                crate::utils::tracing::report_recoverable_error(
                    "shell.timeline",
                    "resolve_reply_preview",
                    "shell.reply_preview_failed",
                    "timeline",
                    &error,
                );
                return Ok(RoomTimelineReplyPreview {
                    event_id: request.event_id,
                    state: RoomTimelineReplyPreviewState::FailedToLoad,
                    sender_id: None,
                    sender_display_name: None,
                    body: None,
                    is_redacted: false,
                });
            }
        };

        Ok(reply_preview_from_focused_items(
            request.event_id,
            &focused_items,
        ))
    }

    fn prepare_room_timeline_load(&self, account: &AccountClientSnapshot, room: &Room) {
        self.sync_coordinator.schedule_room_timeline_warmup(
            account.client.clone(),
            &account.account_key,
            &account.store_dir,
            room.room_id().to_string(),
            self.search_service.indexer_clone(),
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
            self.sync_coordinator
                .subscribe_live_timeline_updates(
                    app.clone(),
                    &account.account_key,
                    &account.store_dir,
                    room,
                )
                .await?;
            let remembered_count = ShellCacheState::remembered_timeline_item_count(
                &account.account_key,
                &account.store_dir,
                room.room_id().as_str(),
            );
            let restored_limit =
                restored_timeline_limit(page_limit, remembered_count, MAX_RESTORED_TIMELINE_ITEMS);
            return self
                .sync_coordinator
                .load_live_room_timeline(&account.account_key, room, restored_limit, page_limit)
                .await;
        }

        self.sync_coordinator
            .load_paginated_room_timeline(
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
    ) -> Result<(), String> {
        if request.before.is_none()
            && let Some(latest_item) = items.last()
        {
            self.sync_coordinator
                .mark_live_timeline_as_read(&account.account_key, room)
                .await?;
            mark_room_read_locally(
                self.sync_coordinator.locally_read_room_state(),
                &account.account_key,
                room.room_id().as_str(),
                latest_item.event_id(),
            );
        }

        self.index_loaded_timeline_items(account, room, items).await
    }

    async fn index_loaded_timeline_items(
        &self,
        account: &AccountClientSnapshot,
        room: &Room,
        items: &[RoomTimelineItem],
    ) -> Result<(), String> {
        let title = room_title(room).await?;
        self.search_service
            .index_timeline_items(
                &account.account_key,
                &account.store_dir,
                room.room_id().as_str(),
                &title,
                items,
            )
            .await;
        self.delete_redacted_timeline_items(&account.account_key, &account.store_dir, room)
            .await;
        Ok(())
    }
}

fn reply_preview_from_focused_items(
    event_id: String,
    focused_items: &[RoomTimelineItem],
) -> RoomTimelineReplyPreview {
    let Some(item) = focused_items
        .iter()
        .find(|focused_item| focused_item.event_id() == event_id)
    else {
        return RoomTimelineReplyPreview {
            event_id,
            state: RoomTimelineReplyPreviewState::Inaccessible,
            sender_id: None,
            sender_display_name: None,
            body: None,
            is_redacted: false,
        };
    };

    reply_preview_from_timeline_item(event_id, item)
}

fn reply_preview_from_timeline_item(
    event_id: String,
    item: &RoomTimelineItem,
) -> RoomTimelineReplyPreview {
    if item.matrix.content.is_redacted {
        return RoomTimelineReplyPreview {
            event_id,
            state: RoomTimelineReplyPreviewState::DeletedRedacted,
            sender_id: Some(item.sender_id().to_owned()),
            sender_display_name: item.sender_display_name().map(ToOwned::to_owned),
            body: None,
            is_redacted: true,
        };
    }

    let body = item.body().trim();
    if body.is_empty() {
        return RoomTimelineReplyPreview {
            event_id,
            state: RoomTimelineReplyPreviewState::InvalidRelation,
            sender_id: Some(item.sender_id().to_owned()),
            sender_display_name: item.sender_display_name().map(ToOwned::to_owned),
            body: None,
            is_redacted: false,
        };
    }

    RoomTimelineReplyPreview {
        event_id,
        state: RoomTimelineReplyPreviewState::Resolved,
        sender_id: Some(item.sender_id().to_owned()),
        sender_display_name: item.sender_display_name().map(ToOwned::to_owned),
        body: Some(body.to_owned()),
        is_redacted: false,
    }
}

fn pagination_response(
    request: PaginateRoomTimelineRequest,
    reached_start: bool,
    revision: u64,
) -> RoomTimelinePaginationResponse {
    // These fields remain wire-compatible. No page traversal, row selection, or
    // server-cursor progress is inferred from them; the SDK owns that state.
    drop(request.known_event_ids);
    let next_before = if reached_start {
        None
    } else {
        Some(
            match request.timeline_identity.focused_event_id.as_deref() {
                Some(event) => focused_timeline_page_token(event, 1),
                None => timeline_page_token(1),
            },
        )
    };
    RoomTimelinePaginationResponse {
        timeline_identity: request.timeline_identity,
        revision,
        room_id: request.room_id,
        items: Vec::new(),
        token_changed: request.before != next_before,
        next_before,
        request_id: request.request_id,
        had_new_items: false,
        returned_item_count: 0,
        new_item_count: 0,
        duplicate_item_count: 0,
        continuation_attempt_count: 0,
        reached_start,
        reason: None,
    }
}

#[cfg(test)]
mod tests {
    use crate::shell::{
        service::paging::timeline_page_token,
        types::{
            PaginateRoomTimelineRequest, RoomTimelineItem, RoomTimelineReplyPreviewState,
            RoomTimelineSendState,
        },
    };

    use super::{pagination_response, reply_preview_from_focused_items};

    #[test]
    fn reply_preview_resolution_reports_inaccessible_target() {
        let preview = reply_preview_from_focused_items(String::from("$missing"), &[]);

        assert_eq!(preview.state, RoomTimelineReplyPreviewState::Inaccessible);
        assert_eq!(preview.event_id, "$missing");
        assert!(preview.body.is_none());
    }

    #[test]
    fn reply_preview_resolution_reports_deleted_target() {
        let mut item = test_timeline_item("$deleted", "Deleted body");
        item.matrix.content.is_redacted = true;

        let preview = reply_preview_from_focused_items(String::from("$deleted"), &[item]);

        assert_eq!(
            preview.state,
            RoomTimelineReplyPreviewState::DeletedRedacted
        );
        assert!(preview.is_redacted);
        assert!(preview.body.is_none());
    }

    #[test]
    fn reply_preview_resolution_reports_resolved_target() {
        let preview = reply_preview_from_focused_items(
            String::from("$target"),
            &[test_timeline_item("$target", "Original body")],
        );

        assert_eq!(preview.state, RoomTimelineReplyPreviewState::Resolved);
        assert_eq!(preview.sender_id.as_deref(), Some("@alice:example.org"));
        assert_eq!(preview.body.as_deref(), Some("Original body"));
    }

    #[test]
    fn completion_uses_sdk_terminal_status_even_without_visible_items() {
        for reached_start in [false, true] {
            let request = PaginateRoomTimelineRequest {
                timeline_identity: crate::shell::types::RoomTimelineIdentity {
                    account_key: "account".into(),
                    room_id: "room".into(),
                    instance_id: "instance".into(),
                    focused_event_id: None,
                },
                room_id: "room".into(),
                before: Some(timeline_page_token(1)),
                limit: None,
                request_id: "request".into(),
                known_event_ids: vec!["ignored".into()],
            };
            let response = pagination_response(request, reached_start, 7);
            assert_eq!(response.reached_start, reached_start);
            assert_eq!(response.next_before.is_none(), reached_start);
            assert!(response.items.is_empty());
            assert_eq!(response.revision, 7);
        }
    }

    fn test_timeline_item(event_id: &str, body: &str) -> RoomTimelineItem {
        let mut item = RoomTimelineItem::text_message(
            event_id.to_owned(),
            String::from("@alice:example.org"),
            Some(String::from("Alice")),
            body.to_owned(),
            1,
            false,
            false,
        );
        item.matrix.send_state = RoomTimelineSendState::Sent;
        item
    }
}
