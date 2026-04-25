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
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use futures_util::{StreamExt, pin_mut};
use matrix_sdk::{Client, ruma::RoomId, sync::RoomUpdates};
use matrix_sdk_ui::room_list_service::{RoomListItem, RoomListService, filters};
use matrix_sdk_ui::sync_service::{State as SyncServiceState, SyncService};
use serde::Serialize;
use tauri::{Emitter, async_runtime::JoinHandle};

use crate::account::{AccountClientSnapshot, AccountManager};

pub const SHELL_SYNC_UPDATED_EVENT: &str = "hyperion://shell-sync-updated";
pub const SHELL_SYNC_STATUS_EVENT: &str = "hyperion://shell-sync-status";

// Focus subscriptions are additive, so keep a short debounce window to avoid
// repeatedly issuing the same room subscription while a user is already there.
const FOCUSED_ROOM_TTL_SECONDS: u64 = 90;
// The active account keeps a broad room-list observer alive so SyncService has
// a visible range immediately after login instead of waiting for a UI command.
const ACTIVE_ROOM_LIST_OBSERVER_PAGE_SIZE: usize = 10_000;

#[derive(Clone, Serialize)]
struct ShellSyncUpdatedPayload {
    account_key: String,
    changed_room_ids: Vec<String>,
    room_list_may_have_changed: bool,
    updated_at_unix_ms: u64,
}

#[derive(Clone, Serialize)]
struct ShellSyncStatusPayload {
    account_key: String,
    state: String,
    detail: Option<String>,
    updated_at_unix_ms: u64,
}

#[derive(Clone)]
struct FocusedRoomState {
    room_id: String,
    last_touched_unix_ms: u64,
}

struct RunningAccountSync {
    sync_service: Arc<SyncService>,
    state_listener_handle: JoinHandle<()>,
    room_update_listener_handle: JoinHandle<()>,
    room_list_observer_handle: JoinHandle<()>,
}

#[derive(Clone, Default)]
pub struct ShellSyncManager {
    running_accounts: Arc<RwLock<HashMap<String, RunningAccountSync>>>,
    focused_rooms: Arc<RwLock<HashMap<String, FocusedRoomState>>>,
}

