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
            relation::RelationType,
            room::message::{MessageType, Relation, SyncRoomMessageEvent},
        },
        room_version_rules::RedactionRules,
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

    for _backfill_page_index in 0..UNREAD_COUNT_BACKFILL_MAX_PAGES {
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
    let items = cached_timeline_items(room).await.ok()?;

    Some(items.len())
}

pub(super) async fn cached_timeline_items(room: &Room) -> Result<Vec<RoomTimelineItem>, String> {
    let Ok((room_event_cache, _drop_handles)) = room.event_cache().await else {
        return Ok(Vec::new());
    };

    let events = room_event_cache
        .events()
        .await
        .map_err(|error| format!("Failed to inspect the room event cache: {error}"))?;

    let items = events
        .iter()
        .filter_map(|event| timeline_item_from_timeline_event(event, room.own_user_id()))
        .collect();

    Ok(items)
}

pub(super) async fn fetch_room_timeline_chunk(
    room: &Room,
    limit: u16,
    from: Option<&str>,
) -> Result<(Vec<RoomTimelineItem>, Option<String>), String> {
    let response = room
        .messages(backward_shell_timeline_options(
            limit,
            from,
            TimelineChunkFilter::MessagesOnly,
        ))
        .await
        .map_err(|error| format!("Failed to load room timeline: {error}"))?;

    let mut items = response
        .chunk
        .into_iter()
        .filter_map(|event| {
            let own_user_id = room.own_user_id();
            timeline_item_from_timeline_event(&event, own_user_id)
        })
        .collect::<Vec<_>>();
    items.reverse();

    Ok((items, response.end))
}

pub(super) async fn fetch_room_timeline_search_updates(
    room: &Room,
    limit: u16,
    from: Option<&str>,
) -> Result<TimelineSearchUpdates, String> {
    let response = room
        .messages(backward_shell_timeline_options(
            limit,
            from,
            TimelineChunkFilter::MessagesAndRedactions,
        ))
        .await
        .map_err(|error| format!("Failed to load room timeline: {error}"))?;

    let own_user_id = room.own_user_id();
    let mut updates = TimelineSearchUpdates::default();
    for event in response.chunk {
        let raw_event = event.raw();
        let Ok(parsed_event) = raw_event.deserialize() else {
            continue;
        };

        if let Some(item) = timeline_item_from_sync_event(&event, &parsed_event, own_user_id) {
            updates.items.push(item);
        }
        if let Some(redacted_event_id) = redacted_event_id_from_sync_event(&parsed_event) {
            updates.redacted_event_ids.push(redacted_event_id);
        }
    }

    updates.items.reverse();
    updates.next_token = response.end;
    Ok(updates)
}

#[derive(Default)]
pub(super) struct TimelineSearchUpdates {
    pub(super) items: Vec<RoomTimelineItem>,
    pub(super) redacted_event_ids: Vec<String>,
    pub(super) next_token: Option<String>,
}

#[derive(Clone, Copy)]
enum TimelineChunkFilter {
    MessagesOnly,
    MessagesAndRedactions,
}

fn backward_shell_timeline_options(
    limit: u16,
    from: Option<&str>,
    chunk_filter: TimelineChunkFilter,
) -> MessagesOptions {
    let mut options = MessagesOptions::backward();
    options.limit = limit.into();
    let mut filter = RoomEventFilter::default();
    // The shell timeline only renders message-like content, and encrypted
    // room messages still arrive as `m.room.encrypted` before decryption.
    let mut event_types = vec![
        String::from("m.room.message"),
        String::from("m.room.encrypted"),
    ];
    if matches!(chunk_filter, TimelineChunkFilter::MessagesAndRedactions) {
        event_types.push(String::from("m.room.redaction"));
    }
    filter.types = Some(event_types);
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
    timeline_item_from_sync_event(event, &parsed, own_user_id)
}

fn timeline_item_from_sync_event(
    event: &matrix_sdk::deserialized_responses::TimelineEvent,
    parsed: &AnySyncTimelineEvent,
    own_user_id: &matrix_sdk::ruma::UserId,
) -> Option<RoomTimelineItem> {
    let (event_id, sender_id, sender_display_name, body, is_edited) =
        message_fields_from_sync_event(parsed)?;

    Some(RoomTimelineItem {
        event_id,
        sender_id: sender_id.clone(),
        sender_display_name,
        body,
        timestamp_unix_ms: {
            let timestamp = event.timestamp();
            timestamp.map_or(0, |timestamp| u64::from(timestamp.0))
        },
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

            let mut event_id = original.event_id.to_string();
            let mut body = message_body(&original.content.msgtype)?;
            let relation = original.content.relates_to.as_ref();
            let is_edited = relation
                .and_then(Relation::rel_type)
                .is_some_and(|relation_type| relation_type == RelationType::Replacement);

            if let Some(Relation::Replacement(replacement)) = relation {
                event_id = replacement.event_id.to_string();
                body = message_body(&replacement.new_content.msgtype)?;
            }

            Some((event_id, original.sender.to_string(), None, body, is_edited))
        }
        _ => None,
    }
}

fn message_body(message_type: &MessageType) -> Option<String> {
    let MessageType::Text(text) = message_type else {
        return None;
    };

    Some(text.body.clone())
}

fn redacted_event_id_from_sync_event(event: &AnySyncTimelineEvent) -> Option<String> {
    let AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomRedaction(redaction)) =
        event
    else {
        return None;
    };

    Some(redaction.redacts(&RedactionRules::V1)?.to_string())
}
