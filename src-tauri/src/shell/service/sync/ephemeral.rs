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

use std::sync::{Arc, RwLock};

use matrix_sdk::Room;
use tauri::async_runtime::JoinHandle;

use super::{
    coordinator::ShellSyncCoordinator,
    diagnostics::emit_sync_diagnostic,
    emit_shell_typing_updated,
    interests::{EPHEMERAL_TYPING_OWNER, RoomFocusMode, RoomInterestKind},
};

#[derive(Default)]
pub(super) struct TypingEphemeralStore {
    pub(super) accounts: std::collections::HashMap<String, AccountTypingState>,
}

#[derive(Default)]
pub(super) struct AccountTypingState {
    pub(super) rooms: std::collections::HashMap<String, RoomTypingState>,
}

#[derive(Default)]
pub(super) struct RoomTypingState {
    pub(super) users: Vec<String>,
    pub(super) app: Option<tauri::AppHandle>,
    pub(super) subscription_reserved: bool,
    pub(super) subscription_handle: Option<JoinHandle<()>>,
    pub(super) last_notice_is_typing: Option<bool>,
}

impl ShellSyncCoordinator {
    pub(super) fn reserve_typing_subscription(
        &self,
        app: Option<tauri::AppHandle>,
        account_key: &str,
        room_id: &str,
    ) -> bool {
        let mut store = self
            .typing_ephemeral_state
            .write()
            .expect("shell sync coordinator typing-state lock poisoned");
        let room_state = store
            .accounts
            .entry(account_key.to_owned())
            .or_default()
            .rooms
            .entry(room_id.to_owned())
            .or_default();
        if app.is_some() {
            room_state.app = app;
        }

        let already_subscribed =
            room_state.subscription_reserved || room_state.subscription_handle.is_some();
        if !already_subscribed {
            room_state.subscription_reserved = true;
        }
        drop(store);

        already_subscribed
    }
    fn install_typing_subscription(
        &self,
        account_key: &str,
        room_id: &str,
        handle: JoinHandle<()>,
    ) {
        let mut pending_handle = Some(handle);
        {
            let mut store = self
                .typing_ephemeral_state
                .write()
                .expect("shell sync coordinator typing-state lock poisoned");
            if let Some(room_state) = store
                .accounts
                .get_mut(account_key)
                .and_then(|account_state| account_state.rooms.get_mut(room_id))
            {
                room_state.subscription_handle = pending_handle.take();
                room_state.subscription_reserved = false;
            }
        }

        if let Some(handle) = pending_handle {
            handle.abort();
        }
    }
    fn record_typing_notice(
        &self,
        account_key: &str,
        room_id: &str,
        is_typing: bool,
    ) -> Option<&'static str> {
        let mut store = self
            .typing_ephemeral_state
            .write()
            .expect("shell sync coordinator typing-state lock poisoned");
        let room_state = store
            .accounts
            .entry(account_key.to_owned())
            .or_default()
            .rooms
            .entry(room_id.to_owned())
            .or_default();
        let previous_notice = room_state.last_notice_is_typing.map(typing_bool_label);
        room_state.last_notice_is_typing = Some(is_typing);
        drop(store);
        previous_notice
    }
    pub(super) fn release_ephemeral_typing_for_room(
        &self,
        account_key: &str,
        room_id: &str,
        reason: &str,
    ) {
        self.release_room_interest(
            account_key,
            room_id,
            RoomInterestKind::EphemeralTyping,
            EPHEMERAL_TYPING_OWNER,
        );
        self.clear_typing_room_state(account_key, room_id, reason);
    }
    pub(super) fn clear_typing_room_state(&self, account_key: &str, room_id: &str, reason: &str) {
        let removed_room = {
            let mut store = self
                .typing_ephemeral_state
                .write()
                .expect("shell sync coordinator typing-state lock poisoned");
            let removed_room = store
                .accounts
                .get_mut(account_key)
                .and_then(|account_state| account_state.rooms.remove(room_id));

            if store
                .accounts
                .get(account_key)
                .is_some_and(|account_state| account_state.rooms.is_empty())
            {
                store.accounts.remove(account_key);
            }

            removed_room
        };

        let Some(mut room_state) = removed_room else {
            return;
        };

        if let Some(handle) = room_state.subscription_handle.take() {
            handle.abort();
            emit_sync_diagnostic(
                "sync.ephemeral.typing.unsubscribe",
                &[
                    ("account_key", account_key),
                    ("room_id", room_id),
                    ("reason", reason),
                ],
            );
        }

        let typing_user_count = room_state.users.len().to_string();
        emit_sync_diagnostic(
            "sync.ephemeral.typing.clear",
            &[
                ("account_key", account_key),
                ("room_id", room_id),
                ("reason", reason),
                ("typing_user_count", &typing_user_count),
            ],
        );
        if !room_state.users.is_empty()
            && let Some(app) = room_state.app.as_ref()
        {
            emit_shell_typing_updated(app, account_key, room_id, Vec::new());
        }
    }
    pub(super) fn clear_account_typing_state(&self, account_key: &str, reason: &str) {
        let room_ids = {
            let store = self
                .typing_ephemeral_state
                .read()
                .expect("shell sync coordinator typing-state lock poisoned");
            store
                .accounts
                .get(account_key)
                .map(|account_state| account_state.rooms.keys().cloned().collect::<Vec<String>>())
                .unwrap_or_default()
        };

        for room_id in room_ids {
            self.clear_typing_room_state(account_key, &room_id, reason);
        }
    }
    pub(super) fn clear_inactive_account_typing_state(&self, active_account_key: &str) {
        let inactive_account_keys = {
            let store = self
                .typing_ephemeral_state
                .read()
                .expect("shell sync coordinator typing-state lock poisoned");
            store
                .accounts
                .keys()
                .filter(|account_key| account_key.as_str() != active_account_key)
                .cloned()
                .collect::<Vec<String>>()
        };

        for account_key in inactive_account_keys {
            self.clear_account_typing_state(&account_key, "inactive_account");
        }
    }
    pub(super) fn clear_all_typing_state(&self, reason: &str) {
        let account_keys = {
            let store = self
                .typing_ephemeral_state
                .read()
                .expect("shell sync coordinator typing-state lock poisoned");
            store.accounts.keys().cloned().collect::<Vec<String>>()
        };

        for account_key in account_keys {
            self.clear_account_typing_state(&account_key, reason);
        }
    }
    pub(in crate::shell::service) fn subscribe_typing_updates(
        &self,
        app: tauri::AppHandle,
        account_key: &str,
        room: &Room,
    ) {
        self.observe_room(
            account_key,
            room.room_id().as_str(),
            RoomInterestKind::EphemeralTyping,
            EPHEMERAL_TYPING_OWNER,
            "typing subscription",
            RoomFocusMode::Observed,
        );
        let room_id = room.room_id().to_string();
        let already_subscribed =
            self.reserve_typing_subscription(Some(app.clone()), account_key, &room_id);
        emit_sync_diagnostic(
            "sync.ephemeral.typing.subscribe",
            &[
                ("account_key", account_key),
                ("room_id", &room_id),
                (
                    "already_subscribed",
                    if already_subscribed { "true" } else { "false" },
                ),
            ],
        );
        if already_subscribed {
            return;
        }

        let (drop_guard, mut subscriber) = room.subscribe_to_typing_notifications();
        let task_account_key = account_key.to_owned();
        let task_room_id = room_id.clone();
        let state = self.typing_ephemeral_state.clone();
        let handle = tauri::async_runtime::spawn(async move {
            let _subscription_guard = drop_guard;

            while let Ok(typing_user_ids) = subscriber.recv().await {
                let users = typing_user_ids
                    .into_iter()
                    .map(|user_id| user_id.to_string())
                    .collect::<Vec<String>>();
                update_typing_users(&state, &app, &task_account_key, &task_room_id, users);
            }

            clear_finished_typing_subscription(&state, &task_account_key, &task_room_id);
            emit_sync_diagnostic(
                "sync.ephemeral.typing.unsubscribe",
                &[
                    ("account_key", &task_account_key),
                    ("room_id", &task_room_id),
                    ("reason", "receiver_closed"),
                ],
            );
        });
        self.install_typing_subscription(account_key, &room_id, handle);
    }
    pub(in crate::shell::service) async fn send_typing_notice(
        &self,
        account_key: &str,
        room: &Room,
        is_typing: bool,
    ) -> Result<(), String> {
        let previous_notice =
            self.record_typing_notice(account_key, room.room_id().as_str(), is_typing);
        emit_sync_diagnostic(
            "sync.ephemeral.typing.notice",
            &[
                ("account_key", account_key),
                ("room_id", room.room_id().as_str()),
                ("is_typing", if is_typing { "true" } else { "false" }),
                ("previous_is_typing", previous_notice.unwrap_or("none")),
            ],
        );
        room.typing_notice(is_typing)
            .await
            .map_err(|error| format!("Failed to update typing notice: {error}"))
    }
}

