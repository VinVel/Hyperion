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

use std::{cmp::Reverse, collections::HashMap, path::Path, sync::Arc, time::Duration};

use matrix_sdk::Client;
use tauri::async_runtime::{JoinHandle, Mutex as AsyncMutex};

use super::{
    indexer::SearchIndexer,
    types::{SearchBackfillState, SearchRoomBackfillProgress},
};
use crate::{
    shell::service::{
        room::{resolve_room, room_title},
        sync_coordinator::ShellSyncCoordinator,
    },
    utils::time::now_unix_ms,
};

// Backfill pages should stay small until search indexing has real-world
// telemetry across desktop and mobile devices.
pub(super) const BACKFILL_PAGE_SIZE: u16 = 20;
// One room at a time keeps Matrix pagination and local FTS writes predictable.
pub(super) const BACKFILL_CONCURRENT_ROOM_LIMIT: usize = 1;
// Per-room batches are bounded so active sync and focused timelines retain
// priority over historical indexing.
pub(super) const BACKFILL_MAX_ROOM_BATCHES_PER_TICK: usize = 2;
// A short cooldown avoids turning idle indexing into a tight pagination loop.
pub(super) const BACKFILL_REQUEST_COOLDOWN: Duration = Duration::from_secs(3);
// Rate-limit responses pause the room before the scheduler attempts another
// backfill pass.
pub(super) const BACKFILL_RATE_LIMIT_RETRY_DELAY: Duration = Duration::from_mins(1);
// The first scheduler pass keeps a conservative session budget. It can be tuned
// after measuring actual account sizes and mobile power behavior.
pub(super) const BACKFILL_SESSION_EVENT_BUDGET: u64 = 1_000;

#[derive(Clone, Default)]
pub(in crate::shell::service) struct SearchBackfillCoordinator {
    handles: Arc<AsyncMutex<HashMap<String, JoinHandle<()>>>>,
}

impl SearchBackfillCoordinator {
    pub(in crate::shell::service) fn new() -> Self {
        Self::default()
    }

    pub(in crate::shell::service) async fn schedule_recent_rooms(
        &self,
        client: Client,
        sync_coordinator: ShellSyncCoordinator,
        account_key: &str,
        store_dir: &Path,
        search_indexer: SearchIndexer,
        mut room_candidates: Vec<(String, u64)>,
    ) {
        if mobile_deep_backfill_is_disabled() {
            return;
        }

        let active_count = self.active_account_backfill_count(account_key).await;
        if active_count >= BACKFILL_CONCURRENT_ROOM_LIMIT {
            return;
        }

        room_candidates.sort_by_key(|room_candidate| Reverse(room_candidate.1));
        let available_slots = BACKFILL_CONCURRENT_ROOM_LIMIT.saturating_sub(active_count);

        for (room_id, _activity_timestamp) in room_candidates.into_iter().take(available_slots) {
            self.schedule_room(
                client.clone(),
                sync_coordinator.clone(),
                account_key,
                store_dir,
                search_indexer.clone(),
                room_id,
            )
            .await;
        }
    }

    pub(in crate::shell::service) async fn stop_account(&self, account_key: &str) {
        let account_prefix = format!("{account_key}::");
        let removed_handles = {
            let mut handles = self.handles.lock().await;
            handles
                .extract_if(|key, _handle| key.starts_with(&account_prefix))
                .map(|(_key, handle)| handle)
                .collect::<Vec<_>>()
        };

        for handle in removed_handles {
            handle.abort();
            drop(handle.await);
        }
    }

    pub(in crate::shell::service) async fn stop_all(&self) {
        let removed_handles = {
            let mut handles = self.handles.lock().await;
            handles.drain().map(|entry| entry.1).collect::<Vec<_>>()
        };

        for handle in removed_handles {
            handle.abort();
            drop(handle.await);
        }
    }

    async fn schedule_room(
        &self,
        client: Client,
        sync_coordinator: ShellSyncCoordinator,
        account_key: &str,
        store_dir: &Path,
        search_indexer: SearchIndexer,
        room_id: String,
    ) {
        let task_key = task_key(account_key, &room_id);
        {
            let handles = self.handles.lock().await;
            if handles.contains_key(&task_key) {
                return;
            }
        }

        let account_key = account_key.to_owned();
        let store_dir = store_dir.to_owned();
        let coordinator = self.clone();
        let task_key_for_cleanup = task_key.clone();
        let handle = tauri::async_runtime::spawn(async move {
            run_room_backfill(
                client,
                sync_coordinator,
                &account_key,
                &store_dir,
                &search_indexer,
                &room_id,
            )
            .await;

            let mut handles = coordinator.handles.lock().await;
            handles.remove(&task_key_for_cleanup);
        });

        let mut handles = self.handles.lock().await;
        handles.insert(task_key, handle);
    }

    async fn active_account_backfill_count(&self, account_key: &str) -> usize {
        let account_prefix = format!("{account_key}::");
        let handles = self.handles.lock().await;
        handles
            .keys()
            .filter(|key| key.starts_with(&account_prefix))
            .count()
    }
}

