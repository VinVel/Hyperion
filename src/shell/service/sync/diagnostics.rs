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

use crate::shell::service::ShellRoomListKind;

impl ShellRoomListKind {
    pub(super) fn label(&self) -> &'static str {
        match self {
            Self::Conversations => "conversations",
            Self::Spaces => "spaces",
        }
    }
}

pub(super) fn emit_timeline_room_diagnostic(label: &'static str, account_key: &str, room: &Room) {
    emit_sync_diagnostic(
        label,
        &[
            ("account_key", account_key),
            ("room_id", room.room_id().as_str()),
        ],
    );
}

pub(super) fn emit_sync_diagnostic(label: &'static str, fields: &[(&str, &str)]) {
    if !tracing::enabled!(target: "hyperion", tracing::Level::TRACE) {
        return;
    }

    #[cfg(debug_assertions)]
    let Some(rendered_fields) =
        crate::utils::tracing::changed_diagnostic_fields("shell.sync", label, fields)
    else {
        return;
    };
    #[cfg(debug_assertions)]
    tracing::trace!(
        target: "hyperion",
        event_name = label,
        component = "shell.sync",
        diagnostic_details = %rendered_fields,
        "{label}: {rendered_fields}"
    );

    #[cfg(not(debug_assertions))]
    {
        if crate::utils::tracing::changed_diagnostic_fields("shell.sync", label, fields).is_none() {
            return;
        }
        tracing::trace!(
            target: "hyperion",
            event_name = label,
            component = "shell.sync",
            "{label}"
        );
    }
}
