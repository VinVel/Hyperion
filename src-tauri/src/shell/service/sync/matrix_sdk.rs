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
};

use futures_util::{StreamExt, pin_mut};
use matrix_sdk::{
    Client, Error as MatrixError, HttpError, RefreshTokenError, SessionChange, ruma::RoomId,
    sync::RoomUpdates,
};
use matrix_sdk_ui::room_list_service::{RoomListItem, RoomListService, filters};
use matrix_sdk_ui::{
    encryption_sync_service, room_list_service,
    sync_service::{Error as SyncServiceError, State as SyncServiceState, SyncService},
};
use serde::Serialize;
use tauri::{Emitter, async_runtime::JoinHandle};

use crate::{
    account::{AccountClientSnapshot, AccountManager},
    settings::sessions::register_session_verification_event_handler,
    shell::types::RoomTimelineItem,
    utils::time::now_unix_ms,
};

// These shell event constants and emitters moved with the SDK adapter to keep
// this refactor behavior-preserving. A later events-module cleanup should move
// them out of the low-level Matrix SDK sync adapter.
pub const SHELL_SYNC_UPDATED_EVENT: &str = "hyperion://shell-sync-updated";
pub const SHELL_TIMELINE_UPDATED_EVENT: &str = "hyperion://shell-timeline-updated";
pub const SHELL_TYPING_UPDATED_EVENT: &str = "hyperion://shell-typing-updated";
pub const SHELL_SYNC_STATUS_EVENT: &str = "hyperion://shell-sync-status";
pub const SHELL_SESSION_DEAUTHORIZED_EVENT: &str = "hyperion://session-deauthorized";
pub const SHELL_SESSION_REAUTHENTICATION_REQUIRED_EVENT: &str =
    "hyperion://session-reauthentication-required";

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

#[derive(Clone, Serialize)]
struct ShellTimelineUpdatedPayload {
    account_key: String,
    room_id: String,
    items: Vec<RoomTimelineItem>,
    redacted_event_ids: Vec<String>,
    updated_at_unix_ms: u64,
}

#[derive(Clone, Serialize)]
struct ShellTypingUpdatedPayload {
    account_key: String,
    room_id: String,
    users: Vec<String>,
    updated_at_unix_ms: u64,
}

#[derive(Clone, Serialize)]
struct SessionReauthenticationRequiredPayload {
    account_key: String,
    state: &'static str,
    reason: &'static str,
}

#[derive(Clone)]
struct FocusedRoomState {
    room_id: String,
}

struct RunningAccountSync {
    sync_service: Arc<SyncService>,
    state_listener_handle: JoinHandle<()>,
    room_update_listener_handle: JoinHandle<()>,
    room_list_observer_handle: JoinHandle<()>,
    session_change_listener_handle: JoinHandle<()>,
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

    pub async fn ensure_started_for_account(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        account: AccountClientSnapshot,
    ) -> Result<(), String> {
        self.stop_other_accounts(&account.account_key).await;
        self.ensure_started(app, account_manager.clone(), account)
            .await
    }

    pub fn set_focused_room(&self, account_key: &str, room_id: &str) {
        {
            let mut focused_rooms = self
                .focused_rooms
                .write()
                .expect("shell sync manager focused-rooms lock poisoned");

            focused_rooms.insert(
                account_key.to_owned(),
                FocusedRoomState {
                    room_id: room_id.to_owned(),
                },
            );
        }

        self.subscribe_to_focused_room_for_account(account_key, room_id);
    }

    pub fn clear_focused_room(&self, account_key: &str, room_id: &str) {
        let mut focused_rooms = self
            .focused_rooms
            .write()
            .expect("shell sync manager focused-rooms lock poisoned");

        if focused_rooms
            .get(account_key)
            .is_some_and(|focused_room| focused_room.room_id == room_id)
        {
            focused_rooms.remove(account_key);
        }
    }

    pub fn room_list_service(&self, account_key: &str) -> Option<Arc<RoomListService>> {
        let running_accounts = self
            .running_accounts
            .read()
            .expect("shell sync manager running-accounts lock poisoned");

        running_accounts
            .get(account_key)
            .map(|running_account| running_account.sync_service.room_list_service())
    }

