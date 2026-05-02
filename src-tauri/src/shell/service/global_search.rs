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

use std::collections::{HashMap, HashSet};

use matrix_sdk::{
    Client, Room,
    ruma::api::client::{filter::RoomEventFilter, search::search_events},
};

use crate::account::AccountManager;

use super::{
    ShellManager, ShellRoomListKind,
    room::room_is_encrypted,
    search::{message_search_hit, push_message_hits, server_backed_search_hit},
    timeline::{
        cached_timeline_items, fetch_room_timeline_chunk, timeline_item_from_timeline_event,
    },
};
use crate::shell::types::{
    GlobalSearchMessageHit, GlobalSearchRequest, GlobalSearchResponse, GlobalSearchRoomHit,
    GlobalSearchSpaceHit,
};

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

#[derive(Clone)]
struct SearchableRoom {
    room: Room,
    title: String,
    is_encrypted: bool,
}

impl ShellManager {
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

        for room in self
            .snapshot_room_list(&account.account_key, ShellRoomListKind::Spaces)
            .await?
        {
            if spaces.len() >= limit {
                break;
            }

            let summary = self
                .build_space_summary(&room, &account.homeserver_url)
                .await?;
            if super::search::matches_query(Some(&query), &[&summary.name, &summary.description]) {
                spaces.push(GlobalSearchSpaceHit {
                    space_id: summary.space_id,
                    title: summary.name,
                    description: summary.description,
                });
            }
        }

        for room in self
            .snapshot_room_list(&account.account_key, ShellRoomListKind::Conversations)
            .await?
        {
            let summary = self
                .build_room_thread_summary(&account.account_key, &room)
                .await?;
            if rooms.len() < limit
                && super::search::matches_query(Some(&query), &[&summary.title, &summary.preview])
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

        self.collect_global_search_messages(
            &account.client,
            &searchable_rooms,
            &query,
            limit,
            &mut messages,
        )
        .await?;

        Ok(GlobalSearchResponse {
            rooms,
            spaces,
            messages,
        })
    }

    async fn collect_global_search_messages(
        &self,
        client: &Client,
        searchable_rooms: &[SearchableRoom],
        query: &str,
        limit: usize,
        messages: &mut Vec<GlobalSearchMessageHit>,
    ) -> Result<(), String> {
        let large_account = searchable_rooms.len() >= SERVER_BACKED_SEARCH_ROOM_THRESHOLD;
        let mut seen_message_ids = HashSet::new();

        if large_account {
            let server_hits = self
                .server_backed_message_search(client, searchable_rooms, query, limit)
                .await?;
            push_message_hits(messages, &mut seen_message_ids, server_hits, limit);
        }

        for searchable_room in searchable_rooms {
            if messages.len() >= limit {
                break;
            }

            if large_account && !searchable_room.is_encrypted {
                continue;
            }

            let scan = self
                .indexed_message_search(
                    &searchable_room.room,
                    &searchable_room.title,
                    query,
                    limit.saturating_sub(messages.len()),
                )
                .await?;
            push_message_hits(messages, &mut seen_message_ids, scan, limit);
        }

        if !large_account || messages.len() >= limit {
            return Ok(());
        }

        for searchable_room in searchable_rooms {
            if searchable_room.is_encrypted || messages.len() >= limit {
                continue;
            }

            let fallback_hits = self
                .indexed_message_search(
                    &searchable_room.room,
                    &searchable_room.title,
                    query,
                    limit.saturating_sub(messages.len()),
                )
                .await?;
            push_message_hits(messages, &mut seen_message_ids, fallback_hits, limit);
        }

        Ok(())
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

    async fn server_backed_message_search(
        &self,
        client: &Client,
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
            .map(|room| {
                let room_id = room.room.room_id().to_string();
                (room_id, room.title.clone())
            })
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

        let request = search_events::v3::Request::new(categories);
        let response = client
            .send(request)
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