impl ShellSyncManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn ensure_started_for_manager(
        &self,
        manager: &AccountManager,
        app: &tauri::AppHandle,
    ) -> Result<(), String> {
        let Some(active_account) = manager.active_account_client(app).await? else {
            self.stop_all_accounts().await;
            return Ok(());
        };

        self.stop_other_accounts(&active_account.account_key).await;
        self.ensure_started(app, active_account).await
    }

    pub fn touch_focused_room(&self, account_key: &str, room_id: &str) {
        let now = now_unix_ms();
        let should_subscribe = {
            let mut focused_rooms = self
                .focused_rooms
                .write()
                .expect("shell sync manager focused-rooms lock poisoned");

            let should_subscribe = focused_rooms.get(account_key).is_none_or(|focused_room| {
                focused_room.room_id != room_id
                    || now.saturating_sub(focused_room.last_touched_unix_ms)
                        >= Duration::from_secs(FOCUSED_ROOM_TTL_SECONDS).as_millis() as u64
            });

            focused_rooms.insert(
                account_key.to_owned(),
                FocusedRoomState {
                    room_id: room_id.to_owned(),
                    last_touched_unix_ms: now,
                },
            );

            should_subscribe
        };

        if !should_subscribe {
            return;
        }

        let sync_service = self
            .running_accounts
            .read()
            .expect("shell sync manager running-accounts lock poisoned")
            .get(account_key)
            .map(|running_account| running_account.sync_service.clone());

        if let Some(sync_service) = sync_service {
            let owned_room_id = match RoomId::parse(room_id) {
                Ok(room_id) => room_id.to_owned(),
                Err(error) => {
                    eprintln!("Failed to parse focused room id {room_id}: {error}");
                    return;
                }
            };

            tauri::async_runtime::spawn(async move {
                sync_service
                    .room_list_service()
                    .subscribe_to_rooms(&[owned_room_id.as_ref()])
                    .await;
            });
        }
    }

    pub fn room_list_service(&self, account_key: &str) -> Option<Arc<RoomListService>> {
        self.running_accounts
            .read()
            .expect("shell sync manager running-accounts lock poisoned")
            .get(account_key)
            .map(|running_account| running_account.sync_service.room_list_service())
    }

    async fn ensure_started(
        &self,
        app: &tauri::AppHandle,
        account: AccountClientSnapshot,
    ) -> Result<(), String> {
        let existing_sync_service = self
            .running_accounts
            .read()
            .expect("shell sync manager running-accounts lock poisoned")
            .get(&account.account_key)
            .map(|running_sync| running_sync.sync_service.clone());

        if let Some(sync_service) = existing_sync_service {
            sync_service.start().await;
            return Ok(());
        }

        emit_shell_sync_status(app, &account.account_key, "starting", None);

        let sync_service = Arc::new(
            SyncService::builder(account.client.clone())
                .with_offline_mode()
                .build()
                .await
                .map_err(|error| {
                    let detail = error.to_string();
                    let state = if is_unsupported_sync_error(&detail) {
                        "unsupported"
                    } else {
                        "error"
                    };
                    emit_shell_sync_status(app, &account.account_key, state, Some(detail.clone()));
                    format!("Failed to build shell sync service: {detail}")
                })?,
        );

        let state_listener_handle = self.spawn_state_listener_task(
            app.clone(),
            account.account_key.clone(),
            sync_service.clone(),
        );
        let room_update_listener_handle = self.spawn_room_update_listener_task(
            app.clone(),
            account.account_key.clone(),
            account.client.clone(),
        );
        let room_list_observer_handle = self.spawn_room_list_observer_task(
            app.clone(),
            account.account_key.clone(),
            sync_service.clone(),
        );

        sync_service.start().await;

        if let Some(focused_room_id) = self.focused_room_id(&account.account_key) {
            self.subscribe_to_focused_room(sync_service.clone(), &focused_room_id);
        }

        self.running_accounts
            .write()
            .expect("shell sync manager running-accounts lock poisoned")
            .insert(
                account.account_key,
                RunningAccountSync {
                    sync_service,
                    state_listener_handle,
                    room_update_listener_handle,
                    room_list_observer_handle,
                },
            );

        Ok(())
    }

    fn spawn_state_listener_task(
        &self,
        app: tauri::AppHandle,
        account_key: String,
        sync_service: Arc<SyncService>,
    ) -> JoinHandle<()> {
        tauri::async_runtime::spawn(async move {
            let mut state = sync_service.state();

            while let Some(next_state) = state.next().await {
                let (status, detail) = shell_sync_status_parts(&next_state);
                emit_shell_sync_status(&app, &account_key, status, detail);
            }
        })
    }

    fn spawn_room_update_listener_task(
        &self,
        app: tauri::AppHandle,
        account_key: String,
        client: Client,
    ) -> JoinHandle<()> {
        tauri::async_runtime::spawn(async move {
            let mut room_updates = client.subscribe_to_all_room_updates();

            loop {
                match room_updates.recv().await {
                    Ok(updates) => emit_shell_sync_updated(&app, &account_key, &updates),
                    Err(error) => {
                        eprintln!(
                            "Shell room update listener for account {account_key} stopped with error: {error}"
                        );
                        break;
                    }
                }
            }
        })
    }

    fn spawn_room_list_observer_task(
        &self,
        app: tauri::AppHandle,
        account_key: String,
        sync_service: Arc<SyncService>,
    ) -> JoinHandle<()> {
        tauri::async_runtime::spawn(async move {
            let room_list = match sync_service.room_list_service().all_rooms().await {
                Ok(room_list) => room_list,
                Err(error) => {
                    eprintln!(
                        "Shell room list observer for account {account_key} could not start: {error}"
                    );
                    return;
                }
            };

            let (entries, entries_controller) =
                room_list.entries_with_dynamic_adapters(ACTIVE_ROOM_LIST_OBSERVER_PAGE_SIZE);
            let filter = Box::new(filters::new_filter_joined());
            let _ = entries_controller.set_filter(filter);

            pin_mut!(entries);
            while let Some(diffs) = entries.next().await {
                if !diffs.is_empty() {
                    let changed_room_ids = room_ids_from_room_list_diffs(&diffs);
                    emit_shell_room_list_updated(&app, &account_key, changed_room_ids);
                }
            }
        })
    }

    fn focused_room_id(&self, account_key: &str) -> Option<String> {
        self.focused_rooms
            .read()
            .expect("shell sync manager focused-rooms lock poisoned")
            .get(account_key)
            .map(|focused_room| focused_room.room_id.clone())
    }

    fn subscribe_to_focused_room(&self, sync_service: Arc<SyncService>, room_id: &str) {
        let owned_room_id = match RoomId::parse(room_id) {
            Ok(room_id) => room_id.to_owned(),
            Err(error) => {
                eprintln!("Failed to parse focused room id {room_id}: {error}");
                return;
            }
        };

        tauri::async_runtime::spawn(async move {
            sync_service
                .room_list_service()
                .subscribe_to_rooms(&[owned_room_id.as_ref()])
                .await;
        });
    }

    async fn stop_other_accounts(&self, active_account_key: &str) {
        let inactive_account_keys = self
            .running_accounts
            .read()
            .expect("shell sync manager running-accounts lock poisoned")
            .keys()
            .filter(|account_key| account_key.as_str() != active_account_key)
            .cloned()
            .collect::<Vec<_>>();

        for account_key in inactive_account_keys {
            self.stop_account(&account_key).await;
        }
    }

    pub async fn stop_all_accounts(&self) {
        let account_keys = self
            .running_accounts
            .read()
            .expect("shell sync manager running-accounts lock poisoned")
            .keys()
            .cloned()
            .collect::<Vec<_>>();

        for account_key in account_keys {
            self.stop_account(&account_key).await;
        }
    }

    pub async fn stop_account(&self, account_key: &str) {
        let Some(running_account) = self
            .running_accounts
            .write()
            .expect("shell sync manager running-accounts lock poisoned")
            .remove(account_key)
        else {
            return;
        };

        running_account.sync_service.stop().await;
        running_account.state_listener_handle.abort();
        running_account.room_update_listener_handle.abort();
        running_account.room_list_observer_handle.abort();
        let _ = running_account.state_listener_handle.await;
        let _ = running_account.room_update_listener_handle.await;
        let _ = running_account.room_list_observer_handle.await;

        self.focused_rooms
            .write()
            .expect("shell sync manager focused-rooms lock poisoned")
            .remove(account_key);
    }
}

