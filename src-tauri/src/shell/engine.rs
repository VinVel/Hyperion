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

use std::{collections::HashMap, sync::Arc, time::Duration};

use futures_util::StreamExt;
use matrix_sdk::ruma::{
    OwnedEventId, api::client::receipt::create_receipt::v3::ReceiptType,
    events::AnyMessageLikeEventContent,
};
use matrix_sdk::{Room, sleep::sleep};
use matrix_sdk_ui::timeline::{
    RoomExt, Timeline, TimelineDetails, TimelineFocus, TimelineItemKind,
};
use tauri::async_runtime::JoinHandle;
use tauri::async_runtime::Mutex as AsyncMutex;

use super::{sync::emit_shell_room_updated, types::RoomTimelineItem};

// The SDK room latest event can be updated shortly before the UI Timeline has
// consumed the same event-cache update. Wait briefly so timeline snapshots do
// not miss the event that already drives badges and room ordering.
const TIMELINE_LATEST_EVENT_WAIT_ATTEMPTS: usize = 8;
// Keep each wait short; this is only a consistency bridge for event propagation
// inside matrix-sdk-ui, not a network retry loop.
const TIMELINE_LATEST_EVENT_WAIT_STEP_MS: u64 = 50;

#[derive(Clone, Default)]
pub struct ShellTimelineRegistry {
    // Timeline instances are expensive live views with their own background
    // tasks, so cache them per active account+room instead of rebuilding them
    // on every command call during the first migration phase.
    live_timelines: Arc<AsyncMutex<HashMap<String, Arc<Timeline>>>>,
    // Focused event timelines are cached separately because they keep their own
    // pagination cursor around a specific anchor event instead of following the
    // room's normal live edge.
    focused_timelines: Arc<AsyncMutex<HashMap<String, Arc<Timeline>>>>,
    // Timeline subscriptions are the live bridge from matrix-sdk-ui into the
    // Tauri shell event stream; snapshots alone do not wake the frontend.
    live_timeline_update_handles: Arc<AsyncMutex<HashMap<String, JoinHandle<()>>>>,
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

    pub async fn subscribe_live_timeline_updates(
        &self,
        app: tauri::AppHandle,
        account_key: &str,
        room: &Room,
    ) -> Result<(), String> {
        let cache_key = Self::cache_key(account_key, room.room_id().as_str());
        {
            let handles = self.live_timeline_update_handles.lock().await;
            if handles.contains_key(&cache_key) {
                return Ok(());
            }
        }

        let timeline = self.live_timeline(account_key, room).await?;
        let (_, mut timeline_stream) = timeline.subscribe().await;
        let account_key = account_key.to_owned();
        let room_id = room.room_id().to_string();
        let handle = tauri::async_runtime::spawn(async move {
            while let Some(diffs) = timeline_stream.next().await {
                if !diffs.is_empty() {
                    emit_shell_room_updated(&app, &account_key, &room_id, false);
                }
            }
        });

        let mut handles = self.live_timeline_update_handles.lock().await;
        handles.entry(cache_key).or_insert(handle);
        Ok(())
    }

    pub async fn ensure_live_timeline_window(
        &self,
        account_key: &str,
        room: &Room,
        visible_limit: u16,
        fetch_limit: u16,
    ) -> Result<(Vec<RoomTimelineItem>, bool), String> {
        let timeline = self.live_timeline(account_key, room).await?;
        let mut items = timeline.items().await;

        if items.len() < usize::from(visible_limit) {
            let _ = timeline
                .paginate_backwards(fetch_limit)
                .await
                .map_err(|error| format!("Failed to bootstrap the live room timeline: {error}"))?;
        }

        Self::wait_for_timeline_to_reach_room_latest(room, &timeline).await;
        items = timeline.items().await;

        let shell_items = items
            .iter()
            .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
            .collect::<Vec<_>>();
        let len = shell_items.len();
        let visible_limit = usize::from(visible_limit);
        let start_index = len.saturating_sub(visible_limit);

        Ok((shell_items[start_index..].to_vec(), start_index == 0))
    }

    async fn wait_for_timeline_to_reach_room_latest(room: &Room, timeline: &Timeline) {
        let Some(latest_room_event_id) = room.latest_event().and_then(|event| event.event_id())
        else {
            return;
        };

        for _ in 0..TIMELINE_LATEST_EVENT_WAIT_ATTEMPTS {
            if timeline
                .latest_event_id()
                .await
                .is_some_and(|event_id| event_id == latest_room_event_id)
            {
                return;
            }

            sleep(Duration::from_millis(TIMELINE_LATEST_EVENT_WAIT_STEP_MS)).await;
        }
    }

    pub async fn paginate_live_timeline_backwards(
        &self,
        account_key: &str,
        room: &Room,
        limit: u16,
    ) -> Result<(Vec<RoomTimelineItem>, bool), String> {
        let timeline = self.live_timeline(account_key, room).await?;
        let before_items = timeline.items().await;
        let seen_item_ids = before_items
            .iter()
            .filter_map(|item| item.as_event().map(|event| event.identifier().to_string()))
            .collect::<std::collections::HashSet<_>>();

        let hit_start = timeline
            .paginate_backwards(limit)
            .await
            .map_err(|error| format!("Failed to paginate the live room timeline: {error}"))?;

        let after_items = timeline.items().await;
        let new_items = after_items
            .iter()
            .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
            .filter(|item| !seen_item_ids.contains(item.event_id.as_str()))
            .collect();

        Ok((new_items, hit_start))
    }

