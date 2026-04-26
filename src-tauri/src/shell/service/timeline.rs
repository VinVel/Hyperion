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
    Room,
    room::MessagesOptions,
    ruma::{
        api::client::filter::RoomEventFilter,
        events::{
            AnySyncMessageLikeEvent, AnySyncTimelineEvent,
            room::message::{MessageType, SyncRoomMessageEvent},
        },
    },
};

use super::{super::types::RoomTimelineItem, room::resolve_room};

// Unread badges need a recent history fallback when the room list has updated
// but the local event cache for an unfocused room is still too cold to count.
const UNREAD_COUNT_BACKFILL_LIMIT: u16 = 80;
// Keep unread backfill bounded; if the read anchor is older than this window,
// fall back to the SDK aggregate instead of scanning deep history in room lists.
const UNREAD_COUNT_BACKFILL_MAX_PAGES: usize = 2;

pub(super) async fn warm_room_recent_timeline(
    client: &matrix_sdk::Client,
    room_id: &str,
    target_limit: u16,
) -> Result<(), String> {
    let room = resolve_room(client, room_id)?;

    if cached_timeline_item_count(&room).await.unwrap_or(0) >= usize::from(target_limit) {
        return Ok(());
    }

    fetch_room_timeline_chunk(&room, target_limit, None)
        .await
        .map_err(|error| format!("Failed to warm the recent room timeline: {error}"))?;

    Ok(())
}

pub(super) async fn count_unread_messages_since(room: &Room, event_id: &str) -> Option<u64> {
    let items = cached_timeline_items(room).await.ok()?;
    let read_event_index = items.iter().position(|item| item.event_id == event_id)?;

    Some(
        items
            .iter()
            .skip(read_event_index + 1)
            .filter(|item| !item.is_own_message)
            .count() as u64,
    )
}

pub(super) async fn count_recent_unread_messages_since(room: &Room, event_id: &str) -> Option<u64> {
    if let Some(cached_count) = count_unread_messages_since(room, event_id).await {
        return Some(cached_count);
    }

    let mut from = None;
    let mut unread_count = 0_u64;

    for _ in 0..UNREAD_COUNT_BACKFILL_MAX_PAGES {
        let (items, next_from) =
            fetch_room_timeline_chunk(room, UNREAD_COUNT_BACKFILL_LIMIT, from.as_deref())
                .await
                .ok()?;

        if let Some(read_event_index) = items.iter().position(|item| item.event_id == event_id) {
            unread_count += items
                .iter()
                .skip(read_event_index + 1)
                .filter(|item| !item.is_own_message)
                .count() as u64;
            return Some(unread_count);
        }

        unread_count += items.iter().filter(|item| !item.is_own_message).count() as u64;

        let next_from = next_from?;
        from = Some(next_from);
    }

    None
}

pub(super) async fn cached_timeline_item_count(room: &Room) -> Option<usize> {
    cached_timeline_items(room)
        .await
        .ok()
        .map(|items| items.len())
}

pub(super) async fn cached_timeline_items(room: &Room) -> Result<Vec<RoomTimelineItem>, String> {
    let Ok((room_event_cache, _drop_handles)) = room.event_cache().await else {
        return Ok(Vec::new());
    };

    let events = room_event_cache
        .events()
        .await
        .map_err(|error| format!("Failed to inspect the room event cache: {error}"))?;

    Ok(events
        .iter()
        .filter_map(|event| timeline_item_from_timeline_event(event, room.own_user_id()))
        .collect())
}

pub(super) async fn fetch_room_timeline_chunk(
    room: &Room,
    limit: u16,
    from: Option<&str>,
) -> Result<(Vec<RoomTimelineItem>, Option<String>), String> {
    let response = room
        .messages(backward_shell_timeline_options(limit, from))
        .await
        .map_err(|error| format!("Failed to load room timeline: {error}"))?;

    let mut items = response
        .chunk
        .into_iter()
        .filter_map(|event| timeline_item_from_timeline_event(&event, room.own_user_id()))
        .collect::<Vec<_>>();
    items.reverse();

    Ok((items, response.end))
}

fn backward_shell_timeline_options(limit: u16, from: Option<&str>) -> MessagesOptions {
    let mut options = MessagesOptions::backward();
    options.limit = limit.into();
    let mut filter = RoomEventFilter::default();
    // The shell timeline only renders message-like content, and encrypted
    // room messages still arrive as `m.room.encrypted` before decryption.
    filter.types = Some(vec![
        String::from("m.room.message"),
        String::from("m.room.encrypted"),
    ]);
    options.filter = filter;

    if let Some(from) = from {
        options = options.from(from);
    }

    options
}

pub(super) fn timeline_item_from_timeline_event(
    event: &matrix_sdk::deserialized_responses::TimelineEvent,
    own_user_id: &matrix_sdk::ruma::UserId,
) -> Option<RoomTimelineItem> {
    let raw_event = event.raw();
    let parsed = raw_event.deserialize().ok()?;
    let (event_id, sender_id, sender_display_name, body, is_edited) =
        message_fields_from_sync_event(&parsed)?;

    Some(RoomTimelineItem {
        event_id,
        sender_id: sender_id.clone(),
        sender_display_name,
        body,
        timestamp_unix_ms: event
            .timestamp()
            .map_or(0, |timestamp| u64::from(timestamp.0)),
        is_edited: Some(is_edited),
        is_own_message: sender_id == own_user_id.as_str(),
    })
}

fn message_fields_from_sync_event(
    event: &AnySyncTimelineEvent,
) -> Option<(String, String, Option<String>, String, bool)> {
    match event {
        AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(message)) => {
            let SyncRoomMessageEvent::Original(original) = message else {
                return None;
            };

            let MessageType::Text(text) = &original.content.msgtype else {
                return None;
            };

            Some((
                original.event_id.to_string(),
                original.sender.to_string(),
                None,
                text.body.clone(),
                original
                    .content
                    .relates_to
                    .as_ref()
                    .and_then(matrix_sdk::ruma::events::room::message::Relation::rel_type)
                    .is_some_and(|relation_type| {
                        relation_type
                            == matrix_sdk::ruma::events::relation::RelationType::Replacement
                    }),
            ))
        }
        _ => None,
    }
}
