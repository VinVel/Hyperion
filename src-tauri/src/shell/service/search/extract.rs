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

use super::types::{SearchDocument, SearchEntityType};
use crate::{
    shell::types::{RoomThreadSummary, RoomTimelineItem, SpaceSummary},
    utils::time::now_unix_ms,
};

pub(super) fn room_document(account_key: &str, summary: &RoomThreadSummary) -> SearchDocument {
    let updated_at_unix_ms = now_unix_ms();
    SearchDocument {
        account_key: account_key.to_owned(),
        document_id: room_document_id(account_key, &summary.room_id),
        entity_type: SearchEntityType::Room,
        room_id: Some(summary.room_id.clone()),
        space_id: None,
        event_id: None,
        sender_id: None,
        user_id: None,
        title: summary.title.clone(),
        subtitle: summary.participant_label.clone(),
        body: [
            summary.participant_label.as_str(),
            summary.homeserver_label.as_str(),
            summary.room_id.as_str(),
        ]
        .join(" "),
        timestamp_unix_ms: summary.last_activity_unix_ms,
        sort_timestamp_unix_ms: summary.last_activity_unix_ms,
        is_deleted: false,
        updated_at_unix_ms,
    }
}

pub(super) fn space_document(account_key: &str, summary: &SpaceSummary) -> SearchDocument {
    let updated_at_unix_ms = now_unix_ms();
    SearchDocument {
        account_key: account_key.to_owned(),
        document_id: space_document_id(account_key, &summary.space_id),
        entity_type: SearchEntityType::Space,
        room_id: None,
        space_id: Some(summary.space_id.clone()),
        event_id: None,
        sender_id: None,
        user_id: None,
        title: summary.name.clone(),
        subtitle: summary.member_label.clone(),
        body: [
            summary.description.as_str(),
            summary.member_label.as_str(),
            summary.space_id.as_str(),
        ]
        .join(" "),
        timestamp_unix_ms: updated_at_unix_ms,
        sort_timestamp_unix_ms: updated_at_unix_ms,
        is_deleted: false,
        updated_at_unix_ms,
    }
}

pub(super) fn message_document(
    account_key: &str,
    room_id: &str,
    room_title: &str,
    item: &RoomTimelineItem,
) -> Option<SearchDocument> {
    if item.body.trim().is_empty() || item.body == "Unable to decrypt this message" {
        return None;
    }

    let updated_at_unix_ms = now_unix_ms();
    Some(SearchDocument {
        account_key: account_key.to_owned(),
        document_id: message_document_id(account_key, room_id, &item.event_id),
        entity_type: SearchEntityType::Message,
        room_id: Some(room_id.to_owned()),
        space_id: None,
        event_id: Some(item.event_id.clone()),
        sender_id: Some(item.sender_id.clone()),
        user_id: None,
        title: format!("Message in {room_title}"),
        subtitle: item
            .sender_display_name
            .clone()
            .unwrap_or_else(|| item.sender_id.clone()),
        body: item.body.clone(),
        timestamp_unix_ms: item.timestamp_unix_ms,
        sort_timestamp_unix_ms: item.timestamp_unix_ms,
        is_deleted: false,
        updated_at_unix_ms,
    })
}

pub(super) fn room_document_id(account_key: &str, room_id: &str) -> String {
    format!("room::{account_key}::{room_id}")
}

pub(super) fn space_document_id(account_key: &str, space_id: &str) -> String {
    format!("space::{account_key}::{space_id}")
}

pub(super) fn message_document_id(account_key: &str, room_id: &str, event_id: &str) -> String {
    format!("message::{account_key}::{room_id}::{event_id}")
}
