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

use std::sync::Arc;

use futures_util::{StreamExt, pin_mut};
use matrix_sdk::Room;
use matrix_sdk_ui::room_list_service::{RoomListService, filters};

use super::{super::sync::ShellSyncManager, ROOM_LIST_SNAPSHOT_PAGE_SIZE, ShellRoomListKind};

pub(super) async fn snapshot_room_list_for_account(
    sync_manager: &ShellSyncManager,
    account_key: &str,
    list_kind: ShellRoomListKind,
) -> Result<Vec<Room>, String> {
    let room_list_service = sync_manager
        .room_list_service(account_key)
        .ok_or_else(|| String::from("The active shell room list service is not available"))?;

    snapshot_room_list(room_list_service, list_kind).await
}

async fn snapshot_room_list(
    room_list_service: Arc<RoomListService>,
    list_kind: ShellRoomListKind,
) -> Result<Vec<Room>, String> {
    let room_list = room_list_service
        .all_rooms()
        .await
        .map_err(|error| format!("Failed to access the shell room list: {error}"))?;
    let (entries, entries_controller) =
        room_list.entries_with_dynamic_adapters(ROOM_LIST_SNAPSHOT_PAGE_SIZE);

    let filter = match list_kind {
        ShellRoomListKind::Conversations => Box::new(filters::new_filter_all(vec![
            Box::new(filters::new_filter_joined()),
            Box::new(filters::new_filter_not(Box::new(
                filters::new_filter_space(),
            ))),
        ])),
        ShellRoomListKind::Spaces => Box::new(filters::new_filter_all(vec![
            Box::new(filters::new_filter_joined()),
            Box::new(filters::new_filter_space()),
        ])),
    };
    let _ = entries_controller.set_filter(filter);

    pin_mut!(entries);
    let diffs = entries
        .next()
        .await
        .ok_or_else(|| String::from("The shell room list stream ended unexpectedly"))?;

    Ok(diffs
        .into_iter()
        .find_map(|diff| match diff {
            eyeball_im::VectorDiff::Reset { values } => Some(
                values
                    .into_iter()
                    .map(
                        |room_list_item: matrix_sdk_ui::room_list_service::RoomListItem| {
                            room_list_item.into_inner()
                        },
                    )
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .unwrap_or_default())
}