    async fn ensure_started(
        &self,
        app: &tauri::AppHandle,
        account_manager: AccountManager,
        account: AccountClientSnapshot,
    ) -> Result<(), String> {
        let existing_sync_service = {
            let running_accounts = self
                .running_accounts
                .read()
                .expect("shell sync manager running-accounts lock poisoned");
            running_accounts
                .get(&account.account_key)
                .map(|running_sync| running_sync.sync_service.clone())
        };

        if let Some(sync_service) = existing_sync_service {
            sync_service.start().await;
            return Ok(());
        }

        emit_shell_sync_status(app, &account.account_key, "starting", None);

        let sync_service = build_shell_sync_service(app, &account).await?;
        let sync_service = Arc::new(sync_service);

        let state_listener_handle = Self::spawn_state_listener_task(
            app.clone(),
            account.account_key.clone(),
            sync_service.clone(),
            self.focused_rooms.clone(),
        );
        let room_update_listener_handle = Self::spawn_room_update_listener_task(
            app.clone(),
            account.account_key.clone(),
            account.client.clone(),
        );
        let room_list_observer_handle = Self::spawn_room_list_observer_task(
            app.clone(),
            account.account_key.clone(),
            sync_service.clone(),
        );
        let session_change_listener_handle = Self::spawn_session_change_listener_task(
            app.clone(),
            account_manager,
            account.clone(),
            self.clone(),
        );

        sync_service.start().await;

        if let Some(focused_room_id) = self.focused_room_id(&account.account_key) {
            Self::subscribe_to_focused_room(sync_service.clone(), &focused_room_id);
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
                    session_change_listener_handle,
                },
            );

