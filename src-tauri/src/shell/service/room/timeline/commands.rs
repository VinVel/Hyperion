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
    collections::{HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};

use matrix_sdk::{Room, ruma::EventId};

use crate::{
    account::{AccountClientSnapshot, AccountManager, ActiveAccount},
    shell::{
        service::{
            DEFAULT_EVENT_CONTEXT_LIMIT, DEFAULT_TIMELINE_LIMIT, MAX_RESTORED_TIMELINE_ITEMS,
            ShellCacheState, ShellManager,
            caching::restored_timeline_limit,
            paging::{
                focused_timeline_page_token, parse_focused_timeline_page_token, timeline_page_token,
            },
            read_state::mark_room_read_locally,
        },
        types::{
            GetRoomEventContextRequest, GetRoomEventRawRequest, GetRoomTimelineRequest,
            PaginateRoomTimelineRequest, ResolveRoomReplyPreviewRequest, RoomTimeline,
            RoomTimelineItem, RoomTimelinePaginationResponse, RoomTimelineReplyPreview,
            RoomTimelineReplyPreviewState, apply_timeline_presentation,
        },
    },
};

use super::super::{resolve_room, room_title};

// A command performs one SDK page. Cursor-advancing empty pages are retried by
// the focused UI with bounded backoff, rather than issuing a burst here.
const MAX_PAGINATION_CONTINUATION_ATTEMPTS: usize = 1;

struct PaginationPageResult {
    accepted_items: Vec<RoomTimelineItem>,
    duplicate_item_count: usize,
    final_next_before: Option<String>,
    last_attempt_index: Option<usize>,
    returned_item_count: usize,
    continuation_attempt_count: usize,
    initial_token_hash: String,
}

struct PaginationAttemptResult {
    items: Vec<RoomTimelineItem>,
    next_before: Option<String>,
}

struct PaginationAttemptRecord<'a> {
    account_key: &'a str,
    room_id: &'a str,
    request: &'a PaginateRoomTimelineRequest,
    known_event_ids: &'a HashSet<&'a str>,
}

impl ShellManager {
    pub async fn get_room_event_raw(
        &self,
        active_account: &ActiveAccount,
        request: GetRoomEventRawRequest,
    ) -> Result<String, String> {
        let account = active_account.snapshot();
        let room = resolve_room(&account.client, &request.room_id)?;
        let event_id = EventId::parse(&request.event_id)
            .map_err(|error| format!("Invalid event id: {error}"))?;
        let event = room
            .event(&event_id, None)
            .await
            .map_err(|error| format!("Could not fetch raw event: {error}"))?;

        Ok(event.raw().json().get().to_owned())
    }

    pub async fn get_room_timeline(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        active_account: &ActiveAccount,
        request: GetRoomTimelineRequest,
    ) -> Result<RoomTimeline, String> {
        let account = active_account.snapshot();

        self.sync_coordinator
            .ensure_account_running(app, account_manager, account.clone())
            .await?;

        let room = resolve_room(&account.client, &request.room_id)?;
        if request.before.is_none() {
            self.mark_room_focused(app, &account.account_key, &room);
        }
        let focused_event = request
            .before
            .as_deref()
            .and_then(parse_focused_timeline_page_token)
            .map(|(event_id, _page_index)| {
                EventId::parse(event_id).map(|event_id| (event_id, DEFAULT_EVENT_CONTEXT_LIMIT))
            })
            .transpose()
            .map_err(|error| format!("Invalid focused event id: {error}"))?;
        let timeline = self
            .sync_coordinator
            .open_timeline_view(app, &account.account_key, &room, focused_event)
            .await?;
        self.prepare_room_timeline_load(account, &room);
        let (mut items, next_before) = self
            .load_room_timeline_items(app, account, &room, &request)
            .await?;
        apply_timeline_presentation(&mut items, room.room_id().as_str());
        self.after_room_timeline_load(account, &room, &request, &items)
            .await?;

        timeline.snapshot(next_before)
    }

