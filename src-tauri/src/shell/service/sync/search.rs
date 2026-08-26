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

use crate::shell::service::room::timeline::{
    TimelineSearchUpdates, fetch_room_timeline_search_updates,
};

use super::{coordinator::ShellSyncCoordinator, diagnostics::emit_timeline_room_diagnostic};

impl ShellSyncCoordinator {
    pub(in crate::shell::service) async fn fetch_search_backfill_updates(
        &self,
        account_key: &str,
        room: &Room,
        limit: u16,
        from: Option<&str>,
    ) -> Result<TimelineSearchUpdates, String> {
        emit_timeline_room_diagnostic("timeline.search_backfill.load", account_key, room);
        fetch_room_timeline_search_updates(room, limit, from).await
    }
}
