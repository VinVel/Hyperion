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
    collections::HashSet,
    sync::{Arc, RwLock},
};

use super::{
    cached_room_thread_summaries, cached_room_timeline, cached_room_timeline_item,
    cached_space_summaries, merge_cached_room_timeline_refresh, prepend_cached_room_timeline_items,
    remember_room_thread_summaries, remember_room_timeline_item_count, remember_space_summaries,
    remembered_room_timeline_item_count,
};
use crate::shell::{
    service::{
        paging::visible_count_after_live_page,
        search::{matches_query, normalize_query},
    },
    types::{
        ListRoomThreadsRequest, ListSpacesRequest, RoomSummary, RoomThreadSummary, SpaceSummary,
    },
};

#[derive(Clone, Default)]
pub(crate) struct ShellCacheState {
    room_thread_cache_served_accounts: Arc<RwLock<HashSet<String>>>,
    space_cache_served_accounts: Arc<RwLock<HashSet<String>>>,
    room_timeline_cache_served_keys: Arc<RwLock<HashSet<String>>>,
}

impl ShellCacheState {
    pub(in crate::shell::service) fn new() -> Self {
        Self::default()
    }

    pub(in crate::shell::service) fn room_thread_cache_was_served(
        &self,
        account_key: &str,
    ) -> bool {
        let served_accounts = self
            .room_thread_cache_served_accounts
            .read()
            .expect("shell manager room-cache-served lock poisoned");
        served_accounts.contains(account_key)
    }

    pub(in crate::shell::service) fn mark_room_thread_cache_served(&self, account_key: &str) {
        let mut served_accounts = self
            .room_thread_cache_served_accounts
            .write()
            .expect("shell manager room-cache-served lock poisoned");
        served_accounts.insert(account_key.to_owned());
    }

    pub(in crate::shell::service) fn clear_served_room_thread_cache(&self, account_key: &str) {
        let mut served_accounts = self
            .room_thread_cache_served_accounts
            .write()
            .expect("shell manager room-cache-served lock poisoned");
        served_accounts.remove(account_key);
    }

    pub(in crate::shell::service) fn clear_all_served_room_thread_caches(&self) {
        let mut served_accounts = self
            .room_thread_cache_served_accounts
            .write()
            .expect("shell manager room-cache-served lock poisoned");
        served_accounts.clear();
    }

    pub(in crate::shell::service) fn space_cache_was_served(&self, account_key: &str) -> bool {
        let served_accounts = self
            .space_cache_served_accounts
            .read()
            .expect("shell manager space-cache-served lock poisoned");
        served_accounts.contains(account_key)
    }

    pub(in crate::shell::service) fn mark_space_cache_served(&self, account_key: &str) {
        let mut served_accounts = self
            .space_cache_served_accounts
            .write()
            .expect("shell manager space-cache-served lock poisoned");
        served_accounts.insert(account_key.to_owned());
    }

    pub(in crate::shell::service) fn clear_served_space_cache(&self, account_key: &str) {
        let mut served_accounts = self
            .space_cache_served_accounts
            .write()
            .expect("shell manager space-cache-served lock poisoned");
        served_accounts.remove(account_key);
    }

    pub(in crate::shell::service) fn clear_all_served_space_caches(&self) {
        let mut served_accounts = self
            .space_cache_served_accounts
            .write()
            .expect("shell manager space-cache-served lock poisoned");
        served_accounts.clear();
    }

    pub(in crate::shell::service) fn room_timeline_cache_was_served(
        &self,
        account_key: &str,
        room_id: &str,
    ) -> bool {
        let served_keys = self
            .room_timeline_cache_served_keys
            .read()
            .expect("shell manager timeline-cache-served lock poisoned");
        served_keys.contains(&Self::room_cache_key(account_key, room_id))
    }

    pub(in crate::shell::service) fn mark_room_timeline_cache_served(
        &self,
        account_key: &str,
        room_id: &str,
    ) {
        let mut served_keys = self
            .room_timeline_cache_served_keys
            .write()
            .expect("shell manager timeline-cache-served lock poisoned");
        served_keys.insert(Self::room_cache_key(account_key, room_id));
    }

    pub(in crate::shell::service) fn clear_served_room_timeline_caches(&self, account_key: &str) {
        let account_prefix = format!("{account_key}::");
        let mut served_keys = self
            .room_timeline_cache_served_keys
            .write()
            .expect("shell manager timeline-cache-served lock poisoned");
        served_keys.retain(|cache_key| !cache_key.starts_with(&account_prefix));
    }

    pub(in crate::shell::service) fn clear_all_served_room_timeline_caches(&self) {
        let mut served_keys = self
            .room_timeline_cache_served_keys
            .write()
            .expect("shell manager timeline-cache-served lock poisoned");
        served_keys.clear();
    }

    pub(in crate::shell::service) fn cached_room_threads(
        account_key: &str,
        store_dir: &std::path::Path,
        request: &ListRoomThreadsRequest,
    ) -> Option<Vec<RoomThreadSummary>> {
        let cached_rooms = match cached_room_thread_summaries(account_key, store_dir) {
            Ok(cached_rooms) => cached_rooms,
            Err(error) => {
                eprintln!("Failed to read cached room list: {error}");
                return None;
            }
        };
        if cached_rooms.is_empty() {
            return None;
        }

        let query = normalize_query(request.search_query.as_deref());
        Some(
            cached_rooms
                .into_iter()
                .filter(|summary| {
                    matches_query(
                        query.as_deref(),
                        &[&summary.title, &summary.preview, &summary.participant_label],
                    )
                })
                .collect(),
        )
    }

