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

use std::{cmp::Reverse, collections::HashSet};

use matrix_sdk::{
    Room,
    ruma::{
        EventId,
        events::room::message::{RoomMessageEventContent, RoomMessageEventContentWithoutRelation},
    },
};

use crate::{
    account::AccountManager,
    shell::{
        service::{
            RECENT_TIMELINE_WARM_ROOM_COUNT, ShellCacheState, ShellManager, ShellRoomListKind,
            read_state::{mark_room_read_locally, unread_message_count_for_shell},
            search::{first_visible_grapheme, matches_query, normalize_query, relative_time_label},
        },
        types::{
            EditRoomMessageRequest, GetRoomSummaryRequest, ListRoomThreadsRequest,
            ListSpacesRequest, RedactRoomMessageRequest, ReplyToRoomMessageRequest, RoomSummary,
            RoomThreadSummary, SendRoomMessageRequest, SendRoomMessageResponse,
            SetRoomTypingRequest, SpaceSummary, ToggleRoomReactionRequest,
            ToggleRoomReactionResponse,
        },
    },
    utils::time::now_unix_ms,
};

use super::{
    can_send_messages, homeserver_label, latest_activity_unix_ms, latest_preview_text,
    list::snapshot_room_list_for_account, participant_label, resolve_room, room_title,
    timeline::cached_timeline_item_count,
};

impl ShellManager {
    pub async fn list_room_threads(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: ListRoomThreadsRequest,
    ) -> Result<Vec<RoomThreadSummary>, String> {
        account_manager.ensure_loaded(app).await?;
        let Some(account) = account_manager.active_account_client_loaded() else {
            return Err(String::from("No active account is available"));
        };

        if !self
            .cache_state
            .room_thread_cache_was_served(&account.account_key)
            && let Some(cached_rooms) = ShellCacheState::cached_room_threads(
                &account.account_key,
                &account.store_dir,
                &request,
            )
        {
            self.cache_state
                .mark_room_thread_cache_served(&account.account_key);
            self.ensure_sync_in_background(app, account_manager);
            return Ok(cached_rooms);
        }

        self.sync_manager
            .ensure_started_for_account(app, account_manager, account.clone())
            .await?;

        let query = normalize_query(request.search_query.as_deref());
        let mut rooms = Vec::new();
        let mut all_room_summaries = Vec::new();
        let mut recent_room_candidates = Vec::new();
        let mut active_room_ids = HashSet::new();

        let room_list_snapshot = self
            .snapshot_room_list(&account.account_key, ShellRoomListKind::Conversations)
            .await?;
        if room_list_snapshot.is_empty()
            && request.search_query.as_deref().is_none_or(str::is_empty)
            && let Some(cached_rooms) = ShellCacheState::cached_room_threads(
                &account.account_key,
                &account.store_dir,
                &request,
            )
            && !cached_rooms.is_empty()
        {
            self.ensure_sync_in_background(app, account_manager);
            return Ok(cached_rooms);
        }

        for room in room_list_snapshot {
            let summary = self
                .build_room_thread_summary(&account.account_key, &room)
                .await?;
            active_room_ids.insert(summary.room_id.clone());
            self.search_service
                .index_room_summary(&account.account_key, &account.store_dir, &summary)
                .await;
            recent_room_candidates.push((summary.room_id.clone(), summary.last_activity_unix_ms));
            all_room_summaries.push(summary.clone());
            if matches_query(
                query.as_deref(),
                &[&summary.title, &summary.preview, &summary.participant_label],
            ) {
                rooms.push(summary);
            }
        }

        self.search_service
            .tombstone_stale_room_documents(
                &account.account_key,
                &account.store_dir,
                &active_room_ids,
            )
            .await;
        ShellCacheState::remember_room_threads(
            &account.account_key,
            &account.store_dir,
            &all_room_summaries,
        );

        self.schedule_recent_timeline_warmup(
            &account.client,
            &account.account_key,
            &account.store_dir,
            recent_room_candidates,
        );
        self.search_service
            .schedule_search_backfill(
                account.client.clone(),
                &account.account_key,
                &account.store_dir,
                rooms
                    .iter()
                    .map(|room| (room.room_id.clone(), room.last_activity_unix_ms))
                    .collect(),
            )
            .await;

        Ok(rooms)
    }

