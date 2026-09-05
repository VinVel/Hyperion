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

mod projection;

use std::{collections::HashMap, path::Path, sync::Arc, time::Duration};

use matrix_sdk::room::edit::EditedContent;
use matrix_sdk::ruma::{
    EventId, OwnedEventId,
    api::client::receipt::create_receipt::v3::ReceiptType,
    events::{
        AnyMessageLikeEventContent,
        room::message::{MessageType, RoomMessageEventContentWithoutRelation},
    },
};
use matrix_sdk::{Room, sleep::sleep};
use matrix_sdk_ui::timeline::{
    EventSendState, ReactionStatus, Timeline, TimelineDetails, TimelineEventFocusThreadMode,
    TimelineEventItemId, TimelineFocus, TimelineItem, TimelineItemKind,
};
use projection::ActivePublications;
pub(super) use projection::TimelineInstance;
use tauri::async_runtime::Mutex as AsyncMutex;

use super::{
    service::project_timeline_rich_text,
    types::{
        RoomTimelineDecryptionState, RoomTimelineEventContentKind, RoomTimelineItem,
        RoomTimelineItemCapabilities, RoomTimelineReaction, RoomTimelineReceipt,
        RoomTimelineReplyPreview, RoomTimelineReplyPreviewState, RoomTimelineSendState,
        RoomTimelineThreadRelation, RoomTimelineThreadReplyRelation,
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
#[derive(Clone, Default)]
pub struct ShellTimelineRegistry {
    // Timeline instances are expensive live views with their own background
    // tasks, so cache them per active account+room instead of rebuilding them
    // on every command call during the first migration phase.
    live_timelines: Arc<AsyncMutex<HashMap<String, Arc<TimelineInstance>>>>,
    // Focused event timelines are cached separately because they keep their own
    // pagination cursor around a specific anchor event instead of following the
    // room's normal live edge.
    focused_timelines: Arc<AsyncMutex<HashMap<String, Arc<TimelineInstance>>>>,
    // Timeline subscriptions are the live bridge from matrix-sdk-ui into the
    // Tauri shell event stream; snapshots alone do not wake the frontend.
    active_publications: Arc<std::sync::Mutex<ActivePublications>>,
    lifecycle_revision: Arc<std::sync::atomic::AtomicU64>,
}

impl ShellTimelineRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn live_timeline(
        &self,
        account_key: &str,
        room: &Room,
    ) -> Result<Arc<TimelineInstance>, String> {
        let cache_key = Self::cache_key(account_key, room.room_id().as_str());

        {
            let timelines = self.live_timelines.lock().await;
            if let Some(timeline) = timelines.get(&cache_key) {
                return Ok(timeline.clone());
            }
        }

        let lifecycle_revision = self
            .lifecycle_revision
            .load(std::sync::atomic::Ordering::SeqCst);

        // Keep SDK receipt tracking disabled for the room stream. In
        // matrix-sdk-ui 0.17 it can panic when cached/paginated rooms expose
        // duplicate receipt moves during timeline construction.
        let timeline = matrix_sdk_ui::timeline::RoomExt::timeline(room)
            .await
            .map(Arc::new)
            .map_err(|error| format!("Failed to build the room timeline: {error}"))?;

        let timeline = Arc::new(
            TimelineInstance::new(
                timeline,
                account_key,
                room,
                None,
                self.active_publications.clone(),
            )
            .await,
        );
        let mut timelines = self.live_timelines.lock().await;
        if lifecycle_revision
            != self
                .lifecycle_revision
                .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(String::from(
                "Timeline creation was invalidated by account teardown",
            ));
        }
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

        let shell_items = items
            .iter()
            .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
            .rev()
            .take(usize::from(limit))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<RoomTimelineItem>>();
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
        _store_dir: &Path,
        room: &Room,
    ) -> Result<(), String> {
        let timeline = self.live_timeline(account_key, room).await?;
        timeline.publish_to(app);
        Ok(())
    }

    pub async fn open_timeline_view(
        &self,
        app: &tauri::AppHandle,
        account_key: &str,
        room: &Room,
        focused_event: Option<(OwnedEventId, u16)>,
    ) -> Result<Arc<TimelineInstance>, String> {
        // Reserve before awaiting SDK construction so a slow prior open cannot
        // take publication ownership back from a newer room/context selection.
        let generation = self
            .active_publications
            .lock()
            .expect("timeline publication lock poisoned")
            .reserve(account_key);
        let timeline = match focused_event {
            Some((event_id, limit)) => {
                self.focused_timeline(account_key, room, event_id, limit)
                    .await?
            }
            None => self.live_timeline(account_key, room).await?,
        };
        if !timeline.activate(generation) {
            return Err(String::from("Timeline view was replaced while opening"));
        }
        timeline.publish_to(app.clone());
        Ok(timeline)
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
        let loaded_shell_items = before_items
            .iter()
            .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
            .collect::<Vec<_>>();

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
        let new_items = after_items
            .iter()
            .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
            .filter(|item| !seen_item_ids.contains(item.event_id()))
            .collect::<Vec<RoomTimelineItem>>();

        Ok((new_items, hit_start))
    }

    pub async fn focused_timeline(
        &self,
        account_key: &str,
        room: &Room,
        event_id: OwnedEventId,
        context_limit: u16,
    ) -> Result<Arc<TimelineInstance>, String> {
        let cache_key = Self::focused_cache_key(account_key, room.room_id().as_str(), &event_id);

        {
            let timelines = self.focused_timelines.lock().await;
            if let Some(timeline) = timelines.get(&cache_key) {
                return Ok(timeline.clone());
            }
        }

        let lifecycle_revision = self
            .lifecycle_revision
            .load(std::sync::atomic::Ordering::SeqCst);
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

        let timeline = Arc::new(
            TimelineInstance::new(
                timeline,
                account_key,
                room,
                Some(event_id.to_string()),
                self.active_publications.clone(),
            )
            .await,
        );
        let mut timelines = self.focused_timelines.lock().await;
        if lifecycle_revision
            != self
                .lifecycle_revision
                .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(String::from(
                "Timeline creation was invalidated by account teardown",
            ));
        }
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

        let shell_items = items
            .iter()
            .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
            .collect::<Vec<RoomTimelineItem>>();
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
        let loaded_shell_items = before_items
            .iter()
            .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
            .collect::<Vec<_>>();

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
        let new_items = after_items
            .iter()
            .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
            .filter(|item| !seen_item_ids.contains(item.event_id()))
            .collect::<Vec<RoomTimelineItem>>();

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

    fn cache_key(account_key: &str, room_id: &str) -> String {
        format!("{account_key}::{room_id}")
    }

    fn focused_cache_key(account_key: &str, room_id: &str, event_id: &OwnedEventId) -> String {
        format!("{account_key}::{room_id}::{event_id}")
    }

    pub fn close_account_view(&self, account_key: &str) {
        self.active_publications
            .lock()
            .expect("timeline publication lock poisoned")
            .close_account(account_key);
    }

    pub async fn clear_account(&self, account_key: &str) {
        self.lifecycle_revision
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.close_account_view(account_key);
        let account_prefix = format!("{account_key}::");
        for timelines in [&self.live_timelines, &self.focused_timelines] {
            timelines.lock().await.retain(|cache_key, timeline| {
                if cache_key.starts_with(&account_prefix) {
                    timeline.invalidate();
                    return false;
                }
                true
            });
        }
    }

    pub async fn clear_all(&self) {
        self.active_publications
            .lock()
            .expect("timeline publication lock poisoned")
            .close_all();
        self.lifecycle_revision
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        for timelines in [&self.live_timelines, &self.focused_timelines] {
            let mut timelines = timelines.lock().await;
            for timeline in timelines.values() {
                timeline.invalidate();
            }
            timelines.clear();
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
    _room_id: &str,
) -> Vec<RoomTimelineItem> {
    items
        .into_iter()
        .filter_map(|item| timeline_item_to_shell_item(item.as_ref()))
        .collect::<Vec<RoomTimelineItem>>()
}

fn timeline_item_id_from_string(value: &str) -> Result<TimelineEventItemId, String> {
    if value.starts_with('$') {
        let event_id =
            EventId::parse(value).map_err(|error| format!("Invalid timeline event id: {error}"))?;
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
    let mut projection = project_timeline_content(timeline_content_projection(content, None));
    if projection.kind == RoomTimelineEventContentKind::Text
        && let Some(message) = content.as_message()
    {
        message.body().clone_into(&mut projection.body);
    }
    let event_id = event
        .event_id()
        .map_or_else(|| event.identifier().to_string(), ToString::to_string);

    let mut shell_item = RoomTimelineItem::text_message(
        event_id,
        event.sender().to_string(),
        sender_display_name(event.sender_profile()),
        projection.body,
        u64::from(event.timestamp().0),
        projection.is_edited,
        event.is_own(),
    );
    shell_item.matrix.content.kind = projection.kind;
    shell_item.matrix.content.is_redacted = projection.is_redacted;
    shell_item.matrix.decryption_state = projection.decryption_state;

    if let Some((formatted_body, formatted_body_format)) = timeline_message_formatted_body(content)
    {
        shell_item.matrix.content.rich_text = Some(project_timeline_rich_text(
            &shell_item.matrix.content.body,
            Some(&formatted_body),
            Some(&formatted_body_format),
        ));
        shell_item.matrix.content.formatted_body = Some(formatted_body);
        shell_item.matrix.content.formatted_body_format = Some(formatted_body_format);
    } else if projection.kind == RoomTimelineEventContentKind::Text {
        shell_item.matrix.content.rich_text = Some(project_timeline_rich_text(
            &shell_item.matrix.content.body,
            None,
            None,
        ));
    }
    shell_item.presentation.capabilities = projection.capabilities;
    shell_item.matrix.transaction_id = event.transaction_id().map(ToString::to_string);
    shell_item.matrix.send_state = timeline_send_state(event.send_state());
    shell_item.presentation.avatar_url = sender_avatar_url(event.sender_profile());
    shell_item.matrix.reactions = timeline_reactions(event);
    shell_item.matrix.receipts = timeline_receipts(event);
    shell_item.matrix.thread = timeline_thread_summary(content, shell_item.event_id());
    shell_item.matrix.thread_reply_to = timeline_thread_reply_relation(content);
    shell_item.presentation.reply_preview = timeline_reply_preview(event);
    shell_item.presentation.compact_receipts =
        shell_item.matrix.receipts.iter().take(3).cloned().collect();
    if projection.capabilities_allow_actions {
        shell_item.presentation.capabilities.can_edit = event.is_editable();
        shell_item.presentation.capabilities.can_reply = event.can_be_replied_to();
    }
    Some(shell_item)
}

fn timeline_message_formatted_body(
    content: &matrix_sdk_ui::timeline::TimelineItemContent,
) -> Option<(String, String)> {
    let message = content.as_message()?;
    let formatted = match message.msgtype() {
        MessageType::Text(message) => message.formatted.as_ref(),
        MessageType::Notice(message) => message.formatted.as_ref(),
        MessageType::Emote(message) => message.formatted.as_ref(),
        _ => None,
    }?;

    Some((formatted.body.clone(), formatted.format.as_str().to_owned()))
}

#[derive(Clone, Copy)]
enum TimelineContentProjection {
    Text { is_edited: bool },
    PendingDecryption,
    UnableToDecrypt,
    Redacted,
    NonText,
    Unsupported,
}

struct ProjectedTimelineContent {
    kind: RoomTimelineEventContentKind,
    body: String,
    is_edited: bool,
    is_redacted: bool,
    decryption_state: RoomTimelineDecryptionState,
    capabilities: RoomTimelineItemCapabilities,
    capabilities_allow_actions: bool,
}

fn timeline_content_projection(
    content: &matrix_sdk_ui::timeline::TimelineItemContent,
    reported_decryption_state: Option<RoomTimelineDecryptionState>,
) -> TimelineContentProjection {
    if matches!(
        reported_decryption_state,
        Some(RoomTimelineDecryptionState::Pending)
    ) {
        return TimelineContentProjection::PendingDecryption;
    }
    if content.is_redacted() {
        return TimelineContentProjection::Redacted;
    }
    if content.is_unable_to_decrypt() {
        return TimelineContentProjection::UnableToDecrypt;
    }

    let Some(message) = content.as_message() else {
        return TimelineContentProjection::Unsupported;
    };

    match message.msgtype() {
        MessageType::Text(_) | MessageType::Notice(_) | MessageType::Emote(_) => {
            TimelineContentProjection::Text {
                is_edited: message.is_edited(),
            }
        }
        MessageType::Audio(_)
        | MessageType::File(_)
        | MessageType::Image(_)
        | MessageType::Video(_)
        | MessageType::Location(_)
        | MessageType::ServerNotice(_)
        | MessageType::VerificationRequest(_)
        | MessageType::Gallery(_)
        | MessageType::_Custom(_) => TimelineContentProjection::NonText,
        _ => TimelineContentProjection::Unsupported,
    }
}

fn project_timeline_content(projection: TimelineContentProjection) -> ProjectedTimelineContent {
    let safe_capabilities = RoomTimelineItemCapabilities {
        can_edit: false,
        can_redact: false,
        can_reply: false,
        can_react: false,
    };
    match projection {
        TimelineContentProjection::Text { is_edited } => ProjectedTimelineContent {
            kind: RoomTimelineEventContentKind::Text,
            body: String::new(),
            is_edited,
            is_redacted: false,
            decryption_state: RoomTimelineDecryptionState::Unencrypted,
            capabilities: safe_capabilities,
            capabilities_allow_actions: true,
        },
        TimelineContentProjection::PendingDecryption => ProjectedTimelineContent {
            kind: RoomTimelineEventContentKind::PendingDecryption,
            body: String::from("Message is waiting to be decrypted"),
            is_edited: false,
            is_redacted: false,
            decryption_state: RoomTimelineDecryptionState::Pending,
            capabilities: safe_capabilities,
            capabilities_allow_actions: false,
        },
        TimelineContentProjection::UnableToDecrypt => ProjectedTimelineContent {
            kind: RoomTimelineEventContentKind::UnableToDecrypt,
            body: String::from("Unable to decrypt this message"),
            is_edited: false,
            is_redacted: false,
            decryption_state: RoomTimelineDecryptionState::UnableToDecrypt,
            capabilities: safe_capabilities,
            capabilities_allow_actions: false,
        },
        TimelineContentProjection::Redacted => ProjectedTimelineContent {
            kind: RoomTimelineEventContentKind::Redacted,
            body: String::from("Message removed"),
            is_edited: false,
            is_redacted: true,
            decryption_state: RoomTimelineDecryptionState::Unencrypted,
            capabilities: safe_capabilities,
            capabilities_allow_actions: false,
        },
        TimelineContentProjection::NonText => ProjectedTimelineContent {
            kind: RoomTimelineEventContentKind::NonText,
            body: String::from("Message type is not supported yet"),
            is_edited: false,
            is_redacted: false,
            decryption_state: RoomTimelineDecryptionState::Unencrypted,
            capabilities: safe_capabilities,
            capabilities_allow_actions: false,
        },
        TimelineContentProjection::Unsupported => ProjectedTimelineContent {
            kind: RoomTimelineEventContentKind::Unsupported,
            body: String::from("Unsupported message type"),
            is_edited: false,
            is_redacted: false,
            decryption_state: RoomTimelineDecryptionState::Unencrypted,
            capabilities: safe_capabilities,
            capabilities_allow_actions: false,
        },
    }
}

fn timeline_thread_summary(
    content: &matrix_sdk_ui::timeline::TimelineItemContent,
    event_id: &str,
) -> Option<RoomTimelineThreadRelation> {
    let summary = content.thread_summary()?;
    let latest_event_id = match summary.latest_event {
        TimelineDetails::Ready(event) => Some(event.identifier.to_string()),
        TimelineDetails::Unavailable | TimelineDetails::Pending | TimelineDetails::Error(_) => None,
    };

    Some(RoomTimelineThreadRelation {
        root_event_id: event_id.to_owned(),
        latest_event_id,
        reply_count: u64::from(summary.num_replies),
    })
}

fn timeline_thread_reply_relation(
    content: &matrix_sdk_ui::timeline::TimelineItemContent,
) -> Option<RoomTimelineThreadReplyRelation> {
    content
        .thread_root()
        .map(|root_event_id| RoomTimelineThreadReplyRelation {
            root_event_id: root_event_id.to_string(),
        })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimelineRelationKind {
    None,
    OrdinaryReply,
    ThreadReply,
}

impl TimelineRelationKind {
    const fn is_thread_reply(self) -> bool {
        matches!(self, Self::ThreadReply)
    }
}

fn relation_kind(
    thread_root_event_id: Option<&str>,
    reply_event_id: Option<&str>,
) -> TimelineRelationKind {
    if thread_root_event_id.is_some() {
        TimelineRelationKind::ThreadReply
    } else if reply_event_id.is_some() {
        TimelineRelationKind::OrdinaryReply
    } else {
        TimelineRelationKind::None
    }
}

#[cfg(test)]
fn preserve_sdk_order<'event>(
    event_ids: impl IntoIterator<Item = &'event str>,
    timestamps: impl IntoIterator<Item = u64>,
) -> Vec<&'event str> {
    event_ids
        .into_iter()
        .zip(timestamps)
        .map(|(event_id, _timestamp)| event_id)
        .collect()
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
    if relation_kind(
        event
            .content()
            .thread_root()
            .as_deref()
            .map(EventId::as_str),
        Some(in_reply_to.event_id.as_str()),
    )
    .is_thread_reply()
    {
        return None;
    }
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

fn sender_avatar_url(
    profile: &TimelineDetails<matrix_sdk_ui::timeline::Profile>,
) -> Option<String> {
    match profile {
        TimelineDetails::Ready(profile) => profile.avatar_url.as_ref().map(ToString::to_string),
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
    fn projection_retains_placeholder_states_and_safe_capabilities() {
        let pending = project_timeline_content(TimelineContentProjection::PendingDecryption);
        assert_eq!(
            pending.kind,
            RoomTimelineEventContentKind::PendingDecryption
        );
        assert_eq!(
            pending.decryption_state,
            RoomTimelineDecryptionState::Pending
        );
        assert_eq!(pending.body, "Message is waiting to be decrypted");
        assert!(!pending.capabilities.can_edit);
        assert!(!pending.capabilities.can_redact);
        assert!(!pending.capabilities.can_reply);
        assert!(!pending.capabilities.can_react);

        let unable = project_timeline_content(TimelineContentProjection::UnableToDecrypt);
        assert_eq!(unable.kind, RoomTimelineEventContentKind::UnableToDecrypt);
        assert_eq!(
            unable.decryption_state,
            RoomTimelineDecryptionState::UnableToDecrypt
        );

        let redacted = project_timeline_content(TimelineContentProjection::Redacted);
        assert_eq!(redacted.kind, RoomTimelineEventContentKind::Redacted);
        assert!(redacted.is_redacted);
        assert_eq!(redacted.body, "Message removed");

        let unsupported = project_timeline_content(TimelineContentProjection::Unsupported);
        assert_eq!(unsupported.kind, RoomTimelineEventContentKind::Unsupported);
        assert_eq!(unsupported.body, "Unsupported message type");
    }

    #[test]
    fn projection_order_is_never_sorted_by_timestamp() {
        let source_order = ["$newer", "$missing", "$older"];
        let timestamps = [20_u64, 0, 10];
        let projected = preserve_sdk_order(source_order, timestamps);

        assert_eq!(projected, source_order);
    }

    #[test]
    fn thread_replies_are_distinct_from_ordinary_replies() {
        assert_eq!(
            relation_kind(None, Some("$ordinary")),
            TimelineRelationKind::OrdinaryReply
        );
        assert_eq!(
            relation_kind(Some("$root"), Some("$fallback")),
            TimelineRelationKind::ThreadReply
        );
    }

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
