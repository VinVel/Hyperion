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
    path::Path,
    sync::{Arc, RwLock},
};

use tauri::async_runtime::JoinHandle;

use super::{
    RECENT_TIMELINE_REWARM_INTERVAL_MS, RECENT_TIMELINE_WARM_LIMIT,
    room::{
        resolve_room, room_title,
        timeline::{cached_timeline_items, warm_room_recent_timeline},
    },
    search::{SearchBackfillCoordinator, SearchIndexer},
};
use crate::{
    shell::{engine::ShellTimelineRegistry, types::RoomThreadSummary},
    utils::time::now_unix_ms,
};

#[derive(Clone, Default)]
pub(super) struct ShellTimelineService {
    registry: ShellTimelineRegistry,
    recent_warm_state: Arc<RwLock<HashMap<String, u64>>>,
    recent_warm_handles: Arc<RwLock<HashMap<String, JoinHandle<()>>>>,
    locally_read_room_state: Arc<RwLock<HashMap<String, String>>>,
}

impl ShellTimelineService {
    pub(super) fn new() -> Self {
        Self {
            registry: ShellTimelineRegistry::new(),
            recent_warm_state: Arc::new(RwLock::new(HashMap::new())),
            recent_warm_handles: Arc::new(RwLock::new(HashMap::new())),
            locally_read_room_state: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub(super) fn registry(&self) -> &ShellTimelineRegistry {
        &self.registry
    }

    pub(super) fn locally_read_room_state(&self) -> &Arc<RwLock<HashMap<String, String>>> {
        &self.locally_read_room_state
    }

    pub(super) fn schedule_room_timeline_warmup(
        &self,
        client: matrix_sdk::Client,
        account_key: &str,
        store_dir: &Path,
        room_id: String,
        search_indexer: SearchIndexer,
    ) {
        let now = now_unix_ms();
        let state_key = format!("{account_key}::{room_id}");

        {
            let mut warm_state = self
                .recent_warm_state
                .write()
                .expect("shell timeline service warm-state lock poisoned");
            if recent_timeline_warmup_is_fresh(warm_state.get(&state_key), now) {
                return;
            }

            warm_state.insert(state_key.clone(), now);
        }

        let account_key = account_key.to_owned();
        let store_dir = store_dir.to_owned();
        let handle = tauri::async_runtime::spawn(async move {
            if let Err(error) =
                warm_room_recent_timeline(&client, &room_id, RECENT_TIMELINE_WARM_LIMIT).await
            {
                crate::utils::tracing::report_background_error(
                    "shell.timeline",
                    "warm_recent_timeline",
                    "shell.timeline_warm_failed",
                    "timeline",
                    &error,
                );
                return;
            }

            let Ok(room) = resolve_room(&client, &room_id) else {
                return;
            };
            let Ok(title) = room_title(&room).await else {
                return;
            };
            let Ok(items) = cached_timeline_items(&room).await else {
                return;
            };
            if let Err(error) = search_indexer
                .upsert_timeline_items(&account_key, &store_dir, &room_id, &title, &items)
                .await
            {
                crate::utils::tracing::report_recoverable_error(
                    "shell.search",
                    "index_warmed_timeline",
                    "shell.search_index_failed",
                    "search",
                    &error,
                );
            }
        });

        let mut warm_handles = self
            .recent_warm_handles
            .write()
            .expect("shell timeline service warm-handles lock poisoned");
        warm_handles.insert(state_key, handle);
    }

    pub(super) async fn stop_account_warmups(&self, account_key: &str) {
        let account_prefix = format!("{account_key}::");
        let removed_handles = {
            let mut warm_handles = self
                .recent_warm_handles
                .write()
                .expect("shell timeline service warm-handles lock poisoned");
            warm_handles
                .extract_if(|state_key, _handle| state_key.starts_with(&account_prefix))
                .map(|(_state_key, handle)| handle)
                .collect::<Vec<_>>()
        };

        for handle in removed_handles {
            handle.abort();
            drop(handle.await);
        }

        let mut warm_state = self
            .recent_warm_state
            .write()
            .expect("shell timeline service warm-state lock poisoned");
        warm_state.retain(|state_key, _| !state_key.starts_with(&account_prefix));
    }

    pub(super) async fn stop_all_warmups(&self) {
        let removed_handles = {
            let mut warm_handles = self
                .recent_warm_handles
                .write()
                .expect("shell timeline service warm-handles lock poisoned");
            warm_handles
                .drain()
                .map(|warm_handle_entry| warm_handle_entry.1)
                .collect::<Vec<_>>()
        };

        for handle in removed_handles {
            handle.abort();
            drop(handle.await);
        }

        let mut warm_state = self
            .recent_warm_state
            .write()
            .expect("shell timeline service warm-state lock poisoned");
        warm_state.clear();
    }
}

#[derive(Clone, Default)]
pub(super) struct ShellSearchService {
    indexer: SearchIndexer,
    backfill_coordinator: SearchBackfillCoordinator,
}

#[derive(Clone, Default)]
pub(super) struct ShellDiscoveryService;

impl ShellDiscoveryService {
    pub(super) fn new() -> Self {
        Self
    }
}

impl ShellSearchService {
    pub(super) fn new() -> Self {
        Self {
            indexer: SearchIndexer::new(),
            backfill_coordinator: SearchBackfillCoordinator::new(),
        }
    }

    pub(super) fn indexer_clone(&self) -> SearchIndexer {
        self.indexer.clone()
    }

    pub(super) fn backfill_coordinator(&self) -> &SearchBackfillCoordinator {
        &self.backfill_coordinator
    }

    pub(super) async fn index_room_summary(
        &self,
        account_key: &str,
        store_dir: &Path,
        summary: &RoomThreadSummary,
    ) {
        if let Err(error) = self
            .indexer
            .upsert_room_summary(account_key, store_dir, summary)
            .await
        {
            crate::utils::tracing::report_recoverable_error(
                "shell.search",
                "index_room_summary",
                "shell.search_index_failed",
                "search",
                &error,
            );
        }
    }

    pub(super) async fn index_space_summary(
        &self,
        account_key: &str,
        store_dir: &Path,
        summary: &crate::shell::types::SpaceSummary,
    ) {
        if let Err(error) = self
            .indexer
            .upsert_space_summary(account_key, store_dir, summary)
            .await
        {
            crate::utils::tracing::report_recoverable_error(
                "shell.search",
                "index_space_summary",
                "shell.search_index_failed",
                "search",
                &error,
            );
        }
    }

    pub(super) async fn tombstone_stale_room_documents(
        &self,
        account_key: &str,
        store_dir: &Path,
        active_room_ids: &HashSet<String>,
    ) {
        if let Err(error) = self
            .indexer
            .tombstone_stale_rooms(account_key, store_dir, active_room_ids)
            .await
        {
            crate::utils::tracing::report_recoverable_error(
                "shell.search",
                "tombstone_stale_rooms",
                "shell.search_index_failed",
                "search",
                &error,
            );
        }
    }

    pub(super) async fn tombstone_stale_space_documents(
        &self,
        account_key: &str,
        store_dir: &Path,
        active_space_ids: &HashSet<String>,
    ) {
        if let Err(error) = self
            .indexer
            .tombstone_stale_spaces(account_key, store_dir, active_space_ids)
            .await
        {
            crate::utils::tracing::report_recoverable_error(
                "shell.search",
                "tombstone_stale_spaces",
                "shell.search_index_failed",
                "search",
                &error,
            );
        }
    }

    pub(super) async fn schedule_search_backfill(
        &self,
        client: matrix_sdk::Client,
        sync_coordinator: super::sync_coordinator::ShellSyncCoordinator,
        account_key: &str,
        store_dir: &Path,
        room_candidates: Vec<(String, u64)>,
    ) {
        self.backfill_coordinator
            .schedule_recent_rooms(
                client,
                sync_coordinator,
                account_key,
                store_dir,
                self.indexer.clone(),
                room_candidates,
            )
            .await;
    }

    pub(super) async fn index_timeline_items(
        &self,
        account_key: &str,
        store_dir: &Path,
        room_id: &str,
        room_title: &str,
        items: &[crate::shell::types::RoomTimelineItem],
    ) {
        if let Err(error) = self
            .indexer
            .upsert_timeline_items(account_key, store_dir, room_id, room_title, items)
            .await
        {
            crate::utils::tracing::report_recoverable_error(
                "shell.search",
                "index_room_timeline",
                "shell.search_index_failed",
                "search",
                &error,
            );
            drop(
                self.indexer
                    .record_room_error(account_key, store_dir, room_id, "message_room", &error)
                    .await,
            );
        }
    }

    pub(super) async fn delete_message_documents(
        &self,
        account_key: &str,
        store_dir: &Path,
        room_id: &str,
        event_ids: &[String],
    ) {
        if let Err(error) = self
            .indexer
            .delete_message_documents(account_key, store_dir, room_id, event_ids)
            .await
        {
            crate::utils::tracing::report_recoverable_error(
                "shell.search",
                "remove_redacted_messages",
                "shell.search_index_failed",
                "search",
                &error,
            );
        }
    }
}

fn recent_timeline_warmup_is_fresh(previous_warm_at: Option<&u64>, now: u64) -> bool {
    let Some(previous_warm_at) = previous_warm_at else {
        return false;
    };

    now.saturating_sub(*previous_warm_at) < RECENT_TIMELINE_REWARM_INTERVAL_MS
}
