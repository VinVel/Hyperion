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

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use matrix_sdk::{Room, ruma::EventId};

use super::{
    super::{engine::ShellTimelineRegistry, types::RoomTimelineItem},
    DEFAULT_EVENT_CONTEXT_LIMIT, RECENT_TIMELINE_WARM_LIMIT,
};

// Timeline-backed pagination uses backend-owned opaque tokens because
// matrix-sdk-ui tracks pagination state inside the live timeline instance rather
// than exposing Matrix prev-batch tokens directly.
const TIMELINE_UI_PAGE_TOKEN_PREFIX: &str = "timeline-ui-page:";
// Focused timelines need to carry their anchor event inside the opaque token so
// later pagination requests can reopen the same TimelineFocus::Event view.
const TIMELINE_UI_EVENT_PAGE_TOKEN_PREFIX: &str = "timeline-ui-event:";

pub(super) async fn load_live_room_timeline(
    timeline_registry: &ShellTimelineRegistry,
    account_key: &str,
    room: &Room,
    limit: u16,
) -> Result<(Vec<RoomTimelineItem>, Option<String>), String> {
    let (items, hit_start) = timeline_registry
        .ensure_live_timeline_window(
            account_key,
            room,
            limit,
            limit.max(RECENT_TIMELINE_WARM_LIMIT),
        )
        .await?;

    let next_before = if hit_start {
        None
    } else {
        Some(timeline_page_token(1))
    };

    Ok((items, next_before))
}

pub(super) async fn load_paginated_room_timeline(
    timeline_registry: &ShellTimelineRegistry,
    account_key: &str,
    room: &Room,
    limit: u16,
    before: Option<&str>,
) -> Result<(Vec<RoomTimelineItem>, Option<String>), String> {
    if let Some((event_id, page_index)) = before.and_then(parse_focused_timeline_page_token) {
        let owned_event_id = EventId::parse(&event_id)
            .map_err(|error| format!("Invalid focused event id: {error}"))?
            .clone();
        let (items, hit_start) = timeline_registry
            .paginate_focused_timeline_backwards(
                account_key,
                room,
                owned_event_id,
                DEFAULT_EVENT_CONTEXT_LIMIT,
                limit,
            )
            .await?;

        let next_before = if hit_start {
            None
        } else {
            Some(focused_timeline_page_token(&event_id, page_index + 1))
        };

        return Ok((items, next_before));
    }

    if let Some(page_index) = before.and_then(parse_timeline_page_token) {
        let (items, hit_start) = timeline_registry
            .paginate_live_timeline_backwards(account_key, room, limit)
            .await?;

        let next_before = if hit_start {
            None
        } else {
            Some(timeline_page_token(page_index + 1))
        };

        return Ok((items, next_before));
    }

    Err(String::from("Unsupported timeline pagination token"))
}

pub(super) fn timeline_page_token(page_index: usize) -> String {
    format!("{TIMELINE_UI_PAGE_TOKEN_PREFIX}{page_index}")
}

pub(super) fn focused_timeline_page_token(event_id: &str, page_index: usize) -> String {
    let encoded_event_id = URL_SAFE_NO_PAD.encode(event_id.as_bytes());
    format!("{TIMELINE_UI_EVENT_PAGE_TOKEN_PREFIX}{encoded_event_id}:{page_index}")
}

fn parse_timeline_page_token(token: &str) -> Option<usize> {
    token
        .strip_prefix(TIMELINE_UI_PAGE_TOKEN_PREFIX)
        .and_then(|value| value.parse::<usize>().ok())
}

fn parse_focused_timeline_page_token(token: &str) -> Option<(String, usize)> {
    let token = token.strip_prefix(TIMELINE_UI_EVENT_PAGE_TOKEN_PREFIX)?;
    let (encoded_event_id, page_index) = token.rsplit_once(':')?;
    let event_id = URL_SAFE_NO_PAD.decode(encoded_event_id).ok()?;
    let event_id = String::from_utf8(event_id).ok()?;
    let page_index = page_index.parse::<usize>().ok()?;

    Some((event_id, page_index))
}