    pub async fn get_room_summary(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: GetRoomSummaryRequest,
    ) -> Result<RoomSummary, String> {
        account_manager.ensure_loaded(app).await?;
        let Some(account) = account_manager.active_account_client_loaded() else {
            return Err(String::from("No active account is available"));
        };
        if let Some(summary) = ShellCacheState::cached_room_summary(
            &account.account_key,
            &account.store_dir,
            &request.room_id,
        ) {
            return Ok(summary);
        }

        self.sync_manager
            .ensure_started_for_account(app, account_manager, account.clone())
            .await?;

        let room = resolve_room(&account.client, &request.room_id)?;
        self.mark_room_focused(&account.account_key, room.room_id().as_str());

        let title = room_title(&room).await?;
        let is_direct = room.is_direct().await.unwrap_or(false);
        let participant_label = participant_label(&room, is_direct);
        let topic = room.topic();
        let homeserver_label = homeserver_label(&room, &account.homeserver_url);
        let can_send_messages = can_send_messages(&room).await;

        Ok(RoomSummary {
            room_id: room.room_id().to_string(),
            title,
            participant_label,
            homeserver_label,
            topic,
            is_direct,
            can_send_messages,
        })
    }

    pub async fn send_room_message(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: SendRoomMessageRequest,
    ) -> Result<SendRoomMessageResponse, String> {
        account_manager.ensure_loaded(app).await?;
        let Some(account) = account_manager.active_account_client_loaded() else {
            return Err(String::from("No active account is available"));
        };

        self.sync_manager
            .ensure_started_for_account(app, account_manager, account.clone())
            .await?;

        let body = request.body.trim();
        if body.is_empty() {
            return Err(String::from("Message body must not be empty"));
        }

        let room = resolve_room(&account.client, &request.room_id)?;
        self.mark_room_focused(&account.account_key, room.room_id().as_str());
        self.timeline_service
            .registry()
            .subscribe_live_timeline_updates(app.clone(), &account.account_key, &room)
            .await?;

        let event_id = self
            .timeline_service
            .registry()
            .send_live_message(
                &account.account_key,
                &room,
                outbound_room_message_content(body, request.formatted_body.as_deref()).into(),
            )
            .await?;

        if let Err(error) = self
            .timeline_service
            .registry()
            .mark_live_timeline_as_read(&account.account_key, &room)
            .await
        {
            eprintln!("Failed to mark sent room message as read: {error}");
        } else if event_id.starts_with('$') {
            mark_room_read_locally(
                self.timeline_service.locally_read_room_state(),
                &account.account_key,
                room.room_id().as_str(),
                &event_id,
            );
        }

        if !event_id.is_empty() {
            let title = room_title(&room)
                .await
                .unwrap_or_else(|_| room.room_id().to_string());
            let sent_item = crate::shell::types::RoomTimelineItem::text_message(
                event_id.clone(),
                room.own_user_id().to_string(),
                None,
                body.to_owned(),
                now_unix_ms(),
                false,
                true,
            );
            self.search_service
                .index_timeline_items(
                    &account.account_key,
                    &account.store_dir,
                    room.room_id().as_str(),
                    &title,
                    &[sent_item],
                )
                .await;
        }

        Ok(SendRoomMessageResponse { event_id })
    }

    pub async fn edit_room_message(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: EditRoomMessageRequest,
    ) -> Result<(), String> {
        account_manager.ensure_loaded(app).await?;
        let Some(account) = account_manager.active_account_client_loaded() else {
            return Err(String::from("No active account is available"));
        };

        let body = request.body.trim();
        if body.is_empty() {
            return Err(String::from("Message body must not be empty"));
        }

        let room = resolve_room(&account.client, &request.room_id)?;
        self.timeline_service
            .registry()
            .edit_live_message(
                &account.account_key,
                &room,
                &request.event_id,
                outbound_room_message_without_relation(body, request.formatted_body.as_deref()),
            )
            .await
    }

    pub async fn redact_room_message(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: RedactRoomMessageRequest,
    ) -> Result<(), String> {
        account_manager.ensure_loaded(app).await?;
        let Some(account) = account_manager.active_account_client_loaded() else {
            return Err(String::from("No active account is available"));
        };

        let room = resolve_room(&account.client, &request.room_id)?;
        self.timeline_service
            .registry()
            .redact_live_message(
                &account.account_key,
                &room,
                &request.event_id,
                request.reason.as_deref(),
            )
            .await
    }

