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

use std::{collections::HashMap, sync::Arc};

use matrix_sdk::Room;
use matrix_sdk::ruma::{
    api::client::receipt::create_receipt::v3::ReceiptType, events::AnyMessageLikeEventContent,
};
use matrix_sdk_ui::timeline::{RoomExt, Timeline, TimelineDetails, TimelineItemKind};
use tauri::async_runtime::Mutex as AsyncMutex;

use super::types::RoomTimelineItem;

#[derive(Clone, Default)]
pub struct ShellTimelineRegistry {
    // Timeline instances are expensive live views with their own background
    // tasks, so cache them per active account+room instead of rebuilding them
    // on every command call during the first migration phase.
    live_timelines: Arc<AsyncMutex<HashMap<String, Arc<Timeline>>>>,
}

impl ShellTimelineRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn live_timeline(
        &self,
        account_key: &str,
        room: &Room,
    ) -> Result<Arc<Timeline>, String> {
        let cache_key = Self::cache_key(account_key, room.room_id().as_str());

        {
            let timelines = self.live_timelines.lock().await;
            if let Some(timeline) = timelines.get(&cache_key) {
                return Ok(timeline.clone());
            }
        }

        let timeline = room
            .timeline()
            .await
            .map(Arc::new)
            .map_err(|error| format!("Failed to build the room timeline: {error}"))?;

        let mut timelines = self.live_timelines.lock().await;
        Ok(timelines
            .entry(cache_key)
            .or_insert_with(|| timeline.clone())
            .clone())
    }

    pub async fn live_timeline_items(
        &self,
        account_key: &str,
        room: &Room,
        limit: u16,
    ) -> Result<Vec<RoomTimelineItem>, String> {
        let timeline = self.live_timeline(account_key, room).await?;
        let items = timeline.items().await;

        Ok(items
            .iter()
            .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
            .rev()
            .take(usize::from(limit))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect())
    }

    pub async fn mark_live_timeline_as_read(
        &self,
        account_key: &str,
        room: &Room,
    ) -> Result<(), String> {
        let timeline = self.live_timeline(account_key, room).await?;
        timeline
            .mark_as_read(ReceiptType::ReadPrivate)
            .await
            .map_err(|error| format!("Failed to mark the live room timeline as read: {error}"))?;
        Ok(())
    }

    pub async fn send_live_message(
        &self,
        account_key: &str,
        room: &Room,
        content: AnyMessageLikeEventContent,
    ) -> Result<String, String> {
        let timeline = self.live_timeline(account_key, room).await?;
        timeline
            .send(content)
            .await
            .map_err(|error| format!("Failed to send the room message: {error}"))?;

        let items = self.live_timeline_items(account_key, room, 1).await?;
        let latest_item = items
            .last()
            .ok_or_else(|| String::from("The timeline send succeeded, but no local echo is available"))?;

        Ok(latest_item.event_id.clone())
    }

    fn cache_key(account_key: &str, room_id: &str) -> String {
        format!("{account_key}::{room_id}")
    }
}

fn timeline_item_to_shell_item(
    item: &matrix_sdk_ui::timeline::TimelineItem,
) -> Option<RoomTimelineItem> {
    let TimelineItemKind::Event(event) = item.kind() else {
        return None;
    };

    let message = event.content().as_message()?;
    let event_id = event
        .event_id()
        .map(ToString::to_string)
        .unwrap_or_else(|| event.identifier().to_string());

    Some(RoomTimelineItem {
        event_id,
        sender_id: event.sender().to_string(),
        sender_display_name: sender_display_name(event.sender_profile()),
        body: message.body().to_owned(),
        timestamp_unix_ms: u64::from(event.timestamp().0),
        is_edited: Some(message.is_edited()),
        is_own_message: event.is_own(),
    })
}

fn sender_display_name(
    profile: &TimelineDetails<matrix_sdk_ui::timeline::Profile>,
) -> Option<String> {
    match profile {
        TimelineDetails::Ready(profile) => profile.display_name.clone(),
        TimelineDetails::Unavailable | TimelineDetails::Pending | TimelineDetails::Error(_) => None,
    }
}

trait TimelineItemIdentifierExt {
    fn to_string(&self) -> String;
}

impl TimelineItemIdentifierExt for matrix_sdk_ui::timeline::TimelineEventItemId {
    fn to_string(&self) -> String {
        match self {
            matrix_sdk_ui::timeline::TimelineEventItemId::TransactionId(transaction_id) => {
                transaction_id.to_string()
            }
            matrix_sdk_ui::timeline::TimelineEventItemId::EventId(event_id) => event_id.to_string(),
        }
    }
}