    pub async fn paginate_room_timeline_backwards(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        active_account: &ActiveAccount,
        request: PaginateRoomTimelineRequest,
    ) -> Result<RoomTimelinePaginationResponse, String> {
        emit_pagination_diagnostic(
            "pagination.command.enter",
            &[
                ("room_id", request.room_id.as_str()),
                ("request_id", request.request_id.as_str()),
                (
                    "context_type",
                    pagination_context_type(request.before.as_deref()),
                ),
                (
                    "token_before_hash",
                    token_hash(request.before.as_deref()).as_str(),
                ),
            ],
        );
        let account = active_account.snapshot();

        self.sync_coordinator
            .ensure_account_running(app, account_manager, account.clone())
            .await?;

        let room = resolve_room(&account.client, &request.room_id)?;
        self.prepare_room_timeline_load(account, &room);
        let page_result = self
            .load_explicit_pagination_pages(app, account, &room, &request)
            .await?;
        self.index_loaded_timeline_items(account, &room, &page_result.accepted_items)
            .await?;

        let response = pagination_response(room.room_id().as_str(), request, page_result);
        emit_pagination_return_payload(account.account_key.as_str(), &response);
        Ok(response)
    }

    async fn load_explicit_pagination_pages(
        &self,
        app: &tauri::AppHandle,
        account: &AccountClientSnapshot,
        room: &Room,
        request: &PaginateRoomTimelineRequest,
    ) -> Result<PaginationPageResult, String> {
        let page_limit = request.limit.unwrap_or(DEFAULT_TIMELINE_LIMIT);
        let known_event_ids = request
            .known_event_ids
            .iter()
            .map(String::as_str)
            .collect::<HashSet<&str>>();
        let mut current_before = request.before.clone();
        let token_before_hash = token_hash(current_before.as_deref());
        let mut result = PaginationPageResult {
            accepted_items: Vec::new(),
            duplicate_item_count: 0,
            final_next_before: current_before.clone(),
            last_attempt_index: None,
            returned_item_count: 0,
            continuation_attempt_count: 0,
            initial_token_hash: token_before_hash,
        };

        emit_pagination_start(
            account.account_key.as_str(),
            request,
            current_before.as_deref(),
            result.initial_token_hash.as_str(),
        );

        for attempt_index in 0..MAX_PAGINATION_CONTINUATION_ATTEMPTS {
            result.last_attempt_index = Some(attempt_index);
            emit_pagination_diagnostic(
                "pagination.backend.request",
                &[
                    ("account_key", account.account_key.as_str()),
                    ("room_id", request.room_id.as_str()),
                    ("request_id", request.request_id.as_str()),
                    ("attempt_index", attempt_index.to_string().as_str()),
                    (
                        "token_before_hash",
                        token_hash(current_before.as_deref()).as_str(),
                    ),
                ],
            );

            let attempt = self
                .load_explicit_pagination_attempt(
                    app,
                    account,
                    room,
                    request,
                    current_before.clone(),
                    page_limit,
                )
                .await
                .inspect_err(|error| {
                    emit_pagination_diagnostic(
                        "pagination.error",
                        &[
                            ("account_key", account.account_key.as_str()),
                            ("room_id", request.room_id.as_str()),
                            ("request_id", request.request_id.as_str()),
                            ("message", error.as_str()),
                        ],
                    );
                })?;
            let PaginationAttemptResult { items, next_before } = attempt;
            let should_continue = record_pagination_attempt_result(
                &PaginationAttemptRecord {
                    account_key: account.account_key.as_str(),
                    room_id: room.room_id().as_str(),
                    request,
                    known_event_ids: &known_event_ids,
                },
                &mut result,
                &mut current_before,
                items,
                next_before.as_deref(),
            );
            if !should_continue {
                break;
            }
        }

        Ok(result)
    }

    async fn load_explicit_pagination_attempt(
        &self,
        app: &tauri::AppHandle,
        account: &AccountClientSnapshot,
        room: &Room,
        request: &PaginateRoomTimelineRequest,
        current_before: Option<String>,
        page_limit: u16,
    ) -> Result<PaginationAttemptResult, String> {
        if current_before.is_none() {
            let (items, hit_start) = self
                .sync_coordinator
                .paginate_live_timeline_backwards(&account.account_key, room, page_limit, 1)
                .await?;
            let next_before = if hit_start {
                None
            } else {
                Some(timeline_page_token(2))
            };
            return Ok(PaginationAttemptResult { items, next_before });
        }

        let request_for_attempt = GetRoomTimelineRequest {
            room_id: request.room_id.clone(),
            before: current_before,
            limit: Some(page_limit),
        };
        let (items, next_before) = self
            .load_room_timeline_items(app, account, room, &request_for_attempt)
            .await?;
        Ok(PaginationAttemptResult { items, next_before })
    }