        Ok(())
    }

    fn spawn_state_listener_task(
        app: tauri::AppHandle,
        account_key: String,
        sync_service: Arc<SyncService>,
        focused_rooms: Arc<RwLock<HashMap<String, FocusedRoomState>>>,
    ) -> JoinHandle<()> {
        tauri::async_runtime::spawn(async move {
            let mut state = sync_service.state();

            while let Some(next_state) = state.next().await {
                let (status, detail) = shell_sync_status_parts(&next_state);
                emit_shell_sync_status(&app, &account_key, status, detail);

                if matches!(next_state, SyncServiceState::Running)
                    && let Some(focused_room_id) =
                        focused_room_id_from_state(&focused_rooms, &account_key)
                {
                    Self::subscribe_to_focused_room(sync_service.clone(), &focused_room_id);
                }
            }
        })
    }

    fn spawn_room_update_listener_task(
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
                        crate::utils::tracing::report_background_error(
                            "shell.sync",
                            "listen_room_updates",
                            "shell.sync_failed",
                            "sync",
                            &error,
                        );
                        break;
                    }
                }
            }
        })
    }

    fn spawn_room_list_observer_task(
        app: tauri::AppHandle,
        account_key: String,
        sync_service: Arc<SyncService>,
    ) -> JoinHandle<()> {
        tauri::async_runtime::spawn(async move {
            let room_list = match sync_service.room_list_service().all_rooms().await {
                Ok(room_list) => room_list,
                Err(error) => {
                    crate::utils::tracing::report_background_error(
                        "shell.sync",
                        "observe_room_list",
                        "shell.sync_failed",
                        "sync",
                        &error,
                    );
                    return;
                }
            };

            let (entries, entries_controller) =
                room_list.entries_with_dynamic_adapters(ACTIVE_ROOM_LIST_OBSERVER_PAGE_SIZE);
            let filter = Box::new(filters::new_filter_joined());
            let _filter_was_applied = entries_controller.set_filter(filter);

            pin_mut!(entries);
            while let Some(diffs) = entries.next().await {
                if diffs.is_empty() {
                    continue;
                }

                let changed_room_ids = room_ids_from_room_list_diffs(&diffs);
                emit_shell_room_list_updated(&app, &account_key, changed_room_ids);
            }
        })
    }

    fn spawn_session_change_listener_task(
        app: tauri::AppHandle,
        account_manager: AccountManager,
        account: AccountClientSnapshot,
        sync_manager: ShellSyncManager,
    ) -> JoinHandle<()> {
        tauri::async_runtime::spawn(async move {
            let mut session_changes = account.client.subscribe_to_session_changes();

            loop {
                match session_changes.recv().await {
                    Ok(SessionChange::UnknownToken(unknown_token)) => {
                        sync_manager
                            .stop_account_from_session_listener(&account.account_key)
                            .await;
                        if unknown_token.soft_logout {
                            account_manager.mark_reauthentication_required(&account.account_key);
                            emit_session_reauthentication_required(&app, &account.account_key);
                            break;
                        }
                        if let Err(error) = account_manager
                            .deauthorize_account_store(&app, &account.store_dir)
                            .await
                        {
                            crate::utils::tracing::report_recoverable_error(
                                "shell.sync",
                                "remove_deauthorized_account",
                                "account.storage_cleanup_failed",
                                "account",
                                &error,
                            );
                        }
                        emit_session_deauthorized(&app, &account.account_key);
                        break;
                    }
                    Ok(SessionChange::TokensRefreshed) => {
                        if let Err(error) =
                            account_manager.persist_refreshed_session(&account).await
                        {
                            crate::utils::tracing::report_recoverable_error(
                                "shell.sync",
                                "persist_refreshed_session",
                                "account.session_refresh_persistence_failed",
                                "storage",
                                &error,
                            );
                        }
                    }
                    Err(error) => {
                        crate::utils::tracing::report_background_error(
                            "shell.sync",
                            "listen_session_changes",
                            "shell.sync_failed",
                            "sync",
                            &error,
                        );
                        break;
                    }
                }
            }
        })
    }

    fn focused_room_id(&self, account_key: &str) -> Option<String> {
        let focused_rooms = self
            .focused_rooms
            .read()
            .expect("shell sync manager focused-rooms lock poisoned");

        focused_rooms
            .get(account_key)
            .map(|focused_room| focused_room.room_id.clone())
    }

    fn subscribe_to_focused_room_for_account(&self, account_key: &str, room_id: &str) {
        let running_accounts = self
            .running_accounts
            .read()
            .expect("shell sync manager running-accounts lock poisoned");
        let sync_service = running_accounts
            .get(account_key)
            .map(|running_account| running_account.sync_service.clone());
        drop(running_accounts);

        if let Some(sync_service) = sync_service {
            Self::subscribe_to_focused_room(sync_service, room_id);
        }
    }

    fn subscribe_to_focused_room(sync_service: Arc<SyncService>, room_id: &str) {
        let owned_room_id = match RoomId::parse(room_id) {
            Ok(room_id) => room_id,
            Err(error) => {
                crate::utils::tracing::report_recoverable_error(
                    "shell.sync",
                    "parse_focused_room_id",
                    "shell.room_id_invalid",
                    "room",
                    &error,
                );
                return;
            }
        };

        tauri::async_runtime::spawn(async move {
            let room_list_service = sync_service.room_list_service();
            room_list_service
                .subscribe_to_rooms(&[owned_room_id.as_ref()])
                .await;
        });
    }

    async fn stop_other_accounts(&self, active_account_key: &str) {
        let inactive_account_keys = {
            let running_accounts = self
                .running_accounts
                .read()
                .expect("shell sync manager running-accounts lock poisoned");
            running_accounts
                .keys()
                .filter(|account_key| account_key.as_str() != active_account_key)
                .cloned()
                .collect::<Vec<_>>()
        };

        for account_key in inactive_account_keys {
            self.stop_account(&account_key).await;
        }
    }

    pub async fn stop_all_accounts(&self) {
        let account_keys = {
            let running_accounts = self
                .running_accounts
                .read()
                .expect("shell sync manager running-accounts lock poisoned");
            running_accounts.keys().cloned().collect::<Vec<_>>()
        };

        for account_key in account_keys {
            self.stop_account(&account_key).await;
        }
    }

    pub async fn stop_account(&self, account_key: &str) {
        let running_account = {
            let mut running_accounts = self
                .running_accounts
                .write()
                .expect("shell sync manager running-accounts lock poisoned");
            running_accounts.remove(account_key)
        };
        let Some(running_account) = running_account else {
            return;
        };

        running_account.sync_service.stop().await;
        running_account.state_listener_handle.abort();
        running_account.room_update_listener_handle.abort();
        running_account.room_list_observer_handle.abort();
        running_account.session_change_listener_handle.abort();
        drop(running_account.state_listener_handle.await);
        drop(running_account.room_update_listener_handle.await);
        drop(running_account.room_list_observer_handle.await);
        drop(running_account.session_change_listener_handle.await);

        let mut focused_rooms = self
            .focused_rooms
            .write()
            .expect("shell sync manager focused-rooms lock poisoned");
        focused_rooms.remove(account_key);
    }

    async fn stop_account_from_session_listener(&self, account_key: &str) {
        let running_account = self
            .running_accounts
            .write()
            .expect("shell sync manager running-accounts lock poisoned")
            .remove(account_key);
        let Some(running_account) = running_account else {
            return;
        };
        running_account.sync_service.stop().await;
        running_account.state_listener_handle.abort();
        running_account.room_update_listener_handle.abort();
        running_account.room_list_observer_handle.abort();
        // This task owns `session_change_listener_handle`; never abort or await itself.
        drop(running_account.state_listener_handle.await);
        drop(running_account.room_update_listener_handle.await);
        drop(running_account.room_list_observer_handle.await);
        self.focused_rooms
            .write()
            .expect("shell sync manager focused-rooms lock poisoned")
            .remove(account_key);
    }
}