fn shell_sync_status_parts(state: &SyncServiceState) -> (&'static str, Option<String>) {
    match state {
        SyncServiceState::Idle => ("idle", None),
        SyncServiceState::Running => ("running", None),
        SyncServiceState::Offline => ("offline", None),
        SyncServiceState::Terminated => ("terminated", None),
        SyncServiceState::Error(error) => {
            let detail = error.to_string();
            let status = if is_unsupported_sync_error(&detail) {
                "unsupported"
            } else {
                "error"
            };

            (status, Some(detail))
        }
    }
}

fn is_unsupported_sync_error(error: &str) -> bool {
    error.contains("M_UNRECOGNIZED")
        || (error.contains("404") && error.to_ascii_lowercase().contains("sliding"))
}

fn emit_shell_sync_status(
    app: &tauri::AppHandle,
    account_key: &str,
    state: &str,
    detail: Option<String>,
) {
    let payload = ShellSyncStatusPayload {
        account_key: account_key.to_owned(),
        state: state.to_owned(),
        detail,
        updated_at_unix_ms: now_unix_ms(),
    };

    if let Err(error) = app.emit(SHELL_SYNC_STATUS_EVENT, payload) {
        eprintln!("Failed to emit shell sync status event: {error}");
    }
}

pub(super) fn emit_shell_room_updated(
    app: &tauri::AppHandle,
    account_key: &str,
    room_id: &str,
    room_list_may_have_changed: bool,
) {
    let payload = ShellSyncUpdatedPayload {
        account_key: account_key.to_owned(),
        changed_room_ids: vec![room_id.to_owned()],
        room_list_may_have_changed,
        updated_at_unix_ms: now_unix_ms(),
    };

    if let Err(error) = app.emit(SHELL_SYNC_UPDATED_EVENT, payload) {
        eprintln!("Failed to emit shell room update event: {error}");
    }
}

fn emit_shell_room_list_updated(
    app: &tauri::AppHandle,
    account_key: &str,
    changed_room_ids: Vec<String>,
) {
    let payload = ShellSyncUpdatedPayload {
        account_key: account_key.to_owned(),
        changed_room_ids,
        room_list_may_have_changed: true,
        updated_at_unix_ms: now_unix_ms(),
    };

    if let Err(error) = app.emit(SHELL_SYNC_UPDATED_EVENT, payload) {
        eprintln!("Failed to emit shell room-list update event: {error}");
    }
}

fn room_ids_from_room_list_diffs(diffs: &[eyeball_im::VectorDiff<RoomListItem>]) -> Vec<String> {
    let mut changed_room_ids = Vec::new();

    for diff in diffs {
        match diff {
            eyeball_im::VectorDiff::Append { values }
            | eyeball_im::VectorDiff::Reset { values } => {
                changed_room_ids.extend(values.iter().map(room_list_item_id));
            }
            eyeball_im::VectorDiff::PushFront { value }
            | eyeball_im::VectorDiff::PushBack { value }
            | eyeball_im::VectorDiff::Insert { value, .. }
            | eyeball_im::VectorDiff::Set { value, .. } => {
                changed_room_ids.push(room_list_item_id(value));
            }
            // Removals and clears do not carry the removed room value. Keep
            // the payload ambiguous so the frontend refreshes the visible room.
            eyeball_im::VectorDiff::Clear
            | eyeball_im::VectorDiff::PopFront
            | eyeball_im::VectorDiff::PopBack
            | eyeball_im::VectorDiff::Remove { .. }
            | eyeball_im::VectorDiff::Truncate { .. } => {}
        }
    }

    changed_room_ids.sort();
    changed_room_ids.dedup();
    changed_room_ids
}

fn room_list_item_id(item: &RoomListItem) -> String {
    item.room_id().to_string()
}

fn emit_shell_sync_updated(app: &tauri::AppHandle, account_key: &str, updates: &RoomUpdates) {
    let payload = ShellSyncUpdatedPayload {
        account_key: account_key.to_owned(),
        changed_room_ids: updates
            .iter_all_room_ids()
            .map(|room_id| room_id.to_string())
            .collect(),
        room_list_may_have_changed: !updates.is_empty(),
        updated_at_unix_ms: now_unix_ms(),
    };

    if let Err(error) = app.emit(SHELL_SYNC_UPDATED_EVENT, payload) {
        eprintln!("Failed to emit shell sync update event: {error}");
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}