    pub async fn get_room_event_context(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        active_account: &ActiveAccount,
        request: GetRoomEventContextRequest,
    ) -> Result<RoomTimeline, String> {
        let account = active_account.snapshot();

        self.sync_coordinator
            .ensure_account_running(app, account_manager, account.clone())
            .await?;

        let room = resolve_room(&account.client, &request.room_id)?;
        let event_id = EventId::parse(&request.event_id)
            .map_err(|error| format!("Invalid event id: {error}"))?
            .clone();
        let context_limit = request.context_limit.unwrap_or(DEFAULT_EVENT_CONTEXT_LIMIT);
        let timeline = self
            .sync_coordinator
            .open_timeline_view(
                app,
                &account.account_key,
                &room,
                Some((event_id.clone(), context_limit)),
            )
            .await?;
        let mut items = self
            .sync_coordinator
            .focused_timeline_items(&account.account_key, &room, event_id.clone(), context_limit)
            .await?;
        apply_timeline_presentation(&mut items, room.room_id().as_str());
        let title = room_title(&room).await?;
        self.search_service
            .index_timeline_items(
                &account.account_key,
                &account.store_dir,
                room.room_id().as_str(),
                &title,
                &items,
            )
            .await;
        self.delete_focused_redacted_timeline_items(
            &account.account_key,
            &account.store_dir,
            &room,
            event_id.clone(),
            context_limit,
        )
        .await;

        timeline.snapshot(Some(focused_timeline_page_token(event_id.as_ref(), 1)))
    }

    pub async fn resolve_room_reply_preview(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        active_account: &ActiveAccount,
        request: ResolveRoomReplyPreviewRequest,
    ) -> Result<RoomTimelineReplyPreview, String> {
        let account = active_account.snapshot();

        let event_id = match EventId::parse(&request.event_id) {
            Ok(event_id) => event_id.clone(),
            Err(_) => {
                return Ok(RoomTimelineReplyPreview {
                    event_id: request.event_id,
                    state: RoomTimelineReplyPreviewState::InvalidRelation,
                    sender_id: None,
                    sender_display_name: None,
                    body: None,
                    is_redacted: false,
                });
            }
        };

        if let Some(cached_item) = ShellCacheState::cached_room_timeline_item(
            &account.account_key,
            &account.store_dir,
            &request.room_id,
            event_id.as_str(),
        ) {
            return Ok(reply_preview_from_timeline_item(
                request.event_id,
                &cached_item,
            ));
        }

        self.sync_coordinator
            .ensure_account_running(app, account_manager, account.clone())
            .await?;

        let room = resolve_room(&account.client, &request.room_id)?;
        let focused_items = match self
            .sync_coordinator
            .focused_timeline_items(
                &account.account_key,
                &room,
                event_id,
                DEFAULT_EVENT_CONTEXT_LIMIT,
            )
            .await
        {
            Ok(items) => items,
            Err(error) => {
                crate::utils::tracing::report_recoverable_error(
                    "shell.timeline",
                    "resolve_reply_preview",
                    "shell.reply_preview_failed",
                    "timeline",
                    &error,
                );
                return Ok(RoomTimelineReplyPreview {
                    event_id: request.event_id,
                    state: RoomTimelineReplyPreviewState::FailedToLoad,
                    sender_id: None,
                    sender_display_name: None,
                    body: None,
                    is_redacted: false,
                });
            }
        };

        Ok(reply_preview_from_focused_items(
            request.event_id,
            &focused_items,
        ))
    }

    fn prepare_room_timeline_load(&self, account: &AccountClientSnapshot, room: &Room) {
        self.sync_coordinator.schedule_room_timeline_warmup(
            account.client.clone(),
            &account.account_key,
            &account.store_dir,
            room.room_id().to_string(),
            self.search_service.indexer_clone(),
        );
    }

    async fn load_room_timeline_items(
        &self,
        app: &tauri::AppHandle,
        account: &AccountClientSnapshot,
        room: &Room,
        request: &GetRoomTimelineRequest,
    ) -> Result<(Vec<crate::shell::types::RoomTimelineItem>, Option<String>), String> {
        let page_limit = request.limit.unwrap_or(DEFAULT_TIMELINE_LIMIT);
        if request.before.is_none() {
            self.sync_coordinator
                .subscribe_live_timeline_updates(
                    app.clone(),
                    &account.account_key,
                    &account.store_dir,
                    room,
                )
                .await?;
            let remembered_count = ShellCacheState::remembered_timeline_item_count(
                &account.account_key,
                &account.store_dir,
                room.room_id().as_str(),
            );
            let restored_limit =
                restored_timeline_limit(page_limit, remembered_count, MAX_RESTORED_TIMELINE_ITEMS);
            return self
                .sync_coordinator
                .load_live_room_timeline(&account.account_key, room, restored_limit, page_limit)
                .await;
        }

        self.sync_coordinator
            .load_paginated_room_timeline(
                &account.account_key,
                room,
                page_limit,
                request.before.as_deref(),
            )
            .await
    }