async fn build_shell_sync_service(
    app: &tauri::AppHandle,
    account: &AccountClientSnapshot,
) -> Result<SyncService, String> {
    register_session_verification_event_handler(app, account);
    let sync_service_builder = SyncService::builder(account.client.clone()).with_offline_mode();

    match sync_service_builder.build().await {
        Ok(sync_service) => Ok(sync_service),
        Err(error) => {
            let detail = error.to_string();
            let state = shell_sync_error_status(&error);
            emit_shell_sync_status(app, &account.account_key, state, Some(detail.clone()));
            Err(format!("Failed to build shell sync service: {detail}"))
        }
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
            let status = shell_sync_error_status(error);

            (status, Some(detail))
        }
    }
}

fn shell_sync_error_status(error: &SyncServiceError) -> &'static str {
    shell_sync_error_kind(error).as_str()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShellSyncErrorKind {
    Offline,
    Unsupported,
    Error,
}

impl ShellSyncErrorKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Unsupported => "unsupported",
            Self::Error => "error",
        }
    }
}

fn shell_sync_error_kind(error: &SyncServiceError) -> ShellSyncErrorKind {
    let matrix_error = match error {
        SyncServiceError::RoomList(room_list_service::Error::SlidingSync(error))
        | SyncServiceError::EncryptionSync(
            encryption_sync_service::Error::SlidingSync(error)
            | encryption_sync_service::Error::ClientError(error)
            | encryption_sync_service::Error::LockError(error),
        ) => error,
        SyncServiceError::RoomList(_) | SyncServiceError::Supervisor => {
            return ShellSyncErrorKind::Error;
        }
    };

    if matrix_error_is_unsupported_sliding_sync_endpoint(matrix_error) {
        return ShellSyncErrorKind::Unsupported;
    }
    if matrix_error_is_offline(matrix_error) {
        ShellSyncErrorKind::Offline
    } else {
        ShellSyncErrorKind::Error
    }
}

fn matrix_error_is_unsupported_sliding_sync_endpoint(error: &MatrixError) -> bool {
    matches!(error, MatrixError::Http(http_error) if http_error.is_endpoint_not_implemented())
}

fn matrix_error_is_offline(error: &MatrixError) -> bool {
    matches!(error, MatrixError::Timeout)
        || matches!(error, MatrixError::Http(http_error) if http_error_is_offline(http_error))
}

fn http_error_is_offline(error: &HttpError) -> bool {
    match error {
        HttpError::Reqwest(_) => true,
        HttpError::Cached(error) => http_error_is_offline(error),
        HttpError::RefreshToken(RefreshTokenError::MatrixAuth(error)) => {
            http_error_is_offline(error)
        }
        HttpError::RefreshToken(
            RefreshTokenError::RefreshTokenRequired | RefreshTokenError::OAuth(_),
        ) => false,
        HttpError::Api(_) | HttpError::IntoHttp(_) => false,
        #[cfg(target_os = "android")]
        HttpError::VerifierBuilder(_) => false,
    }
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
        crate::utils::tracing::report_recoverable_error(
            "shell.sync",
            "emit_sync_status",
            "shell.sync_event_emit_failed",
            "sync",
            &error,
        );
    }
}

