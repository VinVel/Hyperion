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

// Timeline grouping follows the product direction for Discord-style streams:
// same sender, close in time, and uninterrupted by another sender.
const TIMELINE_GROUP_WINDOW_UNIX_MS: u64 = 15 * 60 * 1_000;

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

#[derive(Debug, Clone, Deserialize)]
pub struct GetRoomTimelineRequest {
    pub room_id: String,
    pub before: Option<String>,
    pub limit: Option<u16>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PaginateRoomTimelineRequest {
    pub room_id: String,
    pub before: Option<String>,
    pub limit: Option<u16>,
    pub request_id: String,
    #[serde(default)]
    pub known_event_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct GetRoomEventContextRequest {
    pub room_id: String,
    pub event_id: String,
    pub context_limit: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub struct ResolveRoomReplyPreviewRequest {
    pub room_id: String,
    pub event_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomTimelineItem {
    pub matrix: RoomTimelineMatrixData,
    pub presentation: RoomTimelinePresentationData,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomTimelineMatrixData {
    pub event_id: String,
    pub transaction_id: Option<String>,
    pub sender_id: String,
    pub room_id: Option<String>,
    pub timestamp_unix_ms: u64,
    pub is_own_message: bool,
    pub content: RoomTimelineEventContent,
    pub send_state: RoomTimelineSendState,
    pub decryption_state: RoomTimelineDecryptionState,
    pub reactions: Vec<RoomTimelineReaction>,
    pub receipts: Vec<RoomTimelineReceipt>,
    pub thread: Option<RoomTimelineThreadRelation>,
    pub attachments: Vec<RoomTimelineAttachment>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomTimelineEventContent {
    pub kind: RoomTimelineEventContentKind,
    pub body: String,
    pub formatted_body: Option<String>,
    pub is_edited: bool,
    pub is_redacted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(
    dead_code,
    reason = "Reservation of non-text event kinds before their mappers are populated."
)]
pub enum RoomTimelineEventContentKind {
    Text,
    UnableToDecrypt,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(
    dead_code,
    reason = "Local echo reconciliation will construct these states when send retries land."
)]
pub enum RoomTimelineSendState {
    Pending,
    Sending,
    Sent,
    Failed,
    Retrying,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(
    dead_code,
    reason = "Encrypted timeline updates will construct these states once delayed decryption is mapped."
)]
pub enum RoomTimelineDecryptionState {
    Unencrypted,
    Decrypted,
    UnableToDecrypt,
    Pending,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomTimelineReaction {
    pub key: String,
    pub count: u64,
    pub reacted_by_me: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomTimelineReceipt {
    pub user_id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub timestamp_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomTimelineThreadRelation {
    pub root_event_id: String,
    pub latest_event_id: Option<String>,
    pub reply_count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomTimelineAttachment {
    pub event_id: String,
    pub media_type: RoomTimelineAttachmentType,
    pub media_handle: String,
    pub thumbnail_handle: Option<String>,
    pub filename: Option<String>,
    pub display_caption: Option<String>,
    pub mime_type: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_unix_ms: Option<u64>,
    pub size_bytes: Option<u64>,
    pub blurhash: Option<String>,
    pub requires_reveal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(
    dead_code,
    reason = "Media milestones will populate attachment variants through Matrix media mappers."
)]
pub enum RoomTimelineAttachmentType {
    Image,
    Video,
    Audio,
    File,
    Sticker,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomTimelinePresentationData {
    pub sender_display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub group_position: RoomTimelineGroupPosition,
    pub reply_preview: Option<RoomTimelineReplyPreview>,
    pub permalink: Option<String>,
    pub capabilities: RoomTimelineItemCapabilities,
    pub compact_receipts: Vec<RoomTimelineReceipt>,
    pub thumbnail: Option<RoomTimelineThumbnailState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomTimelineGroupPosition {
    Standalone,
    Start,
    Middle,
    End,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomTimelineReplyPreview {
    pub event_id: String,
    pub state: RoomTimelineReplyPreviewState,
    pub sender_id: Option<String>,
    pub sender_display_name: Option<String>,
    pub body: Option<String>,
    pub is_redacted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomTimelineReplyPreviewState {
    Resolved,
    Loading,
    DeletedRedacted,
    Inaccessible,
    FailedToLoad,
    InvalidRelation,
}

#[derive(Debug, Clone, Serialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Timeline item capabilities are a compact frontend wire shape, not internal state."
)]
pub struct RoomTimelineItemCapabilities {
    pub can_edit: bool,
    pub can_redact: bool,
    pub can_reply: bool,
    pub can_react: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomTimelineThumbnailState {
    pub cache_key: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub blurhash: Option<String>,
}

impl RoomTimelineItem {
    pub fn text_message(
        event_id: String,
        sender_id: String,
        sender_display_name: Option<String>,
        body: String,
        timestamp_unix_ms: u64,
        is_edited: bool,
        is_own_message: bool,
    ) -> Self {
        Self {
            matrix: RoomTimelineMatrixData {
                event_id,
                transaction_id: None,
                sender_id,
                room_id: None,
                timestamp_unix_ms,
                is_own_message,
                content: RoomTimelineEventContent {
                    kind: RoomTimelineEventContentKind::Text,
                    body,
                    formatted_body: None,
                    is_edited,
                    is_redacted: false,
                },
                send_state: RoomTimelineSendState::Sent,
                decryption_state: RoomTimelineDecryptionState::Unencrypted,
                reactions: Vec::new(),
                receipts: Vec::new(),
                thread: None,
                attachments: Vec::new(),
            },
            presentation: RoomTimelinePresentationData {
                sender_display_name,
                avatar_url: None,
                group_position: RoomTimelineGroupPosition::Standalone,
                reply_preview: None,
                permalink: None,
                capabilities: RoomTimelineItemCapabilities {
                    can_edit: is_own_message,
                    can_redact: is_own_message,
                    can_reply: true,
                    can_react: true,
                },
                compact_receipts: Vec::new(),
                thumbnail: None,
            },
        }
    }

    #[allow(
        dead_code,
        reason = "Delayed decryption mapping will use this constructor after the DTO migration."
    )]
    pub fn unable_to_decrypt(
        event_id: String,
        sender_id: String,
        sender_display_name: Option<String>,
        timestamp_unix_ms: u64,
        is_own_message: bool,
    ) -> Self {
        let mut item = Self::text_message(
            event_id,
            sender_id,
            sender_display_name,
            String::from("Unable to decrypt this message"),
            timestamp_unix_ms,
            false,
            is_own_message,
        );
        item.matrix.content.kind = RoomTimelineEventContentKind::UnableToDecrypt;
        item.matrix.decryption_state = RoomTimelineDecryptionState::UnableToDecrypt;
        item
    }

    pub fn event_id(&self) -> &str {
        &self.matrix.event_id
    }

    pub fn sender_id(&self) -> &str {
        &self.matrix.sender_id
    }

    pub fn sender_display_name(&self) -> Option<&str> {
        self.presentation.sender_display_name.as_deref()
    }

    pub fn body(&self) -> &str {
        &self.matrix.content.body
    }

    pub fn timestamp_unix_ms(&self) -> u64 {
        self.matrix.timestamp_unix_ms
    }

    pub fn is_edited(&self) -> bool {
        self.matrix.content.is_edited
    }

    pub fn is_own_message(&self) -> bool {
        self.matrix.is_own_message
    }

    #[cfg(test)]
    pub fn set_body(&mut self, body: String) {
        self.matrix.content.body = body;
    }

    #[cfg(test)]
    pub fn set_edited(&mut self, is_edited: bool) {
        self.matrix.content.is_edited = is_edited;
    }

    pub fn set_group_position(&mut self, group_position: RoomTimelineGroupPosition) {
        self.presentation.group_position = group_position;
    }

    fn set_room_context(&mut self, room_id: &str) {
        self.matrix.room_id = Some(room_id.to_owned());
        self.presentation.permalink = Some(format!(
            "https://matrix.to/#/{}/{}",
            matrix_to_path_segment(room_id),
            matrix_to_path_segment(self.event_id()),
        ));
    }
}

pub fn apply_timeline_presentation(items: &mut [RoomTimelineItem], room_id: &str) {
    for item in items.iter_mut() {
        item.set_room_context(room_id);
    }
    apply_timeline_group_positions(items);
}

fn apply_timeline_group_positions(items: &mut [RoomTimelineItem]) {
    let group_positions = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let previous_is_grouped = index
                .checked_sub(1)
                .and_then(|previous_index| items.get(previous_index))
                .is_some_and(|previous_item| timeline_items_can_group(previous_item, item));
            let next_is_grouped = items
                .get(index + 1)
                .is_some_and(|next_item| timeline_items_can_group(item, next_item));

            match (previous_is_grouped, next_is_grouped) {
                (false, false) => RoomTimelineGroupPosition::Standalone,
                (false, true) => RoomTimelineGroupPosition::Start,
                (true, true) => RoomTimelineGroupPosition::Middle,
                (true, false) => RoomTimelineGroupPosition::End,
            }
        })
        .collect::<Vec<RoomTimelineGroupPosition>>();

    for (item, group_position) in items.iter_mut().zip(group_positions) {
        item.set_group_position(group_position);
    }
}

fn timeline_items_can_group(previous: &RoomTimelineItem, current: &RoomTimelineItem) -> bool {
    if previous.sender_id() != current.sender_id() {
        return false;
    }

    let elapsed_ms = current
        .timestamp_unix_ms()
        .saturating_sub(previous.timestamp_unix_ms());
    elapsed_ms <= TIMELINE_GROUP_WINDOW_UNIX_MS
}

fn matrix_to_path_segment(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| {
            let character = byte as char;
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '~') {
                vec![character.to_string()]
            } else {
                vec![format!("%{byte:02X}")]
            }
        })
        .collect::<String>()
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomTimeline {
    pub room_id: String,
    pub items: Vec<RoomTimelineItem>,
    pub next_before: Option<String>,
    pub focused_event_id: Option<String>,
    pub redacted_event_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomTimelinePaginationResponse {
    pub room_id: String,
    pub items: Vec<RoomTimelineItem>,
    pub next_before: Option<String>,
    pub request_id: String,
    pub had_new_items: bool,
    pub returned_item_count: usize,
    pub new_item_count: usize,
    pub duplicate_item_count: usize,
    pub continuation_attempt_count: usize,
    pub token_changed: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SendRoomMessageRequest {
    pub room_id: String,
    pub body: String,
    #[serde(default)]
    pub formatted_body: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EditRoomMessageRequest {
    pub room_id: String,
    pub event_id: String,
    pub body: String,
    #[serde(default)]
    pub formatted_body: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RedactRoomMessageRequest {
    pub room_id: String,
    pub event_id: String,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReplyToRoomMessageRequest {
    pub room_id: String,
    pub event_id: String,
    pub body: String,
    #[serde(default)]
    pub formatted_body: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ToggleRoomReactionRequest {
    pub room_id: String,
    pub event_id: String,
    pub reaction_key: String,
}

#[derive(Debug, Deserialize)]
pub struct SetRoomTypingRequest {
    pub room_id: String,
    pub is_typing: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendRoomMessageResponse {
    pub event_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToggleRoomReactionResponse {
    pub added: bool,
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

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GlobalSearchIndexState {
    Idle,
    Indexing,
    Paused,
    Degraded,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSearchIndexStatus {
    pub state: GlobalSearchIndexState,
    pub indexed_room_count: u64,
    pub total_room_count: u64,
    pub message_count: u64,
    pub last_indexed_at_unix_ms: Option<u64>,
    pub notice: Option<String>,
}

impl Default for GlobalSearchIndexStatus {
    fn default() -> Self {
        Self {
            state: GlobalSearchIndexState::Idle,
            indexed_room_count: 0,
            total_room_count: 0,
            message_count: 0,
            last_indexed_at_unix_ms: None,
            notice: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSearchResponse {
    pub rooms: Vec<GlobalSearchRoomHit>,
    pub spaces: Vec<GlobalSearchSpaceHit>,
    pub messages: Vec<GlobalSearchMessageHit>,
    pub status: GlobalSearchIndexStatus,
}
