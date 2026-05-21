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
use matrix_sdk::room::edit::EditedContent;
use matrix_sdk::ruma::{
    EventId, OwnedEventId,
    api::client::receipt::create_receipt::v3::ReceiptType,
    events::{AnyMessageLikeEventContent, room::message::RoomMessageEventContentWithoutRelation},
};
use matrix_sdk::{Room, sleep::sleep};
use matrix_sdk_ui::timeline::{
    EventSendState, ReactionStatus, Timeline, TimelineDetails, TimelineEventFocusThreadMode,
    TimelineEventItemId, TimelineFocus, TimelineItem, TimelineItemKind,
};
use tauri::async_runtime::JoinHandle;
use tauri::async_runtime::Mutex as AsyncMutex;

use super::{
    sync::{emit_shell_room_updated, emit_shell_timeline_updated, emit_shell_typing_updated},
    types::{
        RoomTimelineItem, RoomTimelineReaction, RoomTimelineReceipt, RoomTimelineReplyPreview,
        RoomTimelineReplyPreviewState, RoomTimelineSendState, apply_timeline_presentation,
    },
};

// The SDK room latest event can be updated shortly before the UI Timeline has
// consumed the same event-cache update. Wait briefly so timeline snapshots do
// not miss the event that already drives badges and room ordering.
const TIMELINE_LATEST_EVENT_WAIT_ATTEMPTS: usize = 8;
// Keep each wait short; this is only a consistency bridge for event propagation
// inside matrix-sdk-ui, not a network retry loop.
const TIMELINE_LATEST_EVENT_WAIT_STEP_MS: u64 = 50;
// Restoring a remembered room depth can require multiple SDK UI pagination
// passes because Matrix SDK may reveal cached events in smaller chunks.
const TIMELINE_RESTORE_PAGINATION_ATTEMPTS: usize = 8;
// Matrix SDK local echo and remote echo timestamps can differ slightly while
// the send queue reconciles transaction IDs, so body-based fallback matching is bounded.
const LOCAL_ECHO_RECONCILIATION_WINDOW_UNIX_MS: u64 = 120_000;

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
    // Typing notification subscriptions are room-scoped ephemeral bridges.
    typing_update_handles: Arc<AsyncMutex<HashMap<String, JoinHandle<()>>>>,
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

        // Keep SDK receipt tracking disabled for the room stream. In
        // matrix-sdk-ui 0.17 it can panic when cached/paginated rooms expose
        // duplicate receipt moves during timeline construction.
        let timeline = matrix_sdk_ui::timeline::RoomExt::timeline(room)
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

        let mut shell_items = items
            .iter()
            .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
            .rev()
            .take(usize::from(limit))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<RoomTimelineItem>>();
        canonicalize_timeline_items(&mut shell_items);
        apply_timeline_presentation(&mut shell_items, room.room_id().as_str());

        Ok(shell_items)
    }

    pub async fn live_redacted_event_ids(
        &self,
        account_key: &str,
        room: &Room,
    ) -> Result<Vec<String>, String> {
        let timeline = self.live_timeline(account_key, room).await?;
        let items = timeline.items().await;

        Ok(redacted_event_ids_from_timeline_items(items.iter()))
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
        let (timeline_initial_items, mut timeline_stream) = timeline.subscribe().await;
        drop(timeline_initial_items);
        let account_key = account_key.to_owned();
        let room_id = room.room_id().to_string();
        let update_handles = self.live_timeline_update_handles.clone();
        let cache_key_for_task = cache_key.clone();
        let handle = tauri::async_runtime::spawn(async move {
            while let Some(diffs) = timeline_stream.next().await {
                if diffs.is_empty() {
                    continue;
                }

                let items = timeline.items().await;
                let shell_items = timeline_items_to_shell_items(items.iter(), &room_id);
                let redacted_event_ids = redacted_event_ids_from_timeline_items(items.iter());
                emit_shell_timeline_updated(
                    &app,
                    &account_key,
                    &room_id,
                    shell_items,
                    redacted_event_ids,
                );
                emit_shell_room_updated(&app, &account_key, &room_id, false);
            }

            let mut handles = update_handles.lock().await;
            handles.remove(&cache_key_for_task);
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
        let had_existing_items = !items.is_empty();

        let mut hit_timeline_start = false;
        let mut shell_items = timeline_items_to_shell_items(items.iter(), room.room_id().as_str());
        let mut restore_attempts = 0;
        while shell_items.len() < usize::from(visible_limit)
            && !hit_timeline_start
            && restore_attempts < TIMELINE_RESTORE_PAGINATION_ATTEMPTS
        {
            hit_timeline_start = timeline
                .paginate_backwards(fetch_limit)
                .await
                .map_err(|error| format!("Failed to bootstrap the live room timeline: {error}"))?;
            restore_attempts += 1;
            items = timeline.items().await;
            shell_items = timeline_items_to_shell_items(items.iter(), room.room_id().as_str());
        }

        Self::wait_for_timeline_to_reach_room_latest(room, &timeline).await;
        items = timeline.items().await;
        shell_items = timeline_items_to_shell_items(items.iter(), room.room_id().as_str());
        let len = shell_items.len();
        let visible_limit = usize::from(visible_limit);
        let start_index = if had_existing_items {
            0
        } else {
            len.saturating_sub(visible_limit)
        };

        Ok((
            shell_items[start_index..].to_vec(),
            hit_timeline_start && start_index == 0,
        ))
    }

    async fn wait_for_timeline_to_reach_room_latest(room: &Room, timeline: &Timeline) {
        let latest_event = room.latest_event();
        let Some(latest_room_event_id) = latest_event.event_id() else {
            return;
        };

        for _wait_attempt in 0..TIMELINE_LATEST_EVENT_WAIT_ATTEMPTS {
            let timeline_latest_event_id = timeline.latest_event_id().await;
            if timeline_latest_event_id.is_some_and(|event_id| event_id == latest_room_event_id) {
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
        page_index: usize,
    ) -> Result<(Vec<RoomTimelineItem>, bool), String> {
        let timeline = self.live_timeline(account_key, room).await?;
        let before_items = timeline.items().await;
        let mut loaded_shell_items = before_items
            .iter()
            .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
            .collect::<Vec<_>>();
        apply_timeline_presentation(&mut loaded_shell_items, room.room_id().as_str());

        if let Some(loaded_page) =
            loaded_timeline_page(&loaded_shell_items, page_index, usize::from(limit))
        {
            return Ok((loaded_page, false));
        }

        let seen_item_ids = before_items
            .iter()
            .filter_map(|item| {
                let event = item.as_event()?;
                Some(event.identifier().to_string())
            })
            .collect::<std::collections::HashSet<_>>();

        let hit_start = timeline
            .paginate_backwards(limit)
            .await
            .map_err(|error| format!("Failed to paginate the live room timeline: {error}"))?;

        let after_items = timeline.items().await;
        let mut new_items = after_items
            .iter()
            .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
            .filter(|item| !seen_item_ids.contains(item.event_id()))
            .collect::<Vec<RoomTimelineItem>>();
        apply_timeline_presentation(&mut new_items, room.room_id().as_str());

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

        let timeline = matrix_sdk_ui::timeline::RoomExt::timeline_builder(room)
            .with_focus(TimelineFocus::Event {
                target: event_id.clone(),
                num_context_events: context_limit,
                thread_mode: TimelineEventFocusThreadMode::Automatic {
                    hide_threaded_events: false,
                },
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

        let mut shell_items = items
            .iter()
            .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
            .collect::<Vec<RoomTimelineItem>>();
        apply_timeline_presentation(&mut shell_items, room.room_id().as_str());
        Ok(shell_items)
    }

    pub async fn focused_redacted_event_ids(
        &self,
        account_key: &str,
        room: &Room,
        event_id: OwnedEventId,
        context_limit: u16,
    ) -> Result<Vec<String>, String> {
        let timeline = self
            .focused_timeline(account_key, room, event_id, context_limit)
            .await?;
        let items = timeline.items().await;

        Ok(redacted_event_ids_from_timeline_items(items.iter()))
    }

    pub async fn paginate_focused_timeline_backwards(
        &self,
        account_key: &str,
        room: &Room,
        event_id: OwnedEventId,
        context_limit: u16,
        limit: u16,
        page_index: usize,
    ) -> Result<(Vec<RoomTimelineItem>, bool), String> {
        let timeline = self
            .focused_timeline(account_key, room, event_id, context_limit)
            .await?;
        let before_items = timeline.items().await;
        let mut loaded_shell_items = before_items
            .iter()
            .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
            .collect::<Vec<_>>();
        apply_timeline_presentation(&mut loaded_shell_items, room.room_id().as_str());

        if let Some(loaded_page) =
            loaded_timeline_page(&loaded_shell_items, page_index, usize::from(limit))
        {
            return Ok((loaded_page, false));
        }

        let seen_item_ids = before_items
            .iter()
            .filter_map(|item| {
                let event = item.as_event()?;
                Some(event.identifier().to_string())
            })
            .collect::<std::collections::HashSet<_>>();

        let hit_start = timeline
            .paginate_backwards(limit)
            .await
            .map_err(|error| format!("Failed to paginate the focused room timeline: {error}"))?;

        let after_items = timeline.items().await;
        let mut new_items = after_items
            .iter()
            .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
            .filter(|item| !seen_item_ids.contains(item.event_id()))
            .collect::<Vec<RoomTimelineItem>>();
        apply_timeline_presentation(&mut new_items, room.room_id().as_str());

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

        let latest_item_id = self
            .live_timeline_items(account_key, room, 1)
            .await
            .ok()
            .and_then(|items| items.last().map(|item| item.event_id().to_owned()))
            .unwrap_or_default();

        Ok(latest_item_id)
    }

    pub async fn edit_live_message(
        &self,
        account_key: &str,
        room: &Room,
        event_id: &str,
        content: RoomMessageEventContentWithoutRelation,
    ) -> Result<(), String> {
        let timeline = self.live_timeline(account_key, room).await?;
        let item_id = timeline_item_id_from_string(event_id)?;
        timeline
            .edit(&item_id, EditedContent::RoomMessage(content))
            .await
            .map_err(|error| format!("Failed to edit the room message: {error}"))
    }

    pub async fn redact_live_message(
        &self,
        account_key: &str,
        room: &Room,
        event_id: &str,
        reason: Option<&str>,
    ) -> Result<(), String> {
        let timeline = self.live_timeline(account_key, room).await?;
        let item_id = timeline_item_id_from_string(event_id)?;
        timeline
            .redact(&item_id, reason)
            .await
            .map_err(|error| format!("Failed to redact the room message: {error}"))
    }

    pub async fn reply_to_live_message(
        &self,
        account_key: &str,
        room: &Room,
        event_id: OwnedEventId,
        content: RoomMessageEventContentWithoutRelation,
    ) -> Result<(), String> {
        let timeline = self.live_timeline(account_key, room).await?;
        timeline
            .send_reply(content, event_id)
            .await
            .map_err(|error| format!("Failed to reply to the room message: {error}"))
    }

    pub async fn toggle_live_reaction(
        &self,
        account_key: &str,
        room: &Room,
        event_id: &str,
        reaction_key: &str,
    ) -> Result<bool, String> {
        let timeline = self.live_timeline(account_key, room).await?;
        let item_id = timeline_item_id_from_string(event_id)?;
        timeline
            .toggle_reaction(&item_id, reaction_key)
            .await
            .map_err(|error| format!("Failed to toggle the room reaction: {error}"))
    }

    pub async fn send_typing_notice(&self, room: &Room, is_typing: bool) -> Result<(), String> {
        room.typing_notice(is_typing)
            .await
            .map_err(|error| format!("Failed to update typing notice: {error}"))
    }

    pub async fn subscribe_typing_updates(
        &self,
        app: tauri::AppHandle,
        account_key: &str,
        room: &Room,
    ) -> Result<(), String> {
        let cache_key = Self::cache_key(account_key, room.room_id().as_str());
        {
            let handles = self.typing_update_handles.lock().await;
            if handles.contains_key(&cache_key) {
                return Ok(());
            }
        }

        let (_drop_guard, mut subscriber) = room.subscribe_to_typing_notifications();
        let account_key = account_key.to_owned();
        let room_id = room.room_id().to_string();
        let update_handles = self.typing_update_handles.clone();
        let cache_key_for_task = cache_key.clone();
        let handle = tauri::async_runtime::spawn(async move {
            while let Ok(typing_user_ids) = subscriber.recv().await {
                let users = typing_user_ids
                    .into_iter()
                    .map(|user_id| user_id.to_string())
                    .collect::<Vec<String>>();
                emit_shell_typing_updated(&app, &account_key, &room_id, users);
            }

            let mut handles = update_handles.lock().await;
            handles.remove(&cache_key_for_task);
        });

        let mut handles = self.typing_update_handles.lock().await;
        handles.entry(cache_key).or_insert(handle);
        Ok(())
    }

    fn cache_key(account_key: &str, room_id: &str) -> String {
        format!("{account_key}::{room_id}")
    }

    fn focused_cache_key(account_key: &str, room_id: &str, event_id: &OwnedEventId) -> String {
        format!("{account_key}::{room_id}::{event_id}")
    }

    pub async fn clear_account(&self, account_key: &str) {
        let account_prefix = format!("{account_key}::");

        let mut live_timelines = self.live_timelines.lock().await;
        live_timelines.retain(|cache_key, _| !cache_key.starts_with(&account_prefix));
        drop(live_timelines);

        let mut focused_timelines = self.focused_timelines.lock().await;
        focused_timelines.retain(|cache_key, _| !cache_key.starts_with(&account_prefix));
        drop(focused_timelines);

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
            drop(handle.await);
        }

        let removed_typing_handles = {
            let mut handles = self.typing_update_handles.lock().await;
            let removed_keys = handles
                .keys()
                .filter(|cache_key| cache_key.starts_with(&account_prefix))
                .cloned()
                .collect::<Vec<String>>();

            removed_keys
                .into_iter()
                .filter_map(|cache_key| handles.remove(&cache_key))
                .collect::<Vec<JoinHandle<()>>>()
        };

        for handle in removed_typing_handles {
            handle.abort();
            drop(handle.await);
        }
    }

    pub async fn clear_all(&self) {
        let mut live_timelines = self.live_timelines.lock().await;
        live_timelines.clear();
        drop(live_timelines);

        let mut focused_timelines = self.focused_timelines.lock().await;
        focused_timelines.clear();
        drop(focused_timelines);

        let removed_handles = {
            let mut handles = self.live_timeline_update_handles.lock().await;
            handles
                .drain()
                .map(|handle_entry| handle_entry.1)
                .collect::<Vec<_>>()
        };

        for handle in removed_handles {
            handle.abort();
            drop(handle.await);
        }

        let removed_typing_handles = {
            let mut handles = self.typing_update_handles.lock().await;
            handles
                .drain()
                .map(|handle_entry| handle_entry.1)
                .collect::<Vec<JoinHandle<()>>>()
        };

        for handle in removed_typing_handles {
            handle.abort();
            drop(handle.await);
        }
    }
}

fn loaded_timeline_page(
    loaded_items: &[RoomTimelineItem],
    page_index: usize,
    limit: usize,
) -> Option<Vec<RoomTimelineItem>> {
    if page_index == 0 || limit == 0 {
        return None;
    }

    let loaded_offset = page_index.checked_mul(limit)?;
    let end_index = loaded_items.len().checked_sub(loaded_offset)?;
    if end_index == 0 {
        return None;
    }

    let start_index = end_index.saturating_sub(limit);
    Some(loaded_items[start_index..end_index].to_vec())
}

fn timeline_items_to_shell_items<'item>(
    items: impl IntoIterator<Item = &'item Arc<TimelineItem>>,
    room_id: &str,
) -> Vec<RoomTimelineItem> {
    let mut shell_items = items
        .into_iter()
        .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
        .collect::<Vec<RoomTimelineItem>>();
    canonicalize_timeline_items(&mut shell_items);
    apply_timeline_presentation(&mut shell_items, room_id);
    shell_items
}

fn canonicalize_timeline_items(items: &mut Vec<RoomTimelineItem>) {
    let confirmed_remote_items = items
        .iter()
        .filter(|item| is_confirmed_own_remote_event(item))
        .cloned()
        .collect::<Vec<RoomTimelineItem>>();
    let mut seen_item_ids = std::collections::HashSet::<String>::new();

    items.retain(|item| {
        if !seen_item_ids.insert(item.event_id().to_owned()) {
            return false;
        }

        if !is_transient_local_echo(item) {
            return true;
        }

        !confirmed_remote_items
            .iter()
            .any(|confirmed_item| timeline_items_represent_same_send(item, confirmed_item))
    });
}

fn is_transient_local_echo(item: &RoomTimelineItem) -> bool {
    item.is_own_message() && !is_remote_event_id(item.event_id())
}

fn is_confirmed_own_remote_event(item: &RoomTimelineItem) -> bool {
    item.is_own_message()
        && is_remote_event_id(item.event_id())
        && item.matrix.send_state == RoomTimelineSendState::Sent
}

fn timeline_items_represent_same_send(
    local_echo: &RoomTimelineItem,
    confirmed_item: &RoomTimelineItem,
) -> bool {
    if !is_transient_local_echo(local_echo) || !is_confirmed_own_remote_event(confirmed_item) {
        return false;
    }

    if let (Some(local_transaction_id), Some(confirmed_transaction_id)) = (
        local_echo.matrix.transaction_id.as_deref(),
        confirmed_item.matrix.transaction_id.as_deref(),
    ) && local_transaction_id == confirmed_transaction_id
    {
        return true;
    }

    let timestamp_delta = local_echo
        .timestamp_unix_ms()
        .abs_diff(confirmed_item.timestamp_unix_ms());
    local_echo.sender_id() == confirmed_item.sender_id()
        && local_echo.body() == confirmed_item.body()
        && timestamp_delta <= LOCAL_ECHO_RECONCILIATION_WINDOW_UNIX_MS
}

fn is_remote_event_id(event_id: &str) -> bool {
    event_id.starts_with('$')
}

fn timeline_item_id_from_string(value: &str) -> Result<TimelineEventItemId, String> {
    if value.starts_with('$') {
        let event_id = EventId::parse(value)
            .map_err(|error| format!("Invalid timeline event id: {error}"))?
            .clone();
        return Ok(TimelineEventItemId::EventId(event_id));
    }

    Ok(TimelineEventItemId::TransactionId(value.to_owned().into()))
}

fn timeline_item_to_shell_item(
    item: &matrix_sdk_ui::timeline::TimelineItem,
) -> Option<RoomTimelineItem> {
    let TimelineItemKind::Event(event) = item.kind() else {
        return None;
    };

    let content = event.content();
    let (body, is_edited) = timeline_event_body(content)?;
    let event_id = event
        .event_id()
        .map_or_else(|| event.identifier().to_string(), ToString::to_string);

    let mut shell_item = RoomTimelineItem::text_message(
        event_id,
        event.sender().to_string(),
        sender_display_name(event.sender_profile()),
        body,
        u64::from(event.timestamp().0),
        is_edited,
        event.is_own(),
    );
    shell_item.matrix.transaction_id = event.transaction_id().map(ToString::to_string);
    shell_item.matrix.send_state = timeline_send_state(event.send_state());
    shell_item.matrix.content.is_redacted = event.content().is_redacted();
    shell_item.matrix.reactions = timeline_reactions(event);
    shell_item.matrix.receipts = timeline_receipts(event);
    shell_item.presentation.reply_preview = timeline_reply_preview(event);
    shell_item.presentation.compact_receipts =
        shell_item.matrix.receipts.iter().take(3).cloned().collect();
    shell_item.presentation.capabilities.can_edit = event.is_editable();
    shell_item.presentation.capabilities.can_reply = event.can_be_replied_to();
    Some(shell_item)
}

fn timeline_send_state(send_state: Option<&EventSendState>) -> RoomTimelineSendState {
    match send_state {
        Some(EventSendState::NotSentYet { .. }) => RoomTimelineSendState::Sending,
        Some(EventSendState::SendingFailed { .. }) => RoomTimelineSendState::Failed,
        Some(EventSendState::Sent { .. }) | None => RoomTimelineSendState::Sent,
    }
}

fn timeline_reactions(
    event: &matrix_sdk_ui::timeline::EventTimelineItem,
) -> Vec<RoomTimelineReaction> {
    event
        .content()
        .reactions()
        .map(|reactions| {
            reactions
                .iter()
                .map(|(key, senders)| RoomTimelineReaction {
                    key: key.clone(),
                    count: senders.len() as u64,
                    reacted_by_me: senders.values().any(|reaction| {
                        matches!(
                            reaction.status,
                            ReactionStatus::LocalToLocal(_) | ReactionStatus::LocalToRemote(_)
                        )
                    }),
                })
                .collect::<Vec<RoomTimelineReaction>>()
        })
        .unwrap_or_default()
}

fn timeline_receipts(
    event: &matrix_sdk_ui::timeline::EventTimelineItem,
) -> Vec<RoomTimelineReceipt> {
    event
        .read_receipts()
        .iter()
        .map(|(user_id, receipt)| RoomTimelineReceipt {
            user_id: user_id.to_string(),
            display_name: None,
            avatar_url: None,
            timestamp_unix_ms: receipt.ts.map(|timestamp| u64::from(timestamp.0)),
        })
        .collect::<Vec<RoomTimelineReceipt>>()
}

fn timeline_reply_preview(
    event: &matrix_sdk_ui::timeline::EventTimelineItem,
) -> Option<RoomTimelineReplyPreview> {
    let in_reply_to = event.content().in_reply_to()?;
    let embedded_event = match in_reply_to.event {
        TimelineDetails::Ready(embedded_event) => embedded_event,
        TimelineDetails::Unavailable | TimelineDetails::Pending | TimelineDetails::Error(_) => {
            return Some(RoomTimelineReplyPreview {
                event_id: in_reply_to.event_id.to_string(),
                state: RoomTimelineReplyPreviewState::Loading,
                sender_id: None,
                sender_display_name: None,
                body: None,
                is_redacted: false,
            });
        }
    };
    if embedded_event.content.is_redacted() {
        return Some(RoomTimelineReplyPreview {
            event_id: in_reply_to.event_id.to_string(),
            state: RoomTimelineReplyPreviewState::DeletedRedacted,
            sender_id: Some(embedded_event.sender.to_string()),
            sender_display_name: sender_display_name(&embedded_event.sender_profile),
            body: None,
            is_redacted: true,
        });
    }

    let Some((body, _is_edited)) = timeline_event_body(&embedded_event.content) else {
        return Some(RoomTimelineReplyPreview {
            event_id: in_reply_to.event_id.to_string(),
            state: RoomTimelineReplyPreviewState::InvalidRelation,
            sender_id: Some(embedded_event.sender.to_string()),
            sender_display_name: sender_display_name(&embedded_event.sender_profile),
            body: None,
            is_redacted: false,
        });
    };

    Some(RoomTimelineReplyPreview {
        event_id: in_reply_to.event_id.to_string(),
        state: RoomTimelineReplyPreviewState::Resolved,
        sender_id: Some(embedded_event.sender.to_string()),
        sender_display_name: sender_display_name(&embedded_event.sender_profile),
        body: Some(body),
        is_redacted: embedded_event.content.is_redacted(),
    })
}

fn redacted_event_ids_from_timeline_items<'item>(
    items: impl IntoIterator<Item = &'item Arc<TimelineItem>>,
) -> Vec<String> {
    items
        .into_iter()
        .filter_map(|item| redacted_event_id_from_timeline_item(item.as_ref()))
        .collect()
}

fn redacted_event_id_from_timeline_item(
    item: &matrix_sdk_ui::timeline::TimelineItem,
) -> Option<String> {
    let TimelineItemKind::Event(event) = item.kind() else {
        return None;
    };
    if !event.content().is_redacted() {
        return None;
    }

    event.event_id().map_or_else(
        || Some(event.identifier().to_string()),
        |event_id| Some(event_id.to_string()),
    )
}

fn timeline_event_body(
    content: &matrix_sdk_ui::timeline::TimelineItemContent,
) -> Option<(String, bool)> {
    if let Some(message) = content.as_message() {
        return Some((message.body().to_owned(), message.is_edited()));
    }

    if content.is_unable_to_decrypt() {
        return Some((String::from("Unable to decrypt this message"), false));
    }

    None
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loaded_timeline_page_reveals_existing_older_windows_before_paginating() {
        let loaded_items = (0..80)
            .map(|index| test_timeline_item(format!("${index}")))
            .collect::<Vec<_>>();

        let first_older_page = loaded_timeline_page(&loaded_items, 1, 30).unwrap();
        assert_eq!(first_older_page.first().unwrap().event_id(), "$20");
        assert_eq!(first_older_page.last().unwrap().event_id(), "$49");

        let second_older_page = loaded_timeline_page(&loaded_items, 2, 30).unwrap();
        assert_eq!(second_older_page.first().unwrap().event_id(), "$0");
        assert_eq!(second_older_page.last().unwrap().event_id(), "$19");

        assert!(loaded_timeline_page(&loaded_items, 3, 30).is_none());
    }

    #[test]
    fn canonicalize_timeline_items_replaces_local_echo_with_confirmed_event() {
        let mut local_echo = test_timeline_item(String::from("local-txn"));
        local_echo.matrix.is_own_message = true;
        local_echo.matrix.transaction_id = Some(String::from("local-txn"));
        local_echo.matrix.send_state = RoomTimelineSendState::Sending;

        let mut confirmed_event = test_timeline_item(String::from("$confirmed"));
        confirmed_event.matrix.is_own_message = true;
        confirmed_event.matrix.transaction_id = Some(String::from("local-txn"));
        confirmed_event.matrix.send_state = RoomTimelineSendState::Sent;

        let mut items = vec![local_echo, confirmed_event];
        canonicalize_timeline_items(&mut items);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].event_id(), "$confirmed");
    }

    #[test]
    fn canonicalize_timeline_items_keeps_unmatched_failed_local_echo() {
        let mut local_echo = test_timeline_item(String::from("local-txn"));
        local_echo.matrix.is_own_message = true;
        local_echo.matrix.transaction_id = Some(String::from("local-txn"));
        local_echo.matrix.send_state = RoomTimelineSendState::Failed;

        let confirmed_event = test_timeline_item(String::from("$other"));

        let mut items = vec![local_echo, confirmed_event];
        canonicalize_timeline_items(&mut items);

        assert_eq!(items.len(), 2);
    }

    fn test_timeline_item(event_id: String) -> RoomTimelineItem {
        RoomTimelineItem::text_message(
            event_id,
            String::from("@alice:example.org"),
            None,
            String::from("body"),
            0,
            false,
            false,
        )
    }
}