    pub(in crate::shell::service) fn cached_room_summary(
        account_key: &str,
        store_dir: &std::path::Path,
        room_id: &str,
    ) -> Option<RoomSummary> {
        let cached_rooms = match cached_room_thread_summaries(account_key, store_dir) {
            Ok(cached_rooms) => cached_rooms,
            Err(error) => {
                eprintln!("Failed to read cached room summary: {error}");
                return None;
            }
        };
        let room = cached_rooms
            .into_iter()
            .find(|room| room.room_id == room_id)?;

        Some(RoomSummary {
            room_id: room.room_id,
            title: room.title,
            participant_label: room.participant_label,
            homeserver_label: room.homeserver_label,
            topic: None,
            is_direct: room.is_direct,
            can_send_messages: true,
        })
    }

    pub(in crate::shell::service) fn remember_room_threads(
        account_key: &str,
        store_dir: &std::path::Path,
        summaries: &[RoomThreadSummary],
    ) {
        if let Err(error) = remember_room_thread_summaries(account_key, store_dir, summaries) {
            eprintln!("Failed to persist cached room list: {error}");
        }
    }

    pub(in crate::shell::service) fn cached_spaces(
        store_dir: &std::path::Path,
        request: &ListSpacesRequest,
    ) -> Option<Vec<SpaceSummary>> {
        let cached_spaces = match cached_space_summaries(store_dir) {
            Ok(cached_spaces) => cached_spaces,
            Err(error) => {
                eprintln!("Failed to read cached spaces: {error}");
                return None;
            }
        };
        if cached_spaces.is_empty() {
            return None;
        }

        let query = normalize_query(request.search_query.as_deref());
        Some(
            cached_spaces
                .into_iter()
                .filter(|summary| {
                    matches_query(query.as_deref(), &[&summary.name, &summary.description])
                })
                .collect(),
        )
    }

    pub(in crate::shell::service) fn remember_spaces(
        store_dir: &std::path::Path,
        summaries: &[SpaceSummary],
    ) {
        if let Err(error) = remember_space_summaries(store_dir, summaries) {
            eprintln!("Failed to persist cached spaces: {error}");
        }
    }

    pub(in crate::shell::service) fn cached_room_timeline(
        account_key: &str,
        store_dir: &std::path::Path,
        room_id: &str,
    ) -> Option<(Vec<crate::shell::types::RoomTimelineItem>, Option<String>)> {
        match cached_room_timeline(account_key, store_dir, room_id) {
            Ok(timeline) => timeline,
            Err(error) => {
                eprintln!("Failed to read cached room timeline: {error}");
                None
            }
        }
    }

    pub(in crate::shell::service) fn cached_room_timeline_item(
        account_key: &str,
        store_dir: &std::path::Path,
        room_id: &str,
        event_id: &str,
    ) -> Option<crate::shell::types::RoomTimelineItem> {
        match cached_room_timeline_item(account_key, store_dir, room_id, event_id) {
            Ok(item) => item,
            Err(error) => {
                eprintln!("Failed to read cached room timeline item: {error}");
                None
            }
        }
    }

    pub(in crate::shell) fn merge_refreshed_timeline(
        account_key: &str,
        store_dir: &std::path::Path,
        room_id: &str,
        items: &[crate::shell::types::RoomTimelineItem],
        next_before: Option<&str>,
        redacted_event_ids: &[String],
    ) {
        if let Err(error) = merge_cached_room_timeline_refresh(
            account_key,
            store_dir,
            room_id,
            items,
            next_before,
            redacted_event_ids,
        ) {
            eprintln!("Failed to merge refreshed room timeline cache: {error}");
        }
    }

    pub(in crate::shell::service) fn prepend_cached_timeline_items(
        account_key: &str,
        store_dir: &std::path::Path,
        room_id: &str,
        items: &[crate::shell::types::RoomTimelineItem],
        next_before: Option<&str>,
    ) {
        if let Err(error) =
            prepend_cached_room_timeline_items(account_key, store_dir, room_id, items, next_before)
        {
            eprintln!("Failed to merge cached room timeline items: {error}");
        }
    }

    pub(in crate::shell::service) fn remembered_timeline_item_count(
        account_key: &str,
        store_dir: &std::path::Path,
        room_id: &str,
    ) -> Option<usize> {
        match remembered_room_timeline_item_count(account_key, store_dir, room_id) {
            Ok(count) => count,
            Err(error) => {
                eprintln!("Failed to read timeline view cache state: {error}");
                None
            }
        }
    }

    pub(in crate::shell::service) fn remember_timeline_item_count_after_pagination(
        account_key: &str,
        store_dir: &std::path::Path,
        room_id: &str,
        before: Option<&str>,
        page_limit: u16,
        returned_item_count: usize,
    ) {
        let Some(visible_item_count) =
            visible_count_after_live_page(before, page_limit, returned_item_count)
        else {
            return;
        };

        if let Err(error) =
            remember_room_timeline_item_count(account_key, store_dir, room_id, visible_item_count)
        {
            eprintln!("Failed to persist timeline view cache state: {error}");
        }
    }

    fn room_cache_key(account_key: &str, room_id: &str) -> String {
        format!("{account_key}::{room_id}")
    }
}
