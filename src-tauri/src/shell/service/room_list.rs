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

use std::{sync::Arc, time::Duration};

use futures_util::{
    StreamExt,
    future::{Either, select},
    pin_mut,
};
use matrix_sdk::{Room, sleep::sleep};
use matrix_sdk_ui::room_list_service::{RoomListLoadingState, RoomListService, filters};

use super::{super::sync::ShellSyncManager, ROOM_LIST_SNAPSHOT_PAGE_SIZE, ShellRoomListKind};

// The first shell load may race SyncService startup. Wait briefly for the SDK
// room list to report that an initial sync has populated the list metadata.
const ROOM_LIST_INITIAL_LOAD_TIMEOUT_MS: u64 = 3_000;

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

    wait_for_room_list_initial_load(&room_list).await;

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

async fn wait_for_room_list_initial_load(room_list: &matrix_sdk_ui::room_list_service::RoomList) {
    let mut loading_state = room_list.loading_state();

    let wait_for_loaded = async {
        while let Some(state) = loading_state.next().await {
            if matches!(state, RoomListLoadingState::Loaded { .. }) {
                break;
            }
        }
    };

    pin_mut!(wait_for_loaded);
    let timeout = sleep(Duration::from_millis(ROOM_LIST_INITIAL_LOAD_TIMEOUT_MS));
    pin_mut!(timeout);

    match select(wait_for_loaded, timeout).await {
        Either::Left((_, _)) | Either::Right((_, _)) => (),
    }
}
