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

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ListRoomThreadsRequest {
    pub search_query: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomThreadSummary {
    pub room_id: String,
    pub title: String,
    pub preview: String,
    pub participant_label: String,
    pub last_activity_unix_ms: u64,
    pub last_activity_label: String,
    pub message_count: u64,
    pub unread_count: u64,
    pub homeserver_label: String,
    pub avatar_label: Option<String>,
    pub is_direct: bool,
}

#[derive(Debug, Deserialize)]
pub struct GetRoomSummaryRequest {
    pub room_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomSummary {
    pub room_id: String,
    pub title: String,
    pub participant_label: String,
    pub homeserver_label: String,
    pub topic: Option<String>,
    pub is_direct: bool,
    pub can_send_messages: bool,
}

#[derive(Debug, Deserialize)]
pub struct GetRoomTimelineRequest {
    pub room_id: String,
    pub before: Option<String>,
    pub limit: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub struct GetRoomEventContextRequest {
    pub room_id: String,
    pub event_id: String,
    pub context_limit: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomTimelineItem {
    pub event_id: String,
    pub sender_id: String,
    pub sender_display_name: Option<String>,
    pub body: String,
    pub timestamp_unix_ms: u64,
    pub is_edited: Option<bool>,
    pub is_own_message: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomTimeline {
    pub room_id: String,
    pub items: Vec<RoomTimelineItem>,
    pub next_before: Option<String>,
    pub focused_event_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SendRoomMessageRequest {
    pub room_id: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendRoomMessageResponse {
    pub event_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ListSpacesRequest {
    pub search_query: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpaceSummary {
    pub space_id: String,
    pub name: String,
    pub description: String,
    pub member_label: String,
    pub activity_label: String,
    pub accent_label: Option<String>,
    pub is_official: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct GlobalSearchRequest {
    pub query: String,
    pub limit_per_group: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSearchRoomHit {
    pub room_id: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSearchSpaceHit {
    pub space_id: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSearchMessageHit {
    pub result_id: String,
    pub room_id: String,
    pub title: String,
    pub description: String,
    pub event_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSearchResponse {
    pub rooms: Vec<GlobalSearchRoomHit>,
    pub spaces: Vec<GlobalSearchSpaceHit>,
    pub messages: Vec<GlobalSearchMessageHit>,
}