fn emit_session_deauthorized(app: &tauri::AppHandle, account_key: &str) {
    let payload = ShellSyncStatusPayload {
        account_key: account_key.to_owned(),
        state: String::from("deauthorized"),
        detail: None,
        updated_at_unix_ms: now_unix_ms(),
    };

    if let Err(error) = app.emit(SHELL_SESSION_DEAUTHORIZED_EVENT, payload) {
        crate::utils::tracing::report_recoverable_error(
            "shell.sync",
            "emit_session_deauthorization",
            "shell.sync_event_emit_failed",
            "sync",
            &error,
        );
    }
}

fn emit_session_reauthentication_required(app: &tauri::AppHandle, account_key: &str) {
    let payload = SessionReauthenticationRequiredPayload {
        account_key: account_key.to_owned(),
        state: "reauthentication_required",
        reason: "The server requires this session to sign in again.",
    };
    if let Err(error) = app.emit(SHELL_SESSION_REAUTHENTICATION_REQUIRED_EVENT, payload) {
        crate::utils::tracing::report_recoverable_error(
            "shell.sync",
            "emit_reauthentication_required",
            "shell.event_emit_failed",
            "event",
            &error,
        );
    }
}

pub(in crate::shell) fn emit_shell_room_updated(
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
        crate::utils::tracing::report_recoverable_error(
            "shell.sync",
            "emit_room_update",
            "shell.sync_event_emit_failed",
            "sync",
            &error,
        );
    }
}

pub(in crate::shell) fn emit_shell_timeline_updated(
    app: &tauri::AppHandle,
    account_key: &str,
    room_id: &str,
    items: Vec<RoomTimelineItem>,
    redacted_event_ids: Vec<String>,
) {
    let payload = ShellTimelineUpdatedPayload {
        account_key: account_key.to_owned(),
        room_id: room_id.to_owned(),
        items,
        redacted_event_ids,
        updated_at_unix_ms: now_unix_ms(),
    };

    if let Err(error) = app.emit(SHELL_TIMELINE_UPDATED_EVENT, payload) {
        crate::utils::tracing::report_recoverable_error(
            "shell.sync",
            "emit_timeline_update",
            "shell.sync_event_emit_failed",
            "sync",
            &error,
        );
    }
}

pub(in crate::shell) fn emit_shell_typing_updated(
    app: &tauri::AppHandle,
    account_key: &str,
    room_id: &str,
    users: Vec<String>,
) {
    let payload = ShellTypingUpdatedPayload {
        account_key: account_key.to_owned(),
        room_id: room_id.to_owned(),
        users,
        updated_at_unix_ms: now_unix_ms(),
    };

    if let Err(error) = app.emit(SHELL_TYPING_UPDATED_EVENT, payload) {
        crate::utils::tracing::report_recoverable_error(
            "shell.sync",
            "emit_typing_update",
            "shell.sync_event_emit_failed",
            "sync",
            &error,
        );
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
        crate::utils::tracing::report_recoverable_error(
            "shell.sync",
            "emit_room_list_update",
            "shell.sync_event_emit_failed",
            "sync",
            &error,
        );
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
            .map(ToString::to_string)
            .collect(),
        room_list_may_have_changed: !updates.is_empty(),
        updated_at_unix_ms: now_unix_ms(),
    };

    if let Err(error) = app.emit(SHELL_SYNC_UPDATED_EVENT, payload) {
        crate::utils::tracing::report_recoverable_error(
            "shell.sync",
            "emit_sync_update",
            "shell.sync_event_emit_failed",
            "sync",
            &error,
        );
    }
}

