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

use matrix_sdk_ui::room_list_service::RoomListService;

use crate::shell::service::runtime::ShellTimelineService;

use super::{ShellSyncManager, ephemeral::TypingEphemeralStore, interests::FocusedRoomStore};

#[derive(Clone, Default)]
pub(in crate::shell::service) struct ShellSyncCoordinator {
    pub(super) sync_manager: ShellSyncManager,
    pub(super) timeline_service: ShellTimelineService,
    pub(super) focused_rooms: Arc<RwLock<FocusedRoomStore>>,
    pub(super) typing_ephemeral_state: Arc<RwLock<TypingEphemeralStore>>,
}

impl ShellSyncCoordinator {
    pub(in crate::shell::service) fn new() -> Self {
        Self {
            sync_manager: ShellSyncManager::new(),
            timeline_service: ShellTimelineService::new(),
            focused_rooms: Arc::new(RwLock::new(FocusedRoomStore::default())),
            typing_ephemeral_state: Arc::new(RwLock::new(TypingEphemeralStore::default())),
        }
    }
    pub(in crate::shell::service) fn room_list_service(
        &self,
        account_key: &str,
    ) -> Option<Arc<RoomListService>> {
        self.sync_manager.room_list_service(account_key)
    }
}
