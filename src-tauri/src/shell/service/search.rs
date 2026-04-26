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
    collections::{HashMap, HashSet},
    time::{SystemTime, UNIX_EPOCH},
};

use matrix_sdk::{
    Room,
    ruma::events::{AnyMessageLikeEvent, AnyTimelineEvent},
};

use super::super::types::{GlobalSearchMessageHit, RoomTimelineItem};

pub(super) fn message_search_hit(
    room: &Room,
    room_title: &str,
    item: RoomTimelineItem,
) -> GlobalSearchMessageHit {
    GlobalSearchMessageHit {
        result_id: format!("{}::{}", room.room_id(), item.event_id),
        room_id: room.room_id().to_string(),
        title: format!("Recent message in {room_title}"),
        description: item.body,
        event_id: Some(item.event_id),
    }
}

pub(super) fn server_backed_search_hit(
    raw_event: &matrix_sdk::ruma::serde::Raw<AnyTimelineEvent>,
    room_titles: &HashMap<String, String>,
    query: &str,
) -> Option<GlobalSearchMessageHit> {
    let (event_id, room_id, _sender_id, body) = message_fields_from_timeline_event(raw_event)?;
    if !body.to_lowercase().contains(query) {
        return None;
    }

    let title = room_titles
        .get(&room_id)
        .cloned()
        .unwrap_or_else(|| room_id.clone());

    Some(GlobalSearchMessageHit {
        result_id: format!("{room_id}::{event_id}"),
        room_id,
        title: format!("Message in {title}"),
        description: body,
        event_id: Some(event_id),
    })
}

pub(super) fn push_message_hits(
    target: &mut Vec<GlobalSearchMessageHit>,
    seen_message_ids: &mut HashSet<String>,
    hits: Vec<GlobalSearchMessageHit>,
    limit: usize,
) {
    for hit in hits {
        if seen_message_ids.insert(hit.result_id.clone()) {
            target.push(hit);
        }

        if target.len() >= limit {
            break;
        }
    }
}

pub(super) fn normalize_query(query: Option<&str>) -> Option<String> {
    query
        .map(str::trim)
        .filter(|query| !query.is_empty())
        .map(str::to_lowercase)
}

pub(super) fn matches_query(query: Option<&str>, haystacks: &[&str]) -> bool {
    let Some(query) = query else {
        return true;
    };

    haystacks
        .iter()
        .any(|haystack| haystack.to_lowercase().contains(query))
}

pub(super) fn first_visible_grapheme(value: &str) -> Option<String> {
    value
        .trim()
        .chars()
        .next()
        .map(|ch| ch.to_uppercase().collect())
}

// Shell timestamps only need a coarse "now" anchor for relative labels and
// warmup throttling, so a simple system-clock read is sufficient here.
pub(super) fn now_unix_ms() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

pub(super) fn relative_time_label(timestamp_unix_ms: u64) -> String {
    if timestamp_unix_ms == 0 {
        return String::from("No recent activity");
    }

    let now = now_unix_ms();
    let delta_ms = now.saturating_sub(timestamp_unix_ms);
    let delta_minutes = delta_ms / 60_000;
    let delta_hours = delta_ms / 3_600_000;
    let delta_days = delta_ms / 86_400_000;

    if delta_minutes < 1 {
        String::from("Just now")
    } else if delta_minutes < 60 {
        format!(
            "{delta_minutes} minute{} ago",
            if delta_minutes == 1 { "" } else { "s" }
        )
    } else if delta_hours < 24 {
        format!(
            "{delta_hours} hour{} ago",
            if delta_hours == 1 { "" } else { "s" }
        )
    } else if delta_days == 1 {
        String::from("Yesterday")
    } else {
        format!("{delta_days} days ago")
    }
}

fn message_fields_from_timeline_event(
    raw_event: &matrix_sdk::ruma::serde::Raw<AnyTimelineEvent>,
) -> Option<(String, String, String, String)> {
    let parsed = raw_event.deserialize().ok()?;
    match parsed {
        AnyTimelineEvent::MessageLike(AnyMessageLikeEvent::RoomMessage(message)) => {
            let value = raw_event.deserialize_as::<serde_json::Value>().ok()?;
            let body = value.pointer("/content/body")?.as_str()?.to_owned();

            Some((
                message.event_id().to_string(),
                message.room_id().to_string(),
                message.sender().to_string(),
                body,
            ))
        }
        _ => None,
    }
}
