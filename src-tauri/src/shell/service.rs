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

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
    time::Duration,
};

use matrix_sdk::{
    Room,
    ruma::{
        EventId,
        api::client::{filter::RoomEventFilter, search::search_events},
        events::room::message::RoomMessageEventContent,
    },
};

use crate::account::AccountManager;

mod room;
mod search;
mod timeline;

use self::{
    room::{
        can_send_messages, current_latest_event_id, homeserver_label, local_room_state_key,
        participant_label, persisted_read_anchor_event_id, resolve_room, room_is_encrypted,
        room_title, unread_message_count,
    },
    search::{
        first_visible_grapheme, matches_query, message_search_hit, normalize_query, now_unix_ms,
        push_message_hits, relative_time_label, server_backed_search_hit,
    },
    timeline::{
        cached_timeline_item_count, cached_timeline_items, count_unread_messages_since,
        extract_message_body_from_raw, fetch_room_timeline_chunk, mark_room_as_read,
        timeline_item_from_timeline_event, warm_room_recent_timeline,
    },
};

use super::{
    sync::ShellSyncManager,
    types::{
        GetRoomEventContextRequest, GetRoomSummaryRequest, GetRoomTimelineRequest,
        GlobalSearchMessageHit, GlobalSearchRequest, GlobalSearchResponse, GlobalSearchRoomHit,
        GlobalSearchSpaceHit, ListRoomThreadsRequest, ListSpacesRequest, RoomSummary,
        RoomThreadSummary, RoomTimeline, RoomTimelineItem, SendRoomMessageRequest,
        SendRoomMessageResponse, SpaceSummary,
    },
};

// The default room-open page should feel immediate, but still show enough
// surrounding context that the user does not land in a "single screen" view.
const DEFAULT_TIMELINE_LIMIT: u16 = 30;
// Event-context jumps are meant to anchor the user around a hit, not replay a
// full timeline page, so keep the context window smaller than the normal page.
const DEFAULT_EVENT_CONTEXT_LIMIT: u16 = 8;
// Search groups back the current shell UI; keeping them short avoids turning a
// lightweight command into a broad fan-out over every room on each keystroke.
const DEFAULT_SEARCH_LIMIT_PER_GROUP: usize = 5;
// Recent-message fallback search should inspect enough history to be useful,
// but remain bounded so local scans stay interactive on large accounts.
const MESSAGE_SEARCH_SCAN_LIMIT: u16 = 20;
// Search backfills only a couple of pages before giving up, because this path
// is a best-effort fallback after the local index and cache have already run.
const MESSAGE_SEARCH_MAX_PAGES: usize = 2;
// Per-room search hits are capped so one noisy room does not crowd out the
// global search results before other joined rooms get a chance to contribute.
const MESSAGE_SEARCH_HITS_PER_ROOM: usize = 5;
// Server-backed search is reserved for larger accounts where walking local room
// history becomes more expensive than asking the homeserver for non-E2EE rooms.
const SERVER_BACKED_SEARCH_ROOM_THRESHOLD: usize = 20;
// Warm a meaningfully larger local window than the visible timeline so recently
// reopened rooms can render from disk/cache without fetching again immediately.
const RECENT_TIMELINE_WARM_LIMIT: u16 = 80;
// Only warm the most recently active rooms in background; broad warmups would
// compete with sync and make multi-room accounts more expensive than needed.
const RECENT_TIMELINE_WARM_ROOM_COUNT: usize = 6;
// Rewarm infrequently enough to avoid churn, but often enough that active rooms
// keep a recent local window available across normal shell navigation.
const RECENT_TIMELINE_REWARM_INTERVAL_MS: u64 = Duration::from_secs(10 * 60).as_millis() as u64;

#[derive(Clone)]
struct SearchableRoom {
    room: Room,
    title: String,
    is_encrypted: bool,
}

#[derive(Clone, Default)]
pub struct ShellManager {
    sync_manager: ShellSyncManager,
    recent_timeline_warm_state: Arc<RwLock<HashMap<String, u64>>>,
    locally_read_room_state: Arc<RwLock<HashMap<String, String>>>,
}