    pub async fn reply_to_room_message(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: ReplyToRoomMessageRequest,
    ) -> Result<(), String> {
        account_manager.ensure_loaded(app).await?;
        let Some(account) = account_manager.active_account_client_loaded() else {
            return Err(String::from("No active account is available"));
        };

        let body = request.body.trim();
        if body.is_empty() {
            return Err(String::from("Message body must not be empty"));
        }

        let room = resolve_room(&account.client, &request.room_id)?;
        let event_id = EventId::parse(&request.event_id)
            .map_err(|error| format!("Invalid reply event id: {error}"))?
            .clone();
        self.timeline_service
            .registry()
            .reply_to_live_message(
                &account.account_key,
                &room,
                event_id,
                outbound_room_message_without_relation(body, request.formatted_body.as_deref()),
            )
            .await
    }

    pub async fn toggle_room_reaction(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: ToggleRoomReactionRequest,
    ) -> Result<ToggleRoomReactionResponse, String> {
        account_manager.ensure_loaded(app).await?;
        let Some(account) = account_manager.active_account_client_loaded() else {
            return Err(String::from("No active account is available"));
        };

        if request.reaction_key.trim().is_empty() {
            return Err(String::from("Reaction key must not be empty"));
        }

        let room = resolve_room(&account.client, &request.room_id)?;
        let added = self
            .timeline_service
            .registry()
            .toggle_live_reaction(
                &account.account_key,
                &room,
                &request.event_id,
                &request.reaction_key,
            )
            .await?;
        Ok(ToggleRoomReactionResponse { added })
    }

    pub async fn set_room_typing(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: SetRoomTypingRequest,
    ) -> Result<(), String> {
        account_manager.ensure_loaded(app).await?;
        let Some(account) = account_manager.active_account_client_loaded() else {
            return Err(String::from("No active account is available"));
        };

        let room = resolve_room(&account.client, &request.room_id)?;
        self.timeline_service
            .registry()
            .subscribe_typing_updates(app.clone(), &account.account_key, &room)
            .await?;
        self.timeline_service
            .registry()
            .send_typing_notice(&room, request.is_typing)
            .await
    }

    pub async fn list_spaces(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: ListSpacesRequest,
    ) -> Result<Vec<SpaceSummary>, String> {
        account_manager.ensure_loaded(app).await?;
        let Some(account) = account_manager.active_account_client_loaded() else {
            return Err(String::from("No active account is available"));
        };

        if !self
            .cache_state
            .space_cache_was_served(&account.account_key)
            && let Some(cached_spaces) =
                ShellCacheState::cached_spaces(&account.store_dir, &request)
        {
            self.cache_state
                .mark_space_cache_served(&account.account_key);
            self.ensure_sync_in_background(app, account_manager);
            return Ok(cached_spaces);
        }

        self.sync_manager
            .ensure_started_for_account(app, account_manager, account.clone())
            .await?;

        let query = normalize_query(request.search_query.as_deref());
        let mut spaces = Vec::new();
        let mut all_space_summaries = Vec::new();
        let mut active_space_ids = HashSet::new();
        let space_list_snapshot = self
            .snapshot_room_list(&account.account_key, ShellRoomListKind::Spaces)
            .await?;
        if space_list_snapshot.is_empty()
            && request.search_query.as_deref().is_none_or(str::is_empty)
            && let Some(cached_spaces) =
                ShellCacheState::cached_spaces(&account.store_dir, &request)
            && !cached_spaces.is_empty()
        {
            self.ensure_sync_in_background(app, account_manager);
            return Ok(cached_spaces);
        }

        for room in space_list_snapshot {
            let summary = self
                .build_space_summary(&room, &account.homeserver_url)
                .await?;
            active_space_ids.insert(summary.space_id.clone());
            self.search_service
                .index_space_summary(&account.account_key, &account.store_dir, &summary)
                .await;
            all_space_summaries.push(summary.clone());
            if matches_query(query.as_deref(), &[&summary.name, &summary.description]) {
                spaces.push(summary);
            }
        }

        self.search_service
            .tombstone_stale_space_documents(
                &account.account_key,
                &account.store_dir,
                &active_space_ids,
            )
            .await;
        ShellCacheState::remember_spaces(&account.store_dir, &all_space_summaries);

        Ok(spaces)
    }

