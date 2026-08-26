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

use std::collections::HashMap;

use super::{coordinator::ShellSyncCoordinator, diagnostics::emit_sync_diagnostic};

/// The sole synchronization priority selected by the visible room UI.
#[derive(Clone, Debug, Default)]
pub(super) struct FocusedRoomStore {
    pub(super) rooms: HashMap<String, String>,
}

impl ShellSyncCoordinator {
    pub(in crate::shell::service) fn set_focused_room(&self, account_key: &str, room_id: &str) {
        let previous_room_id = self
            .focused_rooms
            .write()
            .expect("shell sync focused-room lock poisoned")
            .rooms
            .insert(account_key.to_owned(), room_id.to_owned());
        if previous_room_id.as_deref() == Some(room_id) {
            return;
        }
        if let Some(previous_room_id) = previous_room_id.as_deref() {
            self.clear_typing_room_state(account_key, previous_room_id, "focus_changed");
            emit_sync_diagnostic(
                "sync.room.focus.clear",
                &[("account_key", account_key), ("room_id", previous_room_id)],
            );
        }
        self.sync_manager.set_focused_room(account_key, room_id);
        emit_sync_diagnostic(
            "sync.room.focus.set",
            &[("account_key", account_key), ("room_id", room_id)],
        );
    }

    pub(super) fn focused_room_id(&self, account_key: &str) -> Option<String> {
        self.focused_rooms
            .read()
            .expect("shell sync focused-room lock poisoned")
            .rooms
            .get(account_key)
            .cloned()
    }

    pub(super) fn clear_inactive_account_focus(&self, active_account_key: &str) {
        let account_keys = self
            .focused_rooms
            .read()
            .expect("shell sync focused-room lock poisoned")
            .rooms
            .keys()
            .filter(|key| key.as_str() != active_account_key)
            .cloned()
            .collect::<Vec<String>>();
        for account_key in account_keys {
            self.clear_account_focus(&account_key);
        }
    }

    pub(super) fn clear_account_focus(&self, account_key: &str) {
        let room_id = self
            .focused_rooms
            .write()
            .expect("shell sync focused-room lock poisoned")
            .rooms
            .remove(account_key);
        if let Some(room_id) = room_id {
            self.sync_manager.clear_focused_room(account_key, &room_id);
            emit_sync_diagnostic(
                "sync.room.focus.clear",
                &[("account_key", account_key), ("room_id", &room_id)],
            );
        }
    }

    pub(super) fn clear_all_account_focus(&self) {
        let account_keys = self
            .focused_rooms
            .read()
            .expect("shell sync focused-room lock poisoned")
            .rooms
            .keys()
            .cloned()
            .collect::<Vec<String>>();
        for account_key in account_keys {
            self.clear_account_focus(&account_key);
        }
    }
}
