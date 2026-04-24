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

use matrix_sdk::{
    Room, RoomState,
    latest_events::LatestEventValue,
    ruma::{
        RoomId,
        events::{
            AnyMessageLikeEventContent, MessageLikeEventType,
            fully_read::FullyReadEventContent,
            receipt::{ReceiptThread, ReceiptType},
            room::message::MessageType,
        },
    },
};

pub(super) fn resolve_room(client: &matrix_sdk::Client, room_id: &str) -> Result<Room, String> {
    let room_id = RoomId::parse(room_id).map_err(|error| format!("Invalid room id: {error}"))?;
    client
        .get_room(&room_id)
        .ok_or_else(|| format!("The room {room_id} could not be resolved"))
}

pub(super) fn local_room_state_key(account_key: &str, room_id: &str) -> String {
    format!("{account_key}::{room_id}")
}

pub(super) fn current_latest_event_id(room: &Room) -> Option<String> {
    room.latest_event()
        .and_then(|event| event.event_id().map(|event_id| event_id.to_string()))
}

pub(super) fn latest_activity_unix_ms(room: &Room) -> u64 {
    room.new_latest_event()
        .timestamp()
        .or_else(|| {
            room.latest_event()
                .and_then(|event| event.event().timestamp())
        })
        .map(|timestamp| u64::from(timestamp.0))
        .unwrap_or_default()
}

pub(super) fn latest_preview_text(room: &Room) -> Option<String> {
    let latest_event = room.new_latest_event();

    match latest_event {
        LatestEventValue::Remote(event) => event
            .raw()
            .deserialize()
            .ok()
            .and_then(message_preview_from_sync_event),
        LatestEventValue::LocalIsSending(local_event)
        | LatestEventValue::LocalCannotBeSent(local_event) => {
            message_preview_from_content(local_event.content.deserialize().ok()?)
        }
        LatestEventValue::None => room.latest_event().and_then(|event| {
            event
                .event()
                .raw()
                .deserialize()
                .ok()
                .and_then(message_preview_from_sync_event)
        }),
    }
}

fn message_preview_from_content(content: AnyMessageLikeEventContent) -> Option<String> {
    match content {
        AnyMessageLikeEventContent::RoomMessage(message) => match message.msgtype {
            MessageType::Text(text) => Some(text.body),
            MessageType::Notice(notice) => Some(notice.body),
            MessageType::Emote(emote) => Some(emote.body),
            _ => None,
        },
        _ => None,
    }
}

fn message_preview_from_sync_event(
    event: matrix_sdk::ruma::events::AnySyncTimelineEvent,
) -> Option<String> {
    let matrix_sdk::ruma::events::AnySyncTimelineEvent::MessageLike(message_like) = event else {
        return None;
    };

    let matrix_sdk::ruma::events::AnySyncMessageLikeEvent::RoomMessage(message) = message_like
    else {
        return None;
    };

    let original = message.as_original()?;

    match &original.content.msgtype {
        MessageType::Text(text) => Some(text.body.clone()),
        MessageType::Notice(notice) => Some(notice.body.clone()),
        MessageType::Emote(emote) => Some(emote.body.clone()),
        _ => None,
    }
}

pub(super) async fn persisted_read_anchor_event_id(room: &Room) -> Option<String> {
    let own_user_id = room.own_user_id();

    if let Ok(Some((event_id, _receipt))) = room
        .load_user_receipt(
            ReceiptType::ReadPrivate,
            ReceiptThread::Unthreaded,
            own_user_id,
        )
        .await
    {
        return Some(event_id.to_string());
    }

    if let Ok(Some((event_id, _receipt))) = room
        .load_user_receipt(ReceiptType::Read, ReceiptThread::Unthreaded, own_user_id)
        .await
    {
        return Some(event_id.to_string());
    }

    if let Ok(Some(raw_fully_read)) = room.account_data_static::<FullyReadEventContent>().await
        && let Ok(fully_read) = raw_fully_read.deserialize()
    {
        return Some(fully_read.content.event_id.to_string());
    }

    None
}

pub(super) async fn room_title(room: &Room) -> Result<String, String> {
    // Prefer the explicit room name over the SDK's computed display name.
    // `display_name()` is a useful fallback, but in small rooms it can resolve
    // to a hero/user-based label even when the room has a real `m.room.name`.
    if let Some(name) = room.name() {
        let trimmed_name = name.trim();
        if !trimmed_name.is_empty() {
            return Ok(trimmed_name.to_owned());
        }
    }

    if let Some(display_name) = room.cached_display_name() {
        let value = display_name.to_string();
        if !value.trim().is_empty() {
            return Ok(value);
        }
    }

    if let Ok(display_name) = room.display_name().await {
        let value = display_name.to_string();
        if !value.trim().is_empty() {
            return Ok(value);
        }
    }

    Ok(room
        .canonical_alias()
        .map(|alias| alias.to_string())
        .unwrap_or_else(|| room.room_id().to_string()))
}

pub(super) fn participant_label(room: &Room, is_direct: bool) -> String {
    if is_direct {
        String::from("Direct chat")
    } else {
        format!("{} members", room.active_members_count())
    }
}

pub(super) fn unread_message_count(room: &Room) -> u64 {
    let unread_messages = room.num_unread_messages();
    if unread_messages > 0 {
        return unread_messages;
    }

    let unread_notifications = room.unread_notification_counts().notification_count;
    if unread_notifications > 0 {
        return unread_notifications;
    }

    // Keep manually-marked unread rooms visible in the thread list even when
    // there is no computed count yet from read-receipt state or sync counters.
    if room.is_marked_unread() {
        return 1;
    }

    0
}

pub(super) fn homeserver_label(room: &Room, fallback_homeserver_url: &str) -> String {
    room.room_id()
        .server_name()
        .map(|server_name| server_name.to_string())
        .unwrap_or_else(|| {
            fallback_homeserver_url
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_end_matches('/')
                .to_owned()
        })
}

pub(super) async fn can_send_messages(room: &Room) -> bool {
    if !matches!(room.state(), RoomState::Joined) {
        return false;
    }

    let Ok(Some(member)) = room.get_member_no_sync(room.own_user_id()).await else {
        return true;
    };

    member.can_send_message(MessageLikeEventType::RoomMessage)
}

pub(super) async fn room_is_encrypted(room: &Room) -> bool {
    room.latest_encryption_state()
        .await
        .map(|state| state.is_encrypted())
        .unwrap_or(false)
}