    async fn build_room_thread_summary(
        &self,
        account_key: &str,
        room: &Room,
    ) -> Result<RoomThreadSummary, String> {
        let title = room_title(room).await?;
        let is_direct = room.is_direct().await.unwrap_or(false);
        let participant_label = participant_label(room, is_direct);
        let preview = latest_preview_text(room)
            .or_else(|| room.topic())
            .unwrap_or_default();
        let last_activity_unix_ms = latest_activity_unix_ms(room);
        let unread_count = unread_message_count_for_shell(
            self.timeline_service.locally_read_room_state(),
            account_key,
            room,
        )
        .await;
        let message_count = self.best_effort_message_count(room).await;

        Ok(RoomThreadSummary {
            room_id: room.room_id().to_string(),
            title: title.clone(),
            preview,
            participant_label,
            last_activity_unix_ms,
            last_activity_label: relative_time_label(last_activity_unix_ms),
            message_count,
            unread_count,
            homeserver_label: homeserver_label(room, room.client().homeserver().as_str()),
            avatar_label: first_visible_grapheme(&title),
            is_direct,
        })
    }

    async fn build_space_summary(
        &self,
        room: &Room,
        fallback_homeserver_url: &str,
    ) -> Result<SpaceSummary, String> {
        let name = room_title(room).await?;
        let description = room.topic().unwrap_or_default();
        let member_label = format!("{} members", room.active_members_count());
        let activity_timestamp = latest_activity_unix_ms(room);
        let activity_label = space_activity_label(activity_timestamp);

        Ok(SpaceSummary {
            space_id: room.room_id().to_string(),
            name: name.clone(),
            description,
            member_label,
            activity_label,
            accent_label: first_visible_grapheme(&name),
            is_official: Some(space_matches_homeserver(room, fallback_homeserver_url)),
        })
    }

    pub(super) async fn snapshot_room_list(
        &self,
        account_key: &str,
        list_kind: ShellRoomListKind,
    ) -> Result<Vec<Room>, String> {
        snapshot_room_list_for_account(&self.sync_manager, account_key, list_kind).await
    }

    async fn best_effort_message_count(&self, room: &Room) -> u64 {
        cached_timeline_item_count(room).await.unwrap_or(0) as u64
    }

    fn schedule_recent_timeline_warmup(
        &self,
        client: &matrix_sdk::Client,
        account_key: &str,
        store_dir: &std::path::Path,
        mut room_candidates: Vec<(String, u64)>,
    ) {
        room_candidates.sort_by_key(|room_candidate| Reverse(room_candidate.1));

        for (room_id, _activity_timestamp) in room_candidates
            .into_iter()
            .take(RECENT_TIMELINE_WARM_ROOM_COUNT)
        {
            self.timeline_service.schedule_room_timeline_warmup(
                client.clone(),
                account_key,
                store_dir,
                room_id,
                self.search_service.indexer_clone(),
            );
        }
    }
}

fn space_activity_label(activity_timestamp: u64) -> String {
    if activity_timestamp == 0 {
        return String::from("No recent activity");
    }

    relative_time_label(activity_timestamp)
}

fn outbound_room_message_content(
    body: &str,
    formatted_body: Option<&str>,
) -> RoomMessageEventContent {
    outbound_room_message_without_relation(body, formatted_body).into()
}

fn outbound_room_message_without_relation(
    body: &str,
    formatted_body: Option<&str>,
) -> RoomMessageEventContentWithoutRelation {
    let Some(formatted_body) = formatted_body
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return RoomMessageEventContentWithoutRelation::text_plain(body);
    };

    RoomMessageEventContentWithoutRelation::text_html(body, formatted_body)
}

fn space_matches_homeserver(room: &Room, fallback_homeserver_url: &str) -> bool {
    let Some(server_name) = room.room_id().server_name() else {
        return false;
    };

    fallback_homeserver_url.contains(server_name.as_str())
}

#[cfg(test)]
mod tests {
    use matrix_sdk::ruma::events::room::message::MessageType;

    use super::outbound_room_message_without_relation;

    #[test]
    fn outbound_message_uses_plain_text_without_formatted_body() {
        let content = outbound_room_message_without_relation("Hello", None);
        let MessageType::Text(text) = content.msgtype else {
            panic!("expected text message content");
        };

        assert_eq!(text.body, "Hello");
        assert!(text.formatted.is_none());
    }

    #[test]
    fn outbound_message_serializes_matrix_html_formatting() {
        let content =
            outbound_room_message_without_relation("**Oh yeah**", Some("<strong>Oh yeah</strong>"));
        let serialized = serde_json::to_value(content).expect("message content should serialize");

        assert_eq!(serialized["body"], "**Oh yeah**");
        assert_eq!(serialized["formatted_body"], "<strong>Oh yeah</strong>");
        assert_eq!(serialized["format"], "org.matrix.custom.html");
    }
}
