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

mod account;
mod actions;
mod coordinator;
mod diagnostics;
mod ephemeral;
mod interests;
pub(in crate::shell) mod matrix_sdk;
mod search;
mod timeline;

#[cfg(test)]
mod tests;

pub(in crate::shell::service) use coordinator::ShellSyncCoordinator;
pub(in crate::shell::service) use interests::{RoomFocusMode, RoomInterestKind};
pub(in crate::shell) use matrix_sdk::{
    ShellSyncManager, emit_shell_room_updated, emit_shell_timeline_updated,
    emit_shell_typing_updated,
};
