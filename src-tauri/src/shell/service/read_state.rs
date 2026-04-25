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

use matrix_sdk::Room;

use super::{
    room::{
        current_latest_event_id, current_latest_event_is_own, local_room_state_key,
        persisted_read_anchor_event_id, unread_message_count,
    },
    timeline::count_recent_unread_messages_since,
};

pub(super) fn mark_room_read_locally(
    locally_read_room_state: &Arc<RwLock<HashMap<String, String>>>,
    account_key: &str,
    room_id: &str,
    event_id: &str,
) {
    locally_read_room_state
        .write()
        .expect("shell manager locally-read-room-state lock poisoned")
        .insert(
            local_room_state_key(account_key, room_id),
            event_id.to_owned(),
        );
}

pub(super) async fn unread_message_count_for_shell(
    locally_read_room_state: &Arc<RwLock<HashMap<String, String>>>,
    account_key: &str,
    room: &Room,
) -> u64 {
    if current_latest_event_is_own(room) {
        return 0;
    }

    let Some(read_anchor_event_id) =
        read_anchor_event_id(locally_read_room_state, account_key, room).await
    else {
        return unread_message_count(room);
    };

    let Some(latest_event_id) = current_latest_event_id(room) else {
        return 0;
    };

    if latest_event_id == read_anchor_event_id {
        return 0;
    }

    if let Some(local_unread_count) =
        count_recent_unread_messages_since(room, &read_anchor_event_id).await
    {
        return local_unread_count.max(1);
    }

    unread_message_count(room)
}

async fn read_anchor_event_id(
    locally_read_room_state: &Arc<RwLock<HashMap<String, String>>>,
    account_key: &str,
    room: &Room,
) -> Option<String> {
    let local_anchor_event_id = locally_read_room_state
        .read()
        .expect("shell manager locally-read-room-state lock poisoned")
        .get(&local_room_state_key(account_key, room.room_id().as_str()))
        .cloned();
    let persisted_anchor_event_id = persisted_read_anchor_event_id(room).await;

    let current_latest_event_id = current_latest_event_id(room);
    if current_latest_event_id.is_some() && persisted_anchor_event_id == current_latest_event_id {
        return persisted_anchor_event_id;
    }

    if current_latest_event_id.is_some() && local_anchor_event_id == current_latest_event_id {
        return local_anchor_event_id;
    }

    persisted_anchor_event_id.or(local_anchor_event_id)
}
