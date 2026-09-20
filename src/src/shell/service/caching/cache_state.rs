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

use crate::shell::types::{
    ListRoomThreadsRequest, ListSpacesRequest, RoomSummary, RoomThreadSummary, SpaceSummary,
};

#[derive(Clone, Default)]
pub struct ShellCacheState {
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
        _account_key: &str,
        _store_dir: &std::path::Path,
        _request: &ListRoomThreadsRequest,
    ) -> Option<Vec<RoomThreadSummary>> {
        None
    }

    pub(in crate::shell::service) fn cached_room_summary(
        _account_key: &str,
        _store_dir: &std::path::Path,
        _room_id: &str,
    ) -> Option<RoomSummary> {
        None
    }

    pub(in crate::shell::service) fn remember_room_threads(
        account_key: &str,
        store_dir: &std::path::Path,
        summaries: &[RoomThreadSummary],
    ) {
        let _ = (account_key, store_dir, summaries);
    }

    pub(in crate::shell::service) fn cached_spaces(
        store_dir: &std::path::Path,
        request: &ListSpacesRequest,
    ) -> Option<Vec<SpaceSummary>> {
        let _ = (store_dir, request);
        None
    }

    pub(in crate::shell::service) fn remember_spaces(
        store_dir: &std::path::Path,
        summaries: &[SpaceSummary],
    ) {
        let _ = (store_dir, summaries);
    }

    pub(in crate::shell::service) fn cached_room_timeline(
        _account_key: &str,
        _store_dir: &std::path::Path,
        _room_id: &str,
    ) -> Option<(Vec<crate::shell::types::RoomTimelineItem>, Option<String>)> {
        None
    }

    pub(in crate::shell::service) fn cached_room_timeline_item(
        _account_key: &str,
        _store_dir: &std::path::Path,
        _room_id: &str,
        _event_id: &str,
    ) -> Option<crate::shell::types::RoomTimelineItem> {
        None
    }

    pub(in crate::shell) fn merge_refreshed_timeline(
        account_key: &str,
        store_dir: &std::path::Path,
        room_id: &str,
        items: &[crate::shell::types::RoomTimelineItem],
        next_before: Option<&str>,
        redacted_event_ids: &[String],
    ) {
        let _ = (
            account_key,
            store_dir,
            room_id,
            items,
            next_before,
            redacted_event_ids,
        );
    }

    pub(in crate::shell::service) fn prepend_cached_timeline_items(
        account_key: &str,
        store_dir: &std::path::Path,
        room_id: &str,
        items: &[crate::shell::types::RoomTimelineItem],
        next_before: Option<&str>,
    ) {
        let _ = (account_key, store_dir, room_id, items, next_before);
    }

    pub(in crate::shell::service) fn remembered_timeline_item_count(
        account_key: &str,
        store_dir: &std::path::Path,
        room_id: &str,
    ) -> Option<usize> {
        let _ = (account_key, store_dir, room_id);
        None
    }

    pub(in crate::shell::service) fn remember_timeline_item_count_after_pagination(
        account_key: &str,
        store_dir: &std::path::Path,
        room_id: &str,
        before: Option<&str>,
        page_limit: u16,
        returned_item_count: usize,
    ) {
        let _ = (
            account_key,
            store_dir,
            room_id,
            before,
            page_limit,
            returned_item_count,
        );
    }

    fn room_cache_key(account_key: &str, room_id: &str) -> String {
        format!("{account_key}::{room_id}")
    }
}