    pub async fn focused_timeline(
        &self,
        account_key: &str,
        room: &Room,
        event_id: OwnedEventId,
        context_limit: u16,
    ) -> Result<Arc<Timeline>, String> {
        let cache_key = Self::focused_cache_key(account_key, room.room_id().as_str(), &event_id);

        {
            let timelines = self.focused_timelines.lock().await;
            if let Some(timeline) = timelines.get(&cache_key) {
                return Ok(timeline.clone());
            }
        }

        let timeline = room
            .timeline_builder()
            .with_focus(TimelineFocus::Event {
                target: event_id.clone(),
                num_context_events: context_limit,
                hide_threaded_events: false,
            })
            .build()
            .await
            .map(Arc::new)
            .map_err(|error| format!("Failed to build the focused room timeline: {error}"))?;

        let mut timelines = self.focused_timelines.lock().await;
        Ok(timelines
            .entry(cache_key)
            .or_insert_with(|| timeline.clone())
            .clone())
    }

    pub async fn focused_timeline_items(
        &self,
        account_key: &str,
        room: &Room,
        event_id: OwnedEventId,
        context_limit: u16,
    ) -> Result<Vec<RoomTimelineItem>, String> {
        let timeline = self
            .focused_timeline(account_key, room, event_id, context_limit)
            .await?;
        let items = timeline.items().await;

        Ok(items
            .iter()
            .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
            .collect())
    }

    pub async fn paginate_focused_timeline_backwards(
        &self,
        account_key: &str,
        room: &Room,
        event_id: OwnedEventId,
        context_limit: u16,
        limit: u16,
    ) -> Result<(Vec<RoomTimelineItem>, bool), String> {
        let timeline = self
            .focused_timeline(account_key, room, event_id, context_limit)
            .await?;
        let before_items = timeline.items().await;
        let seen_item_ids = before_items
            .iter()
            .filter_map(|item| item.as_event().map(|event| event.identifier().to_string()))
            .collect::<std::collections::HashSet<_>>();

        let hit_start = timeline
            .paginate_backwards(limit)
            .await
            .map_err(|error| format!("Failed to paginate the focused room timeline: {error}"))?;

        let after_items = timeline.items().await;
        let new_items = after_items
            .iter()
            .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
            .filter(|item| !seen_item_ids.contains(item.event_id.as_str()))
            .collect();

        Ok((new_items, hit_start))
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
        let latest_item = items.last().ok_or_else(|| {
            String::from("The timeline send succeeded, but no local echo is available")
        })?;

        Ok(latest_item.event_id.clone())
    }

    fn cache_key(account_key: &str, room_id: &str) -> String {
        format!("{account_key}::{room_id}")
    }

    fn focused_cache_key(account_key: &str, room_id: &str, event_id: &OwnedEventId) -> String {
        format!("{account_key}::{room_id}::{event_id}")
    }

    pub async fn clear_account(&self, account_key: &str) {
        let account_prefix = format!("{account_key}::");

        self.live_timelines
            .lock()
            .await
            .retain(|cache_key, _| !cache_key.starts_with(&account_prefix));
        self.focused_timelines
            .lock()
            .await
            .retain(|cache_key, _| !cache_key.starts_with(&account_prefix));

        let removed_handles = {
            let mut handles = self.live_timeline_update_handles.lock().await;
            let removed_keys = handles
                .keys()
                .filter(|cache_key| cache_key.starts_with(&account_prefix))
                .cloned()
                .collect::<Vec<_>>();

            removed_keys
                .into_iter()
                .filter_map(|cache_key| handles.remove(&cache_key))
                .collect::<Vec<_>>()
        };

        for handle in removed_handles {
            handle.abort();
            let _ = handle.await;
        }
    }

    pub async fn clear_all(&self) {
        self.live_timelines.lock().await.clear();
        self.focused_timelines.lock().await.clear();

        let removed_handles = {
            let mut handles = self.live_timeline_update_handles.lock().await;
            handles
                .drain()
                .map(|(_, handle)| handle)
                .collect::<Vec<_>>()
        };

        for handle in removed_handles {
            handle.abort();
            let _ = handle.await;
        }
    }
}

fn timeline_item_to_shell_item(
    item: &matrix_sdk_ui::timeline::TimelineItem,
) -> Option<RoomTimelineItem> {
    let TimelineItemKind::Event(event) = item.kind() else {
        return None;
    };

    let content = event.content();
    let (body, is_edited) = if let Some(message) = content.as_message() {
        (message.body().to_owned(), message.is_edited())
    } else if content.is_unable_to_decrypt() {
        (String::from("Unable to decrypt this message"), false)
    } else {
        return None;
    };
    let event_id = event
        .event_id()
        .map(ToString::to_string)
        .unwrap_or_else(|| event.identifier().to_string());

    Some(RoomTimelineItem {
        event_id,
        sender_id: event.sender().to_string(),
        sender_display_name: sender_display_name(event.sender_profile()),
        body,
        timestamp_unix_ms: u64::from(event.timestamp().0),
        is_edited: Some(is_edited),
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