fn focused_room_id_from_state(
    focused_rooms: &RwLock<HashMap<String, FocusedRoomState>>,
    account_key: &str,
) -> Option<String> {
    focused_rooms
        .read()
        .expect("shell sync manager focused-rooms lock poisoned")
        .get(account_key)
        .map(|focused_room| focused_room.room_id.clone())
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use matrix_sdk::{
        Error as MatrixError, HttpError,
        ruma::api::{
            client::uiaa::UiaaResponse,
            error::{
                Error as RumaApiError, ErrorBody, ErrorKind, FromHttpResponseError,
                StandardErrorBody,
            },
        },
    };
    use matrix_sdk_ui::{
        encryption_sync_service, room_list_service, sync_service::Error as SyncServiceError,
    };
    use reqwest::StatusCode;

    use super::{
        ShellSyncErrorKind, shell_sync_error_kind, shell_sync_error_status, shell_sync_status_parts,
    };

    fn room_list_http_error(
        status_code: StatusCode,
        error_kind: ErrorKind,
        message: &str,
    ) -> SyncServiceError {
        let ruma_error = RumaApiError::new(
            status_code,
            ErrorBody::Standard(StandardErrorBody::new(error_kind, message.to_owned())),
        );
        let http_error = HttpError::Api(Box::new(FromHttpResponseError::Server(
            UiaaResponse::MatrixError(ruma_error),
        )));
        let matrix_error = MatrixError::Http(Box::new(http_error));

        SyncServiceError::RoomList(room_list_service::Error::SlidingSync(matrix_error))
    }

    #[test]
    fn classifies_unsupported_sliding_sync_endpoint_from_ruma_status_and_kind() {
        let error = room_list_http_error(
            StatusCode::NOT_FOUND,
            ErrorKind::Unrecognized,
            "the endpoint is not implemented",
        );

        assert_eq!(
            shell_sync_error_kind(&error),
            ShellSyncErrorKind::Unsupported
        );
        assert_eq!(shell_sync_error_status(&error), "unsupported");
    }

    #[test]
    fn does_not_classify_error_message_text() {
        let unrecognized_without_not_found = room_list_http_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorKind::Unrecognized,
            "M_UNRECOGNIZED and 404 are merely diagnostic text",
        );
        let unrelated_error_with_endpoint_message = room_list_http_error(
            StatusCode::NOT_FOUND,
            ErrorKind::Forbidden,
            "the sliding sync endpoint is unsupported",
        );

        assert_eq!(
            shell_sync_error_kind(&unrecognized_without_not_found),
            ShellSyncErrorKind::Error
        );
        assert_eq!(
            shell_sync_error_kind(&unrelated_error_with_endpoint_message),
            ShellSyncErrorKind::Error
        );
    }

    #[test]
    fn classifies_sdk_timeout_as_offline() {
        let error = SyncServiceError::EncryptionSync(encryption_sync_service::Error::SlidingSync(
            MatrixError::Timeout,
        ));

        assert_eq!(shell_sync_error_kind(&error), ShellSyncErrorKind::Offline);
        assert_eq!(shell_sync_error_status(&error), "offline");
    }

    #[test]
    fn classifies_transport_error_as_offline() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("test listener should report its address");
        drop(listener);

        let transport_error = tauri::async_runtime::block_on(async {
            reqwest::Client::new()
                .get(format!("http://{address}"))
                .send()
                .await
                .expect_err("closed local port should reject the request")
        });
        let error = SyncServiceError::EncryptionSync(encryption_sync_service::Error::ClientError(
            MatrixError::Http(Box::new(HttpError::Reqwest(transport_error))),
        ));

        assert_eq!(shell_sync_error_kind(&error), ShellSyncErrorKind::Offline);
    }

    #[test]
    fn classifies_non_matrix_sync_service_errors_as_error() {
        let error =
            SyncServiceError::RoomList(room_list_service::Error::UnknownList(String::from("all")));

        assert_eq!(shell_sync_error_kind(&error), ShellSyncErrorKind::Error);
        assert_eq!(shell_sync_error_status(&error), "error");
    }

    #[test]
    fn retains_the_existing_non_error_sync_statuses() {
        assert_eq!(
            shell_sync_status_parts(&matrix_sdk_ui::sync_service::State::Idle),
            ("idle", None)
        );
        assert_eq!(
            shell_sync_status_parts(&matrix_sdk_ui::sync_service::State::Running),
            ("running", None)
        );
        assert_eq!(
            shell_sync_status_parts(&matrix_sdk_ui::sync_service::State::Offline),
            ("offline", None)
        );
        assert_eq!(
            shell_sync_status_parts(&matrix_sdk_ui::sync_service::State::Terminated),
            ("terminated", None)
        );
    }
}