impl ShellManager {
    pub fn new() -> Self {
        Self {
            sync_manager: ShellSyncManager::new(),
            recent_timeline_warm_state: Arc::new(RwLock::new(HashMap::new())),
            locally_read_room_state: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn list_room_threads(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: ListRoomThreadsRequest,
    ) -> Result<Vec<RoomThreadSummary>, String> {
        self.sync_manager
            .ensure_started_for_manager(account_manager, app)
            .await?;

        let Some(account) = account_manager.active_account_client(app).await? else {
            return Ok(Vec::new());
        };

        let query = normalize_query(request.search_query.as_deref());
        let mut rooms = Vec::new();
        let mut recent_room_candidates = Vec::new();

        for room in account.client.joined_rooms() {
            if room.is_space() {
                continue;
            }

            let summary = self
                .build_room_thread_summary(&account.account_key, &room)
                .await?;
            recent_room_candidates.push((summary.room_id.clone(), summary.last_activity_unix_ms));
            if matches_query(
                query.as_deref(),
                &[&summary.title, &summary.preview, &summary.participant_label],
            ) {
                rooms.push(summary);
            }
        }

        self.schedule_recent_timeline_warmup(
            &account.client,
            &account.account_key,
            recent_room_candidates,
        );

        Ok(rooms)
    }

    pub async fn get_room_summary(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: GetRoomSummaryRequest,
    ) -> Result<RoomSummary, String> {
        self.sync_manager
            .ensure_started_for_manager(account_manager, app)
            .await?;

        let Some(account) = account_manager.active_account_client(app).await? else {
            return Err(String::from("No active account is available"));
        };
        let room = resolve_room(&account.client, &request.room_id)?;
        self.mark_room_focused(&account.account_key, room.room_id().to_string());

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

    pub async fn get_room_timeline(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: GetRoomTimelineRequest,
    ) -> Result<RoomTimeline, String> {
        self.sync_manager
            .ensure_started_for_manager(account_manager, app)
            .await?;

        let Some(account) = account_manager.active_account_client(app).await? else {
            return Err(String::from("No active account is available"));
        };
        let room = resolve_room(&account.client, &request.room_id)?;
        self.mark_room_focused(&account.account_key, room.room_id().to_string());
        self.schedule_room_timeline_warmup(
            account.client.clone(),
            account.account_key.clone(),
            room.room_id().to_string(),
        );

        let limit = request.limit.unwrap_or(DEFAULT_TIMELINE_LIMIT);
        let (items, next_before) = if request.before.is_none() {
            self.load_latest_room_timeline(&room, limit).await?
        } else {
            self.load_paginated_room_timeline(&room, limit, request.before.as_deref())
                .await?
        };

        if request.before.is_none()
            && let Some(latest_item) = items.last()
        {
            mark_room_as_read(&room, &latest_item.event_id).await?;
            self.mark_room_read_locally(
                &account.account_key,
                room.room_id().as_str(),
                &latest_item.event_id,
            );
        }

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
        self.mark_room_focused(&account.account_key, room.room_id().to_string());

        let event_id = EventId::parse(&request.event_id)
            .map_err(|error| format!("Invalid event id: {error}"))?;
        let context_limit = u32::from(request.context_limit.unwrap_or(DEFAULT_EVENT_CONTEXT_LIMIT));
        let context = room
            .event_with_context(&event_id, true, context_limit.into(), None)
            .await
            .map_err(|error| format!("Failed to load event context: {error}"))?;

        let mut items = context
            .events_before
            .iter()
            .rev()
            .filter_map(|event| timeline_item_from_timeline_event(event, room.own_user_id()))
            .collect::<Vec<_>>();

        if let Some(event) = &context.event
            && let Some(item) = timeline_item_from_timeline_event(event, room.own_user_id())
        {
            items.push(item);
        }

        items.extend(
            context
                .events_after
                .iter()
                .filter_map(|event| timeline_item_from_timeline_event(event, room.own_user_id())),
        );

        Ok(RoomTimeline {
            room_id: room.room_id().to_string(),
            items,
            next_before: context.prev_batch_token,
            focused_event_id: Some(request.event_id),
        })
    }

    pub async fn send_room_message(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: SendRoomMessageRequest,
    ) -> Result<SendRoomMessageResponse, String> {
        self.sync_manager
            .ensure_started_for_manager(account_manager, app)
            .await?;

        let body = request.body.trim();
        if body.is_empty() {
            return Err(String::from("Message body must not be empty"));
        }

        let Some(account) = account_manager.active_account_client(app).await? else {
            return Err(String::from("No active account is available"));
        };
        let room = resolve_room(&account.client, &request.room_id)?;
        self.mark_room_focused(&account.account_key, room.room_id().to_string());

        let response = room
            .send(RoomMessageEventContent::text_plain(body))
            .await
            .map_err(|error| format!("Failed to send the room message: {error}"))?;

        mark_room_as_read(&room, response.event_id.as_str()).await?;
        self.mark_room_read_locally(
            &account.account_key,
            room.room_id().as_str(),
            response.event_id.as_str(),
        );

        Ok(SendRoomMessageResponse {
            event_id: response.event_id.to_string(),
        })
    }

    pub async fn list_spaces(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: ListSpacesRequest,
    ) -> Result<Vec<SpaceSummary>, String> {
        self.sync_manager
            .ensure_started_for_manager(account_manager, app)
            .await?;

        let Some(account) = account_manager.active_account_client(app).await? else {
            return Ok(Vec::new());
        };

        let query = normalize_query(request.search_query.as_deref());
        let mut spaces = Vec::new();
        for room in account.client.joined_space_rooms() {
            let summary = self
                .build_space_summary(&room, &account.homeserver_url)
                .await?;
            if matches_query(query.as_deref(), &[&summary.name, &summary.description]) {
                spaces.push(summary);
            }
        }

        Ok(spaces)
    }

    pub async fn global_search(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: GlobalSearchRequest,
    ) -> Result<GlobalSearchResponse, String> {
        self.sync_manager
            .ensure_started_for_manager(account_manager, app)
            .await?;

        let query = request.query.trim().to_lowercase();
        if query.is_empty() {
            return Ok(GlobalSearchResponse {
                rooms: Vec::new(),
                spaces: Vec::new(),
                messages: Vec::new(),
            });
        }

        let Some(account) = account_manager.active_account_client(app).await? else {
            return Ok(GlobalSearchResponse {
                rooms: Vec::new(),
                spaces: Vec::new(),
                messages: Vec::new(),
            });
        };

        let limit = request
            .limit_per_group
            .unwrap_or(DEFAULT_SEARCH_LIMIT_PER_GROUP);

        let mut rooms = Vec::new();
        let mut spaces = Vec::new();
        let mut messages = Vec::new();
        let mut searchable_rooms = Vec::new();

        for room in account.client.joined_rooms() {
            if room.is_space() {
                if spaces.len() < limit {
                    let summary = self
                        .build_space_summary(&room, &account.homeserver_url)
                        .await?;
                    if matches_query(Some(&query), &[&summary.name, &summary.description]) {
                        spaces.push(GlobalSearchSpaceHit {
                            space_id: summary.space_id,
                            title: summary.name,
                            description: summary.description,
                        });
                    }
                }
                continue;
            }

            let summary = self
                .build_room_thread_summary(&account.account_key, &room)
                .await?;
            if rooms.len() < limit
                && matches_query(Some(&query), &[&summary.title, &summary.preview])
            {
                rooms.push(GlobalSearchRoomHit {
                    room_id: summary.room_id.clone(),
                    title: summary.title.clone(),
                    description: summary.preview.clone(),
                });
            }

            searchable_rooms.push(SearchableRoom {
                is_encrypted: room_is_encrypted(&room).await,
                room,
                title: summary.title,
            });
        }

        let large_account = searchable_rooms.len() >= SERVER_BACKED_SEARCH_ROOM_THRESHOLD;
        let mut seen_message_ids = HashSet::new();

        if large_account {
            let server_hits = self
                .server_backed_message_search(&account.client, &searchable_rooms, &query, limit)
                .await?;
            push_message_hits(&mut messages, &mut seen_message_ids, server_hits, limit);
        }

        for searchable_room in &searchable_rooms {
            if messages.len() >= limit {
                break;
            }

            if !searchable_room.is_encrypted && large_account {
                continue;
            }

            let scan = self
                .indexed_message_search(
                    &searchable_room.room,
                    &searchable_room.title,
                    &query,
                    limit.saturating_sub(messages.len()),
                )
                .await?;
            push_message_hits(&mut messages, &mut seen_message_ids, scan, limit);
        }

        if large_account && messages.len() < limit {
            for searchable_room in &searchable_rooms {
                if searchable_room.is_encrypted || messages.len() >= limit {
                    continue;
                }

                let fallback_hits = self
                    .indexed_message_search(
                        &searchable_room.room,
                        &searchable_room.title,
                        &query,
                        limit.saturating_sub(messages.len()),
                    )
                    .await?;
                push_message_hits(&mut messages, &mut seen_message_ids, fallback_hits, limit);
            }
        }

        Ok(GlobalSearchResponse {
            rooms,
            spaces,
            messages,
        })
    }

    async fn build_room_thread_summary(
        &self,
        account_key: &str,
        room: &Room,
    ) -> Result<RoomThreadSummary, String> {
        let title = room_title(room).await?;
        let is_direct = room.is_direct().await.unwrap_or(false);
        let participant_label = participant_label(room, is_direct);
        let latest_event = room.latest_event();
        let preview = latest_event
            .as_ref()
            .and_then(|event| extract_message_body_from_raw(event.event().raw()))
            .or_else(|| room.topic())
            .unwrap_or_default();
        let last_activity_unix_ms = latest_event
            .as_ref()
            .and_then(|event| event.event().timestamp())
            .map(|timestamp| timestamp.0.into())
            .unwrap_or_default();
        let unread_count = self.unread_message_count(account_key, room).await;
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
        let activity_timestamp = room
            .latest_event()
            .and_then(|event| event.event().timestamp())
            .map(|timestamp| u64::from(timestamp.0))
            .unwrap_or_default();
        let activity_label = if activity_timestamp == 0 {
            String::from("No recent activity")
        } else {
            relative_time_label(activity_timestamp)
        };

        Ok(SpaceSummary {
            space_id: room.room_id().to_string(),
            name: name.clone(),
            description,
            member_label,
            activity_label,
            accent_label: first_visible_grapheme(&name),
            is_official: Some(
                room.room_id()
                    .server_name()
                    .map(|server_name| fallback_homeserver_url.contains(server_name.as_str()))
                    .unwrap_or(false),
            ),
        })
    }

    async fn best_effort_message_count(&self, room: &Room) -> u64 {
        cached_timeline_item_count(room).await.unwrap_or(0) as u64
    }

    fn schedule_recent_timeline_warmup(
        &self,
        client: &matrix_sdk::Client,
        account_key: &str,
        mut room_candidates: Vec<(String, u64)>,
    ) {
        room_candidates.sort_by(|left, right| right.1.cmp(&left.1));

        for (room_id, _) in room_candidates
            .into_iter()
            .take(RECENT_TIMELINE_WARM_ROOM_COUNT)
        {
            self.schedule_room_timeline_warmup(client.clone(), account_key.to_owned(), room_id);
        }
    }

    fn schedule_room_timeline_warmup(
        &self,
        client: matrix_sdk::Client,
        account_key: String,
        room_id: String,
    ) {
        let now = now_unix_ms();
        let state_key = format!("{account_key}::{room_id}");

        {
            let mut warm_state = self
                .recent_timeline_warm_state
                .write()
                .expect("shell manager warm-state lock poisoned");
            if warm_state.get(&state_key).is_some_and(|previous_warm_at| {
                now.saturating_sub(*previous_warm_at) < RECENT_TIMELINE_REWARM_INTERVAL_MS
            }) {
                return;
            }

            warm_state.insert(state_key.clone(), now);
        }

        tauri::async_runtime::spawn(async move {
            if let Err(error) =
                warm_room_recent_timeline(&client, &room_id, RECENT_TIMELINE_WARM_LIMIT).await
            {
                eprintln!("Failed to warm recent room timeline for {room_id}: {error}");
            }
        });
    }

    async fn indexed_message_search(
        &self,
        room: &Room,
        room_title: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<GlobalSearchMessageHit>, String> {
        let mut hits = self
            .local_index_hits(room, room_title, query, limit)
            .await?;
        if hits.len() >= limit {
            return Ok(hits);
        }

        let fallback_hits = self
            .scan_room_messages_for_search(
                room,
                room_title,
                query,
                limit.saturating_sub(hits.len()),
            )
            .await?;
        hits.extend(fallback_hits);

        Ok(hits)
    }

    async fn local_index_hits(
        &self,
        room: &Room,
        room_title: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<GlobalSearchMessageHit>, String> {
        let event_ids = room
            .search(query, limit, None)
            .await
            .map_err(|error| format!("Failed to search the local room index: {error}"))?;

        let mut hits = Vec::new();
        let mut seen_event_ids = HashSet::new();

        for event_id in event_ids {
            let event = room
                .load_or_fetch_event(&event_id, None)
                .await
                .map_err(|error| format!("Failed to load an indexed message match: {error}"))?;

            let Some(item) = timeline_item_from_timeline_event(&event, room.own_user_id()) else {
                continue;
            };

            if !item.body.to_lowercase().contains(query) {
                continue;
            }

            if seen_event_ids.insert(item.event_id.clone()) {
                hits.push(message_search_hit(room, room_title, item));
            }

            if hits.len() >= limit {
                return Ok(hits);
            }
        }

        Ok(hits)
    }

    async fn scan_room_messages_for_search(
        &self,
        room: &Room,
        room_title: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<GlobalSearchMessageHit>, String> {
        let mut hits = Vec::new();
        let mut seen_event_ids = HashSet::new();

        // The SDK's event cache is the intended local source for recent room
        // history, so search it before paying for explicit pagination.
        for item in cached_timeline_items(room).await? {
            if !item.body.to_lowercase().contains(query) {
                continue;
            }

            if seen_event_ids.insert(item.event_id.clone()) {
                hits.push(message_search_hit(room, room_title, item));
            }

            if hits.len() >= limit.min(MESSAGE_SEARCH_HITS_PER_ROOM) {
                return Ok(hits);
            }
        }

        let mut before: Option<String> = None;
        for _ in 0..MESSAGE_SEARCH_MAX_PAGES {
            let (chunk, next_before) =
                fetch_room_timeline_chunk(room, MESSAGE_SEARCH_SCAN_LIMIT, before.as_deref())
                    .await
                    .map_err(|error| format!("Failed to search the room timeline: {error}"))?;
            before = next_before;

            for item in chunk {
                if !item.body.to_lowercase().contains(query) {
                    continue;
                }

                if seen_event_ids.insert(item.event_id.clone()) {
                    hits.push(message_search_hit(room, room_title, item));
                }

                if hits.len() >= limit.min(MESSAGE_SEARCH_HITS_PER_ROOM) {
                    return Ok(hits);
                }
            }

            if before.is_none() {
                break;
            }
        }

        Ok(hits)
    }

    async fn load_latest_room_timeline(
        &self,
        room: &Room,
        limit: u16,
    ) -> Result<(Vec<RoomTimelineItem>, Option<String>), String> {
        // Prefer the SDK event cache for the visible latest slice. It is the
        // intended local observer for recent room history and avoids turning
        // every room open into a network request.
        let cached_items = cached_timeline_items(room).await?;
        if cached_items.len() >= usize::from(limit) {
            let items = cached_items[cached_items.len() - usize::from(limit)..].to_vec();
            return Ok((items, room.last_prev_batch()));
        }

        let fetch_limit = limit.max(RECENT_TIMELINE_WARM_LIMIT);
        let (mut items, next_before) = fetch_room_timeline_chunk(room, fetch_limit, None).await?;
        if items.len() > usize::from(limit) {
            items = items.split_off(items.len() - usize::from(limit));
        }

        Ok((items, next_before))
    }

    async fn load_paginated_room_timeline(
        &self,
        room: &Room,
        limit: u16,
        before: Option<&str>,
    ) -> Result<(Vec<RoomTimelineItem>, Option<String>), String> {
        fetch_room_timeline_chunk(room, limit, before).await
    }

    fn mark_room_focused(&self, account_key: &str, _room_id: String) {
        self.sync_manager.touch_focused_room(account_key, &_room_id);
    }

    fn mark_room_read_locally(&self, account_key: &str, room_id: &str, event_id: &str) {
        self.locally_read_room_state
            .write()
            .expect("shell manager locally-read-room-state lock poisoned")
            .insert(
                local_room_state_key(account_key, room_id),
                event_id.to_owned(),
            );
    }

    async fn unread_message_count(&self, account_key: &str, room: &Room) -> u64 {
        let Some(read_anchor_event_id) = self.read_anchor_event_id(account_key, room).await else {
            return unread_message_count(room);
        };

        let Some(latest_event_id) = current_latest_event_id(room) else {
            return 0;
        };

        if latest_event_id == read_anchor_event_id {
            return 0;
        }

        if let Some(local_unread_count) =
            count_unread_messages_since(room, &read_anchor_event_id).await
        {
            return local_unread_count.max(1);
        }

        unread_message_count(room)
    }

    async fn read_anchor_event_id(&self, account_key: &str, room: &Room) -> Option<String> {
        let local_anchor_event_id = self
            .locally_read_room_state
            .read()
            .expect("shell manager locally-read-room-state lock poisoned")
            .get(&local_room_state_key(account_key, room.room_id().as_str()))
            .cloned();
        let persisted_anchor_event_id = persisted_read_anchor_event_id(room).await;

        let current_latest_event_id = current_latest_event_id(room);
        if current_latest_event_id.is_some() && persisted_anchor_event_id == current_latest_event_id
        {
            return persisted_anchor_event_id;
        }

        if current_latest_event_id.is_some() && local_anchor_event_id == current_latest_event_id {
            return local_anchor_event_id;
        }

        persisted_anchor_event_id.or(local_anchor_event_id)
    }

    async fn server_backed_message_search(
        &self,
        client: &matrix_sdk::Client,
        searchable_rooms: &[SearchableRoom],
        query: &str,
        limit: usize,
    ) -> Result<Vec<GlobalSearchMessageHit>, String> {
        let searchable_room_ids = searchable_rooms
            .iter()
            .filter(|room| !room.is_encrypted)
            .map(|room| room.room.room_id().to_owned())
            .collect::<Vec<_>>();

        if searchable_room_ids.is_empty() {
            return Ok(Vec::new());
        }

        let room_titles = searchable_rooms
            .iter()
            .map(|room| (room.room.room_id().to_string(), room.title.clone()))
            .collect::<HashMap<_, _>>();

        let mut filter = RoomEventFilter::default();
        filter.rooms = Some(searchable_room_ids);
        filter.limit = Some(u32::try_from(limit).unwrap_or(u32::MAX).into());

        let mut criteria = search_events::v3::Criteria::new(query.to_owned());
        criteria.keys = Some(vec![search_events::v3::SearchKeys::ContentBody]);
        criteria.filter = filter;
        criteria.order_by = Some(search_events::v3::OrderBy::Recent);

        let mut categories = search_events::v3::Categories::new();
        categories.room_events = Some(criteria);

        let response = client
            .send(search_events::v3::Request::new(categories))
            .await
            .map_err(|error| format!("Failed to execute server-backed message search: {error}"))?;

        let mut hits = Vec::new();
        for result in response.search_categories.room_events.results {
            let Some(raw_event) = result.result else {
                continue;
            };

            let Some(hit) = server_backed_search_hit(&raw_event, &room_titles, query) else {
                continue;
            };

            hits.push(hit);
            if hits.len() >= limit {
                break;
            }
        }

        Ok(hits)
    }
}
