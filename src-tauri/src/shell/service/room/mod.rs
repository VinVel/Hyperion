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
            AnyMessageLikeEventContent, AnySyncMessageLikeEvent, AnySyncStateEvent,
            AnySyncTimelineEvent, MessageLikeEventType,
            fully_read::FullyReadEventContent,
            receipt::{ReceiptThread, ReceiptType},
            room::message::MessageType,
        },
    },
};

mod commands;
pub(super) mod list;
pub(super) mod timeline;

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
    let latest_event = room.latest_event();
    let event_id = latest_event.event_id()?;

    Some(event_id.to_string())
}

pub(super) fn current_latest_event_is_own(room: &Room) -> bool {
    let latest_event = room.latest_event();

    match latest_event {
        LatestEventValue::Remote(event) => {
            let Ok(event) = event.raw().deserialize() else {
                return false;
            };

            event.sender() == room.own_user_id()
        }
        LatestEventValue::LocalIsSending(_)
        | LatestEventValue::LocalHasBeenSent { .. }
        | LatestEventValue::LocalCannotBeSent(_) => true,
        LatestEventValue::None | LatestEventValue::RemoteInvite { .. } => false,
    }
}

pub(super) fn latest_activity_unix_ms(room: &Room) -> u64 {
    room.latest_event()
        .timestamp()
        .map(|timestamp| u64::from(timestamp.0))
        .unwrap_or_default()
}

pub(super) fn latest_preview_text(room: &Room) -> Option<String> {
    let latest_event = room.latest_event();

    match latest_event {
        LatestEventValue::Remote(event) => {
            let raw_event = event.raw();
            let event = raw_event.deserialize().ok()?;
            Some(preview_from_sync_event(event))
        }
        LatestEventValue::RemoteInvite { .. } => Some(String::from("Room invite")),
        LatestEventValue::LocalIsSending(local_event)
        | LatestEventValue::LocalHasBeenSent {
            value: local_event, ..
        }
        | LatestEventValue::LocalCannotBeSent(local_event) => Some(preview_from_content(
            local_event.content.deserialize().ok()?,
        )),
        LatestEventValue::None => None,
    }
}

fn preview_from_content(content: AnyMessageLikeEventContent) -> String {
    match content {
        AnyMessageLikeEventContent::RoomMessage(message) => match message.msgtype {
            MessageType::Text(text) => text.body,
            MessageType::Notice(notice) => notice.body,
            MessageType::Emote(emote) => emote.body,
            message_type => preview_from_message_type(&message_type),
        },
        _ => String::from("Room activity"),
    }
}

fn preview_from_sync_event(event: AnySyncTimelineEvent) -> String {
    match event {
        AnySyncTimelineEvent::MessageLike(message_like) => {
            preview_from_sync_message_like_event(&message_like)
        }
        AnySyncTimelineEvent::State(state) => preview_from_sync_state_event(&state),
    }
}

fn preview_from_sync_message_like_event(event: &AnySyncMessageLikeEvent) -> String {
    if let AnySyncMessageLikeEvent::RoomMessage(message) = event
        && let Some(original) = message.as_original()
    {
        return match &original.content.msgtype {
            MessageType::Text(text) => text.body.clone(),
            MessageType::Notice(notice) => notice.body.clone(),
            MessageType::Emote(emote) => emote.body.clone(),
            message_type => preview_from_message_type(message_type),
        };
    }

    preview_from_message_like_event_type(&event.event_type().to_string())
}

fn preview_from_message_type(message_type: &MessageType) -> String {
    match message_type {
        MessageType::Audio(_) => String::from("Audio message"),
        MessageType::File(_) => String::from("File shared"),
        MessageType::Image(_) => String::from("Image shared"),
        MessageType::Location(_) => String::from("Location shared"),
        MessageType::ServerNotice(_) => String::from("Server notice"),
        MessageType::VerificationRequest(_) => String::from("Verification request"),
        MessageType::Video(_) => String::from("Video shared"),
        MessageType::Text(_) | MessageType::Notice(_) | MessageType::Emote(_) => {
            String::from("Message")
        }
        _ => String::from("Message"),
    }
}

fn preview_from_message_like_event_type(event_type: &str) -> String {
    match event_type {
        "m.location" => String::from("Live location shared"),
        "m.beacon" | "org.matrix.msc3672.beacon" => String::from("Live location updated"),
        "m.reaction" => String::from("Reaction"),
        "m.room.encrypted" => String::from("Encrypted message"),
        "m.room.redaction" => String::from("Message removed"),
        "m.sticker" => String::from("Sticker"),
        value if value.starts_with("m.call.") || value.contains(".call.") => {
            String::from("Call activity")
        }
        value if value.starts_with("m.key.verification.") => String::from("Verification activity"),
        value if value.contains("poll.") => String::from("Poll activity"),
        _ => String::from("Room activity"),
    }
}

fn preview_from_sync_state_event(event: &AnySyncStateEvent) -> String {
    match event.event_type().to_string().as_str() {
        "m.room.avatar" | "m.room.canonical_alias" | "m.room.name" | "m.room.topic" => {
            String::from("Room details updated")
        }
        "m.room.member" => String::from("Room membership updated"),
        "m.room.encryption" => String::from("Encryption updated"),
        "m.beacon_info" | "org.matrix.msc3672.beacon_info" => String::from("Live location shared"),
        "m.room.guest_access"
        | "m.room.history_visibility"
        | "m.room.join_rules"
        | "m.room.pinned_events"
        | "m.room.power_levels"
        | "m.room.server_acl" => String::from("Room settings updated"),
        _ => String::from("Room activity"),
    }
}

pub(super) async fn persisted_read_anchor_event_id(room: &Room) -> Option<String> {
    let own_user_id = room.own_user_id();

    if let Ok(Some((event_id, _read_private_receipt))) = room
        .load_user_receipt(
            ReceiptType::ReadPrivate,
            ReceiptThread::Unthreaded,
            own_user_id,
        )
        .await
    {
        return Some(event_id.to_string());
    }

    if let Ok(Some((event_id, _read_receipt))) = room
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

    let room_alias = room.canonical_alias();
    let title = room_alias.map_or_else(|| room.room_id().to_string(), |alias| alias.to_string());

    Ok(title)
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
    if let Some(server_name) = room.room_id().server_name() {
        return server_name.to_string();
    }

    let homeserver = fallback_homeserver_url.trim_start_matches("https://");
    let homeserver = homeserver.trim_start_matches("http://");
    let homeserver = homeserver.trim_end_matches('/');

    homeserver.to_owned()
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
