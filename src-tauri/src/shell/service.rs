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

use matrix_sdk::Room;

use crate::account::AccountManager;

mod cache_state;
mod caching;
pub mod discovery;
mod global_search;
mod paging;
mod read_state;
mod room;
mod room_commands;
mod room_list;
mod runtime;
mod search;
mod timeline;
mod timeline_commands;

pub(super) use self::cache_state::ShellCacheState;

use self::runtime::{ShellDiscoveryService, ShellSearchService, ShellTimelineService};

use super::sync::ShellSyncManager;

// The default room-open page should feel immediate, but still show enough
// surrounding context that the user does not land in a "single screen" view.
const DEFAULT_TIMELINE_LIMIT: u16 = 30;
// Event-context jumps are meant to anchor the user around a hit, not replay a
// full timeline page, so keep the context window smaller than the normal page.
const DEFAULT_EVENT_CONTEXT_LIMIT: u16 = 8;
// Warm a meaningfully larger local window than the visible timeline so recently
// reopened rooms can render from disk/cache without fetching again immediately.
const RECENT_TIMELINE_WARM_LIMIT: u16 = 80;
// Only warm the most recently active rooms in background; broad warmups would
// compete with sync and make multi-room accounts more expensive than needed.
const RECENT_TIMELINE_WARM_ROOM_COUNT: usize = 6;
// Rewarm infrequently enough to avoid churn, but often enough that active rooms
// keep a recent local window available across normal shell navigation.
const RECENT_TIMELINE_REWARM_INTERVAL_MS: u64 = 10 * 60 * 1_000;
// Command snapshots need a bounded page size when materializing the room-list
// stream. Keep it large enough to cover realistic active accounts in one pass.
const ROOM_LIST_SNAPSHOT_PAGE_SIZE: usize = 10_000;
// Restoring very deep rooms should stay bounded on app startup; explicit older
// pagination can still continue beyond this as the user asks for more history.
const MAX_RESTORED_TIMELINE_ITEMS: usize = 1_000;

pub(super) enum ShellRoomListKind {
    Conversations,
    Spaces,
}

#[derive(Clone, Default)]
pub struct ShellManager {
    sync_manager: ShellSyncManager,
    cache_state: ShellCacheState,
    timeline_service: ShellTimelineService,
    search_service: ShellSearchService,
    discovery_service: ShellDiscoveryService,
}

impl ShellManager {
    pub fn new() -> Self {
        Self {
            sync_manager: ShellSyncManager::new(),
            cache_state: ShellCacheState::new(),
            timeline_service: ShellTimelineService::new(),
            search_service: ShellSearchService::new(),
            discovery_service: ShellDiscoveryService::new(),
        }
    }

    pub async fn ensure_active_account_sync(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
    ) -> Result<(), String> {
        account_manager.ensure_loaded(app).await?;
        let Some(account) = account_manager.active_account_client_loaded() else {
            self.sync_manager.stop_all_accounts().await;
            return Ok(());
        };

        self.sync_manager
            .ensure_started_for_account(app, account_manager, account)
            .await
    }

    pub async fn stop_account(&self, account_key: &str) {
        self.timeline_service
            .stop_account_warmups(account_key)
            .await;
        self.search_service
            .backfill_coordinator()
            .stop_account(account_key)
            .await;
        self.sync_manager.stop_account(account_key).await;
        self.timeline_service
            .registry()
            .clear_account(account_key)
            .await;
        self.cache_state.clear_served_room_thread_cache(account_key);
        self.cache_state.clear_served_space_cache(account_key);
        self.cache_state
            .clear_served_room_timeline_caches(account_key);
    }

    pub async fn stop_all_accounts(&self) {
        self.timeline_service.stop_all_warmups().await;
        self.search_service.backfill_coordinator().stop_all().await;
        self.sync_manager.stop_all_accounts().await;
        self.timeline_service.registry().clear_all().await;
        self.cache_state.clear_all_served_room_thread_caches();
        self.cache_state.clear_all_served_space_caches();
        self.cache_state.clear_all_served_room_timeline_caches();
    }

    fn mark_room_focused(&self, account_key: &str, room_id: &str) {
        self.sync_manager.touch_focused_room(account_key, room_id);
    }

    fn ensure_sync_in_background(&self, app: &tauri::AppHandle, account_manager: &AccountManager) {
        let shell_manager = self.clone();
        let app = app.clone();
        let account_manager = account_manager.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = shell_manager
                .ensure_active_account_sync(&app, &account_manager)
                .await
            {
                eprintln!("Failed to refresh cached shell room list in background: {error}");
            }
        });
    }

    async fn delete_redacted_timeline_items(
        &self,
        account_key: &str,
        store_dir: &std::path::Path,
        room: &Room,
    ) {
        let redacted_event_ids = match self
            .timeline_service
            .registry()
            .live_redacted_event_ids(account_key, room)
            .await
        {
            Ok(redacted_event_ids) => redacted_event_ids,
            Err(error) => {
                eprintln!("Failed to inspect redacted timeline items for search: {error}");
                return;
            }
        };
        self.search_service
            .delete_message_documents(
                account_key,
                store_dir,
                room.room_id().as_str(),
                &redacted_event_ids,
            )
            .await;
    }

    async fn delete_focused_redacted_timeline_items(
        &self,
        account_key: &str,
        store_dir: &std::path::Path,
        room: &Room,
        event_id: matrix_sdk::ruma::OwnedEventId,
        context_limit: u16,
    ) {
        let redacted_event_ids = match self
            .timeline_service
            .registry()
            .focused_redacted_event_ids(account_key, room, event_id, context_limit)
            .await
        {
            Ok(redacted_event_ids) => redacted_event_ids,
            Err(error) => {
                eprintln!("Failed to inspect focused redacted timeline items for search: {error}");
                return;
            }
        };
        self.search_service
            .delete_message_documents(
                account_key,
                store_dir,
                room.room_id().as_str(),
                &redacted_event_ids,
            )
            .await;
    }
}