    async fn after_room_timeline_load(
        &self,
        account: &AccountClientSnapshot,
        room: &Room,
        request: &GetRoomTimelineRequest,
        items: &[crate::shell::types::RoomTimelineItem],
    ) -> Result<(), String> {
        if request.before.is_none()
            && let Some(latest_item) = items.last()
        {
            self.sync_coordinator
                .mark_live_timeline_as_read(&account.account_key, room)
                .await?;
            mark_room_read_locally(
                self.sync_coordinator.locally_read_room_state(),
                &account.account_key,
                room.room_id().as_str(),
                latest_item.event_id(),
            );
        }

        self.index_loaded_timeline_items(account, room, items).await
    }

    async fn index_loaded_timeline_items(
        &self,
        account: &AccountClientSnapshot,
        room: &Room,
        items: &[RoomTimelineItem],
    ) -> Result<(), String> {
        let title = room_title(room).await?;
        self.search_service
            .index_timeline_items(
                &account.account_key,
                &account.store_dir,
                room.room_id().as_str(),
                &title,
                items,
            )
            .await;
        self.delete_redacted_timeline_items(&account.account_key, &account.store_dir, room)
            .await;
        Ok(())
    }
}

fn reply_preview_from_focused_items(
    event_id: String,
    focused_items: &[RoomTimelineItem],
) -> RoomTimelineReplyPreview {
    let Some(item) = focused_items
        .iter()
        .find(|focused_item| focused_item.event_id() == event_id)
    else {
        return RoomTimelineReplyPreview {
            event_id,
            state: RoomTimelineReplyPreviewState::Inaccessible,
            sender_id: None,
            sender_display_name: None,
            body: None,
            is_redacted: false,
        };
    };

    reply_preview_from_timeline_item(event_id, item)
}

fn reply_preview_from_timeline_item(
    event_id: String,
    item: &RoomTimelineItem,
) -> RoomTimelineReplyPreview {
    if item.matrix.content.is_redacted {
        return RoomTimelineReplyPreview {
            event_id,
            state: RoomTimelineReplyPreviewState::DeletedRedacted,
            sender_id: Some(item.sender_id().to_owned()),
            sender_display_name: item.sender_display_name().map(ToOwned::to_owned),
            body: None,
            is_redacted: true,
        };
    }

    let body = item.body().trim();
    if body.is_empty() {
        return RoomTimelineReplyPreview {
            event_id,
            state: RoomTimelineReplyPreviewState::InvalidRelation,
            sender_id: Some(item.sender_id().to_owned()),
            sender_display_name: item.sender_display_name().map(ToOwned::to_owned),
            body: None,
            is_redacted: false,
        };
    }

    RoomTimelineReplyPreview {
        event_id,
        state: RoomTimelineReplyPreviewState::Resolved,
        sender_id: Some(item.sender_id().to_owned()),
        sender_display_name: item.sender_display_name().map(ToOwned::to_owned),
        body: Some(body.to_owned()),
        is_redacted: false,
    }
}

fn pagination_context_type(before: Option<&str>) -> &'static str {
    match before {
        Some(token) if token.starts_with("timeline-ui-event:") => "focused",
        _ => "live",
    }
}