async fn run_room_backfill(
    client: Client,
    sync_coordinator: ShellSyncCoordinator,
    account_key: &str,
    store_dir: &Path,
    search_indexer: &SearchIndexer,
    room_id: &str,
) {
    if let Err(error) = run_room_backfill_inner(
        client,
        &sync_coordinator,
        account_key,
        store_dir,
        search_indexer,
        room_id,
    )
    .await
    {
        let backfill_state = state_for_backfill_error(&error);
        crate::utils::tracing::report_background_error(
            "shell.search",
            "backfill_room",
            "shell.search_index_failed",
            "search",
            &error,
        );
        drop(
            search_indexer
                .mark_backfill_state(
                    account_key,
                    store_dir,
                    room_id,
                    backfill_state,
                    Some(error.clone()),
                )
                .await,
        );

        if retry_delay_for_backfill_state(backfill_state).is_some() {
            // The persisted state is the important part; the delay keeps this
            // task from immediately freeing its scheduler slot after a rate limit.
            matrix_sdk::sleep::sleep(BACKFILL_RATE_LIMIT_RETRY_DELAY).await;
        }
    }
}

async fn run_room_backfill_inner(
    client: Client,
    sync_coordinator: &ShellSyncCoordinator,
    account_key: &str,
    store_dir: &Path,
    search_indexer: &SearchIndexer,
    room_id: &str,
) -> Result<(), String> {
    let room = resolve_room(&client, room_id)?;
    let room_title = room_title(&room).await?;
    let mut progress = search_indexer
        .room_backfill_progress(account_key, store_dir, room_id)
        .await?
        .unwrap_or_else(|| new_backfill_progress(account_key, room_id));

    if progress.backfill_state == SearchBackfillState::Complete {
        return Ok(());
    }

    progress.backfill_state = SearchBackfillState::Indexing;
    progress.last_error = None;
    progress.last_indexed_at_unix_ms = Some(now_unix_ms());
    search_indexer
        .update_backfill_progress(store_dir, &progress)
        .await?;

    let mut indexed_this_tick = 0_u64;
    for _batch_index in 0..BACKFILL_MAX_ROOM_BATCHES_PER_TICK {
        if indexed_this_tick >= BACKFILL_SESSION_EVENT_BUDGET {
            progress.backfill_state = SearchBackfillState::Queued;
            break;
        }

        let updates = sync_coordinator
            .fetch_search_backfill_updates(
                account_key,
                &room,
                BACKFILL_PAGE_SIZE,
                progress.backfill_token.as_deref(),
            )
            .await?;
        if updates.items.is_empty()
            && updates.redacted_event_ids.is_empty()
            && updates.next_token.is_none()
        {
            progress.backfill_state = SearchBackfillState::Complete;
            progress.backfill_token = None;
            break;
        }

        search_indexer
            .upsert_timeline_items(account_key, store_dir, room_id, &room_title, &updates.items)
            .await?;
        search_indexer
            .delete_message_documents(account_key, store_dir, room_id, &updates.redacted_event_ids)
            .await?;

        let indexed_count = u64::try_from(updates.items.len()).unwrap_or(u64::MAX);
        indexed_this_tick = indexed_this_tick.saturating_add(indexed_count);
        progress.indexed_event_count = progress.indexed_event_count.saturating_add(indexed_count);
        progress.backfill_token = updates.next_token;
        progress.last_indexed_at_unix_ms = Some(now_unix_ms());

        if progress.backfill_token.is_none() {
            progress.backfill_state = SearchBackfillState::Complete;
            break;
        }

        matrix_sdk::sleep::sleep(BACKFILL_REQUEST_COOLDOWN).await;
    }

    if progress.backfill_state == SearchBackfillState::Indexing {
        progress.backfill_state = SearchBackfillState::Queued;
    }

    search_indexer
        .update_backfill_progress(store_dir, &progress)
        .await
}

fn new_backfill_progress(account_key: &str, room_id: &str) -> SearchRoomBackfillProgress {
    SearchRoomBackfillProgress {
        account_key: account_key.to_owned(),
        room_id: room_id.to_owned(),
        room_kind: String::from("conversation"),
        backfill_token: None,
        backfill_state: SearchBackfillState::Queued,
        indexed_event_count: 0,
        last_indexed_at_unix_ms: None,
        last_error: None,
    }
}

fn task_key(account_key: &str, room_id: &str) -> String {
    format!("{account_key}::{room_id}")
}

fn mobile_deep_backfill_is_disabled() -> bool {
    cfg!(any(target_os = "android", target_os = "ios"))
}

fn state_for_backfill_error(error: &str) -> SearchBackfillState {
    let normalized_error = error.to_ascii_lowercase();
    let is_rate_limited = normalized_error.contains("m_limit_exceeded")
        || normalized_error.contains("too many requests")
        || normalized_error.contains("rate limit")
        || normalized_error.contains("rate_limit")
        || normalized_error.contains("429");

    if is_rate_limited {
        return SearchBackfillState::RateLimited;
    }

    SearchBackfillState::Error
}

fn retry_delay_for_backfill_state(state: SearchBackfillState) -> Option<Duration> {
    if state == SearchBackfillState::RateLimited {
        return Some(BACKFILL_RATE_LIMIT_RETRY_DELAY);
    }

    None
}