fn update_typing_users(
    state: &Arc<RwLock<TypingEphemeralStore>>,
    app: &tauri::AppHandle,
    account_key: &str,
    room_id: &str,
    users: Vec<String>,
) {
    let typing_user_count = users.len().to_string();
    {
        let mut store = state
            .write()
            .expect("shell sync coordinator typing-state lock poisoned");
        let room_state = store
            .accounts
            .entry(account_key.to_owned())
            .or_default()
            .rooms
            .entry(room_id.to_owned())
            .or_default();
        room_state.app = Some(app.clone());
        room_state.users.clone_from(&users);
        drop(store);
    }

    emit_sync_diagnostic(
        "sync.ephemeral.typing.update",
        &[
            ("account_key", account_key),
            ("room_id", room_id),
            ("typing_user_count", &typing_user_count),
        ],
    );
    emit_shell_typing_updated(app, account_key, room_id, users);
}

fn clear_finished_typing_subscription(
    state: &Arc<RwLock<TypingEphemeralStore>>,
    account_key: &str,
    room_id: &str,
) {
    let mut store = state
        .write()
        .expect("shell sync coordinator typing-state lock poisoned");
    let Some(room_state) = store
        .accounts
        .get_mut(account_key)
        .and_then(|account_state| account_state.rooms.get_mut(room_id))
    else {
        return;
    };

    room_state.subscription_reserved = false;
    room_state.subscription_handle = None;
    drop(store);
}

fn typing_bool_label(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}