fn token_hash(token: Option<&str>) -> String {
    let Some(token) = token else {
        return String::from("none");
    };

    let mut hasher = DefaultHasher::new();
    token.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn pagination_response(
    room_id: &str,
    request: PaginateRoomTimelineRequest,
    page_result: PaginationPageResult,
) -> RoomTimelinePaginationResponse {
    let final_token_hash = token_hash(page_result.final_next_before.as_deref());
    let token_changed = page_result.initial_token_hash != final_token_hash;
    let reason = pagination_result_reason(&page_result).map(ToOwned::to_owned);

    RoomTimelinePaginationResponse {
        room_id: room_id.to_owned(),
        had_new_items: !page_result.accepted_items.is_empty(),
        new_item_count: page_result.accepted_items.len(),
        returned_item_count: page_result.returned_item_count,
        duplicate_item_count: page_result.duplicate_item_count,
        continuation_attempt_count: page_result.continuation_attempt_count,
        token_changed,
        reached_start: page_result.final_next_before.is_none(),
        next_before: page_result.final_next_before,
        request_id: request.request_id,
        items: page_result.accepted_items,
        reason,
    }
}

fn pagination_result_reason(page_result: &PaginationPageResult) -> Option<&'static str> {
    if !page_result.accepted_items.is_empty() {
        return None;
    }

    if page_result.duplicate_item_count > 0 {
        return Some("duplicate_only");
    }

    Some("empty_result")
}

fn record_pagination_attempt_result(
    record: &PaginationAttemptRecord<'_>,
    result: &mut PaginationPageResult,
    current_before: &mut Option<String>,
    mut items: Vec<RoomTimelineItem>,
    next_before: Option<&str>,
) -> bool {
    let attempt_returned_item_count = items.len();
    result.returned_item_count += attempt_returned_item_count;
    apply_timeline_presentation(&mut items, record.room_id);
    let returned_items = items;
    let visible_items = returned_items
        .iter()
        .filter(|item| !record.known_event_ids.contains(item.event_id()))
        .cloned()
        .collect::<Vec<RoomTimelineItem>>();
    result.duplicate_item_count += attempt_returned_item_count.saturating_sub(visible_items.len());
    emit_pagination_backend_response(
        record.account_key,
        record.request,
        result.returned_item_count,
        visible_items.len(),
        next_before,
    );

    if let Some(next_before) = next_before {
        result.final_next_before = Some(next_before.to_owned());
        *current_before = Some(next_before.to_owned());
    }

    if !visible_items.is_empty() {
        result.accepted_items = visible_items;
        return false;
    }

    if next_before.is_none() {
        return false;
    }

    result.continuation_attempt_count += 1;
    true
}

fn emit_pagination_return_payload(account_key: &str, response: &RoomTimelinePaginationResponse) {
    emit_pagination_diagnostic(
        "pagination.return_payload",
        &[
            ("account_key", account_key),
            ("room_id", response.room_id.as_str()),
            ("request_id", response.request_id.as_str()),
            (
                "returned_item_count",
                response.returned_item_count.to_string().as_str(),
            ),
            (
                "returned_new_item_count",
                response.new_item_count.to_string().as_str(),
            ),
            (
                "duplicate_item_count",
                response.duplicate_item_count.to_string().as_str(),
            ),
        ],
    );
}

fn emit_pagination_start(
    account_key: &str,
    request: &PaginateRoomTimelineRequest,
    current_before: Option<&str>,
    token_before_hash: &str,
) {
    emit_pagination_diagnostic(
        "pagination.start",
        &[
            ("account_key", account_key),
            ("room_id", request.room_id.as_str()),
            ("request_id", request.request_id.as_str()),
            ("context_type", pagination_context_type(current_before)),
            ("token_before_hash", token_before_hash),
        ],
    );
}

fn emit_pagination_backend_response(
    account_key: &str,
    request: &PaginateRoomTimelineRequest,
    returned_item_count: usize,
    new_item_count: usize,
    next_before: Option<&str>,
) {
    emit_pagination_diagnostic(
        "pagination.backend.response",
        &[
            ("account_key", account_key),
            ("room_id", request.room_id.as_str()),
            ("request_id", request.request_id.as_str()),
            (
                "returned_item_count",
                returned_item_count.to_string().as_str(),
            ),
            (
                "returned_new_item_count",
                new_item_count.to_string().as_str(),
            ),
            ("has_next_token", next_before.is_some().to_string().as_str()),
            ("token_after_hash", token_hash(next_before).as_str()),
        ],
    );
}

fn emit_pagination_diagnostic(label: &'static str, fields: &[(&str, &str)]) {
    if !tracing::enabled!(target: "hyperion", tracing::Level::DEBUG) {
        return;
    }

    #[cfg(debug_assertions)]
    let Some(rendered_fields) =
        crate::utils::tracing::changed_diagnostic_fields("timeline.pagination", label, fields)
    else {
        return;
    };
    #[cfg(debug_assertions)]
    tracing::debug!(
        target: "hyperion",
        event_name = label,
        component = "shell.timeline",
        operation = "paginate_backwards",
        diagnostic_details = %rendered_fields,
        "{label}: {rendered_fields}"
    );

    #[cfg(not(debug_assertions))]
    {
        if crate::utils::tracing::changed_diagnostic_fields("timeline.pagination", label, fields)
            .is_none()
        {
            return;
        }
        tracing::debug!(
            target: "hyperion",
            event_name = label,
            component = "shell.timeline",
            operation = "paginate_backwards",
            "{label}"
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::shell::{
        service::paging::timeline_page_token,
        types::{
            PaginateRoomTimelineRequest, RoomTimelineItem, RoomTimelineReplyPreviewState,
            RoomTimelineSendState,
        },
    };

    use super::{PaginationPageResult, pagination_response, reply_preview_from_focused_items};

    #[test]
    fn reply_preview_resolution_reports_inaccessible_target() {
        let preview = reply_preview_from_focused_items(String::from("$missing"), &[]);

        assert_eq!(preview.state, RoomTimelineReplyPreviewState::Inaccessible);
        assert_eq!(preview.event_id, "$missing");
        assert!(preview.body.is_none());
    }

    #[test]
    fn reply_preview_resolution_reports_deleted_target() {
        let mut item = test_timeline_item("$deleted", "Deleted body");
        item.matrix.content.is_redacted = true;

        let preview = reply_preview_from_focused_items(String::from("$deleted"), &[item]);

        assert_eq!(
            preview.state,
            RoomTimelineReplyPreviewState::DeletedRedacted
        );
        assert!(preview.is_redacted);
        assert!(preview.body.is_none());
    }

    #[test]
    fn reply_preview_resolution_reports_resolved_target() {
        let preview = reply_preview_from_focused_items(
            String::from("$target"),
            &[test_timeline_item("$target", "Original body")],
        );

        assert_eq!(preview.state, RoomTimelineReplyPreviewState::Resolved);
        assert_eq!(preview.sender_id.as_deref(), Some("@alice:example.org"));
        assert_eq!(preview.body.as_deref(), Some("Original body"));
    }

    #[test]
    fn duplicate_only_pagination_response_advances_to_final_continuation_token() {
        let start_token = timeline_page_token(1);
        let final_token = timeline_page_token(4);
        let page_result = PaginationPageResult {
            accepted_items: Vec::new(),
            duplicate_item_count: 90,
            final_next_before: Some(final_token.clone()),
            last_attempt_index: Some(2),
            returned_item_count: 90,
            continuation_attempt_count: 3,
            initial_token_hash: super::token_hash(Some(start_token.as_str())),
        };
        let request = PaginateRoomTimelineRequest {
            room_id: String::from("!room:example.org"),
            before: Some(start_token),
            limit: Some(30),
            request_id: String::from("request-1"),
            known_event_ids: Vec::new(),
        };

        let response = pagination_response("!room:example.org", request, page_result);

        assert!(!response.had_new_items);
        assert!(response.token_changed);
        assert_eq!(response.next_before.as_deref(), Some(final_token.as_str()));
        assert_eq!(response.reason.as_deref(), Some("duplicate_only"));
        assert!(!response.reached_start);
    }

    #[test]
    fn terminal_pagination_response_is_distinct_from_an_empty_advanced_cursor() {
        let page_result = PaginationPageResult {
            accepted_items: Vec::new(),
            duplicate_item_count: 0,
            final_next_before: None,
            last_attempt_index: Some(0),
            returned_item_count: 0,
            continuation_attempt_count: 0,
            initial_token_hash: super::token_hash(Some("timeline-ui-page:4")),
        };
        let request = PaginateRoomTimelineRequest {
            room_id: String::from("!room:example.org"),
            before: Some(String::from("timeline-ui-page:4")),
            limit: Some(30),
            request_id: String::from("request-terminal"),
            known_event_ids: Vec::new(),
        };

        let response = pagination_response("!room:example.org", request, page_result);

        assert!(response.reached_start);
        assert!(response.token_changed);
        assert_eq!(response.next_before, None);
    }

    fn test_timeline_item(event_id: &str, body: &str) -> RoomTimelineItem {
        let mut item = RoomTimelineItem::text_message(
            event_id.to_owned(),
            String::from("@alice:example.org"),
            Some(String::from("Alice")),
            body.to_owned(),
            1,
            false,
            false,
        );
        item.matrix.send_state = RoomTimelineSendState::Sent;
        item
    }
}
