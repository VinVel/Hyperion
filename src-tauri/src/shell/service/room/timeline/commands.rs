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
    path::Path,
};

use matrix_sdk::{Room, ruma::EventId};

use crate::{
    account::{AccountClientSnapshot, AccountManager, ActiveAccount},
    shell::{
        service::emit_shell_room_updated,
        service::{
            DEFAULT_EVENT_CONTEXT_LIMIT, DEFAULT_TIMELINE_LIMIT, MAX_RESTORED_TIMELINE_ITEMS,
            ShellCacheState, ShellManager,
            caching::restored_timeline_limit,
            paging::{focused_timeline_page_token, parse_timeline_page_token, timeline_page_token},
            read_state::mark_room_read_locally,
        },
        types::{
            GetRoomEventContextRequest, GetRoomTimelineRequest, PaginateRoomTimelineRequest,
            ResolveRoomReplyPreviewRequest, RoomTimeline, RoomTimelineItem,
            RoomTimelinePaginationResponse, RoomTimelineReplyPreview,
            RoomTimelineReplyPreviewState, apply_timeline_presentation,
        },
    },
};

use super::super::{resolve_room, room_title};

const MAX_PAGINATION_CONTINUATION_ATTEMPTS: usize = 3;

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
    cached_items: &'a [RoomTimelineItem],
    known_event_ids: &'a HashSet<&'a str>,
}

impl ShellManager {
    pub async fn get_room_timeline(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        active_account: &ActiveAccount,
        request: GetRoomTimelineRequest,
    ) -> Result<RoomTimeline, String> {
        let account = active_account.snapshot();

        if let Some(cached_timeline) =
            self.cached_timeline_response(app, account_manager, account, &request)
        {
            return Ok(cached_timeline);
        }

        self.sync_coordinator
            .ensure_account_running(app, account_manager, account.clone())
            .await?;

        let room = resolve_room(&account.client, &request.room_id)?;
        self.prepare_room_timeline_load(account, &room);
        let (mut items, next_before) = self
            .load_room_timeline_items(app, account, &room, &request)
            .await?;
        apply_timeline_presentation(&mut items, room.room_id().as_str());
        let redacted_event_ids = self
            .live_redacted_event_ids(&account.account_key, &room)
            .await;
        self.after_room_timeline_load(
            account,
            &room,
            &request,
            &items,
            next_before.as_deref(),
            &redacted_event_ids,
        )
        .await?;

        Ok(RoomTimeline {
            room_id: room.room_id().to_string(),
            items,
            next_before,
            focused_event_id: None,
            redacted_event_ids,
        })
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
        let cached_items_before_pagination = ShellCacheState::cached_room_timeline(
            &account.account_key,
            &account.store_dir,
            &request.room_id,
        )
        .map(|(items, _next_before)| items)
        .unwrap_or_default();
        let backend_reconciled_count = self
            .sync_coordinator
            .live_timeline_item_count(&account.account_key, &room)
            .await
            .ok();
        emit_pagination_authority_snapshot(
            account.account_key.as_str(),
            &request,
            &cached_items_before_pagination,
            backend_reconciled_count,
        );
        emit_pagination_boundary_source(
            account.account_key.as_str(),
            &request,
            &cached_items_before_pagination,
        );
        if let Some(cached_page_result) = reveal_cached_older_pagination_page(
            account,
            &request,
            request.limit.unwrap_or(DEFAULT_TIMELINE_LIMIT),
        ) {
            self.commit_explicit_pagination_page(account, &room, &request, &cached_page_result)
                .await?;
            let response =
                pagination_response(room.room_id().as_str(), request, cached_page_result);
            emit_pagination_return_payload(account.account_key.as_str(), &response);
            return Ok(response);
        }

        let page_result = self
            .load_explicit_pagination_pages(app, account, &room, &request)
            .await?;
        self.commit_explicit_pagination_page(account, &room, &request, &page_result)
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
        let cached_items = ShellCacheState::cached_room_timeline(
            &account.account_key,
            &account.store_dir,
            &request.room_id,
        )
        .map(|(items, _next_before)| items)
        .unwrap_or_default();
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
                    cached_items: &cached_items,
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

    async fn commit_explicit_pagination_page(
        &self,
        account: &AccountClientSnapshot,
        room: &Room,
        request: &PaginateRoomTimelineRequest,
        page_result: &PaginationPageResult,
    ) -> Result<(), String> {
        let redacted_event_ids = self
            .live_redacted_event_ids(&account.account_key, room)
            .await;
        emit_pagination_diagnostic(
            "pagination.commit.start",
            &[
                ("account_key", account.account_key.as_str()),
                ("room_id", request.room_id.as_str()),
                ("request_id", request.request_id.as_str()),
                (
                    "new_committed_item_count",
                    page_result.accepted_items.len().to_string().as_str(),
                ),
            ],
        );
        if page_result.accepted_items.is_empty() {
            commit_pagination_cursor_without_items(account, room, request, page_result);
        } else {
            self.commit_pagination_items(account, room, request, page_result, &redacted_event_ids)
                .await?;
        }

        emit_pagination_token_update(account.account_key.as_str(), request, page_result);
        Ok(())
    }

    async fn commit_pagination_items(
        &self,
        account: &AccountClientSnapshot,
        room: &Room,
        request: &PaginateRoomTimelineRequest,
        page_result: &PaginationPageResult,
        redacted_event_ids: &[String],
    ) -> Result<(), String> {
        let cache_count_before = ShellCacheState::cached_room_timeline(
            &account.account_key,
            &account.store_dir,
            room.room_id().as_str(),
        )
        .map(|(items, _next_before)| items.len())
        .unwrap_or_default();
        self.after_room_timeline_load(
            account,
            room,
            &GetRoomTimelineRequest {
                room_id: request.room_id.clone(),
                before: request.before.clone(),
                limit: request.limit,
            },
            &page_result.accepted_items,
            page_result.final_next_before.as_deref(),
            redacted_event_ids,
        )
        .await?;
        emit_pagination_persist_check(
            account.account_key.as_str(),
            request,
            page_result,
            cache_count_before,
            &account.store_dir,
        );
        emit_pagination_diagnostic(
            "pagination.commit.success",
            &[
                ("account_key", account.account_key.as_str()),
                ("room_id", request.room_id.as_str()),
                ("request_id", request.request_id.as_str()),
                (
                    "new_committed_item_count",
                    page_result.accepted_items.len().to_string().as_str(),
                ),
            ],
        );
        Ok(())
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
        self.mark_room_focused(&account.account_key, room.room_id().as_str());
        let event_id = EventId::parse(&request.event_id)
            .map_err(|error| format!("Invalid event id: {error}"))?
            .clone();
        let context_limit = request.context_limit.unwrap_or(DEFAULT_EVENT_CONTEXT_LIMIT);
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

        Ok(RoomTimeline {
            room_id: room.room_id().to_string(),
            items,
            next_before: Some(focused_timeline_page_token(event_id.as_ref(), 1)),
            focused_event_id: Some(request.event_id),
            redacted_event_ids: self
                .focused_redacted_event_ids(&account.account_key, &room, event_id, context_limit)
                .await,
        })
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

    fn cached_timeline_response(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        account: &AccountClientSnapshot,
        request: &GetRoomTimelineRequest,
    ) -> Option<RoomTimeline> {
        if self
            .cache_state
            .room_timeline_cache_was_served(&account.account_key, &request.room_id)
            && request.before.is_none()
        {
            return None;
        }

        let (items, next_before) = ShellCacheState::cached_room_timeline(
            &account.account_key,
            &account.store_dir,
            &request.room_id,
        )?;
        let (mut items, next_before) = cached_timeline_window(&items, next_before, request)?;
        if request.before.is_none() {
            emit_pagination_diagnostic(
                "pagination.restart_restore_check",
                &[
                    ("account_key", account.account_key.as_str()),
                    ("room_id", request.room_id.as_str()),
                    ("request_id", "room_load_cache"),
                    (
                        "cached_count_on_room_load",
                        items.len().to_string().as_str(),
                    ),
                    ("localStorage_count_on_room_load", "frontend_only"),
                    (
                        "backend_returned_count_on_room_load",
                        items.len().to_string().as_str(),
                    ),
                ],
            );
        }
        apply_timeline_presentation(&mut items, request.room_id.as_str());

        self.mark_room_focused(&account.account_key, &request.room_id);
        if request.before.is_none() {
            self.cache_state
                .mark_room_timeline_cache_served(&account.account_key, &request.room_id);
        }
        self.refresh_room_timeline_in_background(app, account_manager, request.clone());
        Some(RoomTimeline {
            room_id: request.room_id.clone(),
            items,
            next_before,
            focused_event_id: None,
            redacted_event_ids: Vec::new(),
        })
    }

    fn refresh_room_timeline_in_background(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: GetRoomTimelineRequest,
    ) {
        let shell_manager = self.clone();
        let app = app.clone();
        let account_manager = account_manager.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = shell_manager
                .refresh_room_timeline_after_cached_response(&app, &account_manager, request)
                .await
            {
                crate::utils::tracing::report_background_error(
                    "shell.timeline",
                    "refresh_cached_timeline",
                    "shell.timeline_load_failed",
                    "timeline",
                    &error,
                );
            }
        });
    }

    async fn refresh_room_timeline_after_cached_response(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: GetRoomTimelineRequest,
    ) -> Result<(), String> {
        let Some(active_account) = account_manager.optional_active_account(app).await? else {
            return Ok(());
        };
        let account = active_account.snapshot();

        self.sync_coordinator
            .ensure_account_running(app, account_manager, account.clone())
            .await?;

        let room = resolve_room(&account.client, &request.room_id)?;
        self.prepare_room_timeline_load(account, &room);
        let (mut items, next_before) = self
            .load_room_timeline_items(app, account, &room, &request)
            .await?;
        apply_timeline_presentation(&mut items, room.room_id().as_str());
        let redacted_event_ids = self
            .live_redacted_event_ids(&account.account_key, &room)
            .await;
        self.after_room_timeline_load(
            account,
            &room,
            &request,
            &items,
            next_before.as_deref(),
            &redacted_event_ids,
        )
        .await?;
        emit_shell_room_updated(app, &account.account_key, room.room_id().as_str(), false);

        Ok(())
    }

    fn prepare_room_timeline_load(&self, account: &AccountClientSnapshot, room: &Room) {
        self.mark_room_focused(&account.account_key, room.room_id().as_str());
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
            self.sync_coordinator
                .subscribe_typing_updates(app.clone(), &account.account_key, room);
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
        next_before: Option<&str>,
        redacted_event_ids: &[String],
    ) -> Result<(), String> {
        Self::record_room_timeline_pagination(account, room, request, items, next_before);

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
        if request.before.is_none() {
            ShellCacheState::merge_refreshed_timeline(
                &account.account_key,
                &account.store_dir,
                room.room_id().as_str(),
                items,
                next_before,
                redacted_event_ids,
            );
        }

        Ok(())
    }

    fn record_room_timeline_pagination(
        account: &AccountClientSnapshot,
        room: &Room,
        request: &GetRoomTimelineRequest,
        items: &[crate::shell::types::RoomTimelineItem],
        next_before: Option<&str>,
    ) {
        if request.before.is_none() {
            return;
        }

        ShellCacheState::remember_timeline_item_count_after_pagination(
            &account.account_key,
            &account.store_dir,
            room.room_id().as_str(),
            request.before.as_deref(),
            request.limit.unwrap_or(DEFAULT_TIMELINE_LIMIT),
            items.len(),
        );
        ShellCacheState::prepend_cached_timeline_items(
            &account.account_key,
            &account.store_dir,
            room.room_id().as_str(),
            items,
            next_before,
        );
    }

    async fn live_redacted_event_ids(&self, account_key: &str, room: &Room) -> Vec<String> {
        match self
            .sync_coordinator
            .live_redacted_event_ids(account_key, room)
            .await
        {
            Ok(redacted_event_ids) => redacted_event_ids,
            Err(error) => {
                crate::utils::tracing::report_recoverable_error(
                    "shell.timeline",
                    "inspect_redacted_items",
                    "shell.timeline_redaction_scan_failed",
                    "timeline",
                    &error,
                );
                Vec::new()
            }
        }
    }

    async fn focused_redacted_event_ids(
        &self,
        account_key: &str,
        room: &Room,
        event_id: matrix_sdk::ruma::OwnedEventId,
        context_limit: u16,
    ) -> Vec<String> {
        match self
            .sync_coordinator
            .focused_redacted_event_ids(account_key, room, event_id, context_limit)
            .await
        {
            Ok(redacted_event_ids) => redacted_event_ids,
            Err(error) => {
                crate::utils::tracing::report_recoverable_error(
                    "shell.timeline",
                    "inspect_focused_redacted_items",
                    "shell.timeline_redaction_scan_failed",
                    "timeline",
                    &error,
                );
                Vec::new()
            }
        }
    }
}

fn cached_timeline_window(
    items: &[crate::shell::types::RoomTimelineItem],
    next_before: Option<String>,
    request: &GetRoomTimelineRequest,
) -> Option<(Vec<crate::shell::types::RoomTimelineItem>, Option<String>)> {
    let Some(before) = request.before.as_deref() else {
        return Some((items.to_vec(), next_before));
    };
    let page_index = parse_timeline_page_token(before)?;
    let page_limit = usize::from(request.limit.unwrap_or(DEFAULT_TIMELINE_LIMIT));
    if page_limit == 0 {
        return None;
    }

    let visible_count = page_index.saturating_mul(page_limit);
    if items.len() <= visible_count {
        return None;
    }

    let older_end = items.len().saturating_sub(visible_count);
    let older_start = older_end.saturating_sub(page_limit);
    let cached_items = items[older_start..older_end].to_vec();
    if cached_items.is_empty() {
        return None;
    }

    let next_before = if older_start == 0 {
        None
    } else {
        Some(crate::shell::service::paging::timeline_page_token(
            page_index + 1,
        ))
    };

    Some((cached_items, next_before))
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

fn reveal_cached_older_pagination_page(
    account: &AccountClientSnapshot,
    request: &PaginateRoomTimelineRequest,
    page_limit: u16,
) -> Option<PaginationPageResult> {
    let (cached_items, cached_next_before) = ShellCacheState::cached_room_timeline(
        &account.account_key,
        &account.store_dir,
        &request.room_id,
    )?;
    let oldest_visible_event_id = request.known_event_ids.first()?;
    let oldest_visible_cache_index = cached_items
        .iter()
        .position(|item| item.event_id() == oldest_visible_event_id)?;
    let cache_has_older_than_frontend = oldest_visible_cache_index > 0;
    if !cache_has_older_than_frontend {
        emit_pagination_cache_reveal(
            account.account_key.as_str(),
            request,
            false,
            cached_items.len(),
            0,
        );
        return None;
    }

    let page_limit = usize::from(page_limit);
    let page_start_index = oldest_visible_cache_index.saturating_sub(page_limit);
    let known_event_ids = request
        .known_event_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<&str>>();
    let cached_page_items = cached_items[page_start_index..oldest_visible_cache_index]
        .iter()
        .filter(|item| !known_event_ids.contains(item.event_id()))
        .cloned()
        .collect::<Vec<RoomTimelineItem>>();

    emit_pagination_cache_reveal(
        account.account_key.as_str(),
        request,
        true,
        cached_items.len(),
        cached_page_items.len(),
    );
    emit_pagination_page_boundary(
        account.account_key.as_str(),
        request,
        &cached_items,
        &cached_page_items,
    );

    if cached_page_items.is_empty() {
        return None;
    }

    Some(PaginationPageResult {
        accepted_items: cached_page_items,
        duplicate_item_count: 0,
        final_next_before: cached_next_before.or_else(|| request.before.clone()),
        last_attempt_index: None,
        returned_item_count: oldest_visible_cache_index.saturating_sub(page_start_index),
        continuation_attempt_count: 0,
        initial_token_hash: token_hash(request.before.as_deref()),
    })
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
    emit_pagination_snapshot_compare(
        record.account_key,
        record.request,
        attempt_returned_item_count,
        visible_items.len(),
        record.cached_items.len(),
    );
    emit_pagination_page_boundary(
        record.account_key,
        record.request,
        record.cached_items,
        &returned_items,
    );
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

fn commit_pagination_cursor_without_items(
    account: &AccountClientSnapshot,
    room: &Room,
    request: &PaginateRoomTimelineRequest,
    page_result: &PaginationPageResult,
) {
    let cache_count_before = ShellCacheState::cached_room_timeline(
        &account.account_key,
        &account.store_dir,
        room.room_id().as_str(),
    )
    .map(|(items, _next_before)| items.len())
    .unwrap_or_default();
    ShellCacheState::prepend_cached_timeline_items(
        &account.account_key,
        &account.store_dir,
        room.room_id().as_str(),
        &[],
        page_result.final_next_before.as_deref(),
    );
    emit_pagination_persist_check(
        account.account_key.as_str(),
        request,
        page_result,
        cache_count_before,
        &account.store_dir,
    );

    let label = if page_result.duplicate_item_count > 0 {
        "pagination.commit.duplicate_only"
    } else {
        "pagination.empty_result"
    };
    emit_pagination_diagnostic(
        label,
        &[
            ("account_key", account.account_key.as_str()),
            ("room_id", request.room_id.as_str()),
            ("request_id", request.request_id.as_str()),
            (
                "duplicate_item_count",
                page_result.duplicate_item_count.to_string().as_str(),
            ),
        ],
    );
}

fn emit_pagination_cache_reveal(
    account_key: &str,
    request: &PaginateRoomTimelineRequest,
    cache_has_older_than_frontend: bool,
    cache_item_count: usize,
    cached_items_returned_to_frontend: usize,
) {
    emit_pagination_diagnostic(
        "pagination.cache.reveal",
        &[
            ("account_key", account_key),
            ("room_id", request.room_id.as_str()),
            ("request_id", request.request_id.as_str()),
            (
                "cache_has_older_than_frontend",
                cache_has_older_than_frontend.to_string().as_str(),
            ),
            ("cache_item_count", cache_item_count.to_string().as_str()),
            (
                "cached_items_returned_to_frontend",
                cached_items_returned_to_frontend.to_string().as_str(),
            ),
        ],
    );
}

fn emit_pagination_authority_snapshot(
    account_key: &str,
    request: &PaginateRoomTimelineRequest,
    cached_items: &[RoomTimelineItem],
    backend_reconciled_count: Option<usize>,
) {
    let frontend_oldest_event_id = request.known_event_ids.first().map(String::as_str);
    let frontend_newest_event_id = request.known_event_ids.last().map(String::as_str);
    let cache_oldest_event_id = cached_items.first().map(RoomTimelineItem::event_id);
    let cache_newest_event_id = cached_items.last().map(RoomTimelineItem::event_id);
    let frontend_visible_count = request.known_event_ids.len();
    let backend_cache_count = cached_items.len();

    emit_pagination_diagnostic(
        "pagination.authority.snapshot",
        &[
            ("account_key", account_key),
            ("room_id", request.room_id.as_str()),
            ("request_id", request.request_id.as_str()),
            (
                "frontend_visible_count",
                frontend_visible_count.to_string().as_str(),
            ),
            (
                "backend_cache_count",
                backend_cache_count.to_string().as_str(),
            ),
            (
                "backend_reconciled_count",
                backend_reconciled_count
                    .map_or_else(|| String::from("unknown"), |count| count.to_string())
                    .as_str(),
            ),
            (
                "frontend_oldest_event_id",
                frontend_oldest_event_id.unwrap_or("none"),
            ),
            (
                "cache_oldest_event_id",
                cache_oldest_event_id.unwrap_or("none"),
            ),
            (
                "frontend_newest_event_id",
                frontend_newest_event_id.unwrap_or("none"),
            ),
            (
                "cache_newest_event_id",
                cache_newest_event_id.unwrap_or("none"),
            ),
            (
                "counts_match",
                (frontend_visible_count == backend_cache_count)
                    .to_string()
                    .as_str(),
            ),
        ],
    );
}

fn emit_pagination_boundary_source(
    account_key: &str,
    request: &PaginateRoomTimelineRequest,
    cached_items: &[RoomTimelineItem],
) {
    let boundary_event_id = request
        .known_event_ids
        .first()
        .map(String::as_str)
        .or_else(|| cached_items.first().map(RoomTimelineItem::event_id));
    let boundary_source = if !request.known_event_ids.is_empty() {
        "frontend_visible"
    } else if !cached_items.is_empty() {
        "backend_cache"
    } else {
        "token_only"
    };

    emit_pagination_diagnostic(
        "pagination.boundary.source",
        &[
            ("account_key", account_key),
            ("room_id", request.room_id.as_str()),
            ("request_id", request.request_id.as_str()),
            ("boundary_source", boundary_source),
            ("boundary_event_id", boundary_event_id.unwrap_or("none")),
            (
                "token_hash_used",
                token_hash(request.before.as_deref()).as_str(),
            ),
            ("cache_count", cached_items.len().to_string().as_str()),
            (
                "frontend_count",
                request.known_event_ids.len().to_string().as_str(),
            ),
        ],
    );
}

fn emit_pagination_page_boundary(
    account_key: &str,
    request: &PaginateRoomTimelineRequest,
    cached_items: &[RoomTimelineItem],
    returned_items: &[RoomTimelineItem],
) {
    let oldest_visible_event_id = request.known_event_ids.first().map(String::as_str);
    let oldest_cache_event_id = cached_items.first().map(RoomTimelineItem::event_id);
    let returned_first_event_id = returned_items.first().map(RoomTimelineItem::event_id);
    let returned_last_event_id = returned_items.last().map(RoomTimelineItem::event_id);
    let returned_page_is_older_than_visible_boundary =
        returned_page_is_older_than_boundary(returned_items, cached_items, oldest_visible_event_id);
    let returned_page_is_older_than_cache_boundary =
        returned_page_is_older_than_boundary(returned_items, cached_items, oldest_cache_event_id);

    emit_pagination_diagnostic(
        "pagination.page.boundary",
        &[
            ("account_key", account_key),
            ("room_id", request.room_id.as_str()),
            ("request_id", request.request_id.as_str()),
            (
                "current_oldest_visible_event_id",
                oldest_visible_event_id.unwrap_or("none"),
            ),
            (
                "current_oldest_cache_event_id",
                oldest_cache_event_id.unwrap_or("none"),
            ),
            (
                "returned_first_event_id",
                returned_first_event_id.unwrap_or("none"),
            ),
            (
                "returned_last_event_id",
                returned_last_event_id.unwrap_or("none"),
            ),
            (
                "returned_page_is_older_than_visible_boundary",
                returned_page_is_older_than_visible_boundary
                    .to_string()
                    .as_str(),
            ),
            (
                "returned_page_is_older_than_cache_boundary",
                returned_page_is_older_than_cache_boundary
                    .to_string()
                    .as_str(),
            ),
            (
                "cache_contains_returned_first",
                returned_first_event_id
                    .is_some_and(|event_id| cache_contains_event_id(cached_items, event_id))
                    .to_string()
                    .as_str(),
            ),
            (
                "cache_contains_returned_last",
                returned_last_event_id
                    .is_some_and(|event_id| cache_contains_event_id(cached_items, event_id))
                    .to_string()
                    .as_str(),
            ),
            (
                "oldest_loaded_event_id",
                oldest_visible_event_id.unwrap_or("none"),
            ),
            (
                "expected_pagination_boundary_event_id",
                oldest_visible_event_id.unwrap_or("none"),
            ),
        ],
    );

    if !returned_items.is_empty() && !returned_page_is_older_than_visible_boundary {
        emit_pagination_diagnostic(
            "pagination.boundary_mismatch",
            &[
                ("account_key", account_key),
                ("room_id", request.room_id.as_str()),
                ("request_id", request.request_id.as_str()),
                (
                    "current_oldest_visible_event_id",
                    oldest_visible_event_id.unwrap_or("none"),
                ),
                (
                    "returned_first_event_id",
                    returned_first_event_id.unwrap_or("none"),
                ),
                (
                    "returned_last_event_id",
                    returned_last_event_id.unwrap_or("none"),
                ),
                (
                    "returned_page_is_older_than_visible_boundary",
                    returned_page_is_older_than_visible_boundary
                        .to_string()
                        .as_str(),
                ),
            ],
        );
    }
}

fn returned_page_is_older_than_boundary(
    returned_items: &[RoomTimelineItem],
    cached_items: &[RoomTimelineItem],
    boundary_event_id: Option<&str>,
) -> bool {
    let Some(boundary_event_id) = boundary_event_id else {
        return false;
    };
    let Some(returned_last_event_id) = returned_items.last().map(RoomTimelineItem::event_id) else {
        return false;
    };
    let Some(boundary_index) = cached_items
        .iter()
        .position(|item| item.event_id() == boundary_event_id)
    else {
        return false;
    };
    let Some(returned_last_index) = cached_items
        .iter()
        .position(|item| item.event_id() == returned_last_event_id)
    else {
        return false;
    };

    returned_last_index < boundary_index
}

fn cache_contains_event_id(cached_items: &[RoomTimelineItem], event_id: &str) -> bool {
    cached_items.iter().any(|item| item.event_id() == event_id)
}

fn emit_pagination_token_update(
    account_key: &str,
    request: &PaginateRoomTimelineRequest,
    page_result: &PaginationPageResult,
) {
    let final_token_hash = token_hash(page_result.final_next_before.as_deref());
    let token_changed = page_result.initial_token_hash != final_token_hash;
    emit_pagination_diagnostic(
        "pagination.token_update",
        &[
            ("account_key", account_key),
            ("room_id", request.room_id.as_str()),
            ("request_id", request.request_id.as_str()),
            (
                "initial_token_hash",
                page_result.initial_token_hash.as_str(),
            ),
            ("final_token_hash", final_token_hash.as_str()),
            ("token_changed", token_changed.to_string().as_str()),
            (
                "last_attempt_index",
                page_result
                    .last_attempt_index
                    .map_or_else(|| String::from("none"), |index| index.to_string())
                    .as_str(),
            ),
            (
                "new_committed_item_count",
                page_result.accepted_items.len().to_string().as_str(),
            ),
            (
                "duplicate_item_count",
                page_result.duplicate_item_count.to_string().as_str(),
            ),
            (
                "returned_item_count",
                page_result.returned_item_count.to_string().as_str(),
            ),
            (
                "reason",
                pagination_result_reason(page_result).unwrap_or("new_items"),
            ),
        ],
    );
}

fn emit_pagination_persist_check(
    account_key: &str,
    request: &PaginateRoomTimelineRequest,
    page_result: &PaginationPageResult,
    cache_count_before: usize,
    store_dir: &Path,
) {
    let (cached_items_after, _cached_next_before) =
        ShellCacheState::cached_room_timeline(account_key, store_dir, request.room_id.as_str())
            .unwrap_or_default();
    let cached_event_ids_after = cached_items_after
        .iter()
        .map(RoomTimelineItem::event_id)
        .collect::<HashSet<&str>>();
    let new_items_cache_written_count = page_result
        .accepted_items
        .iter()
        .filter(|item| cached_event_ids_after.contains(item.event_id()))
        .count();
    let frontend_count_before = request.known_event_ids.len();
    let frontend_count_after =
        frontend_count_before.saturating_add(page_result.accepted_items.len());
    let persisted_all_visible_new_items =
        new_items_cache_written_count == page_result.accepted_items.len();

    emit_pagination_diagnostic(
        "pagination.persist.check",
        &[
            ("account_key", account_key),
            ("room_id", request.room_id.as_str()),
            ("request_id", request.request_id.as_str()),
            (
                "new_items_visible_count",
                page_result.accepted_items.len().to_string().as_str(),
            ),
            (
                "new_items_cache_written_count",
                new_items_cache_written_count.to_string().as_str(),
            ),
            (
                "cache_count_before",
                cache_count_before.to_string().as_str(),
            ),
            (
                "cache_count_after",
                cached_items_after.len().to_string().as_str(),
            ),
            (
                "frontend_count_before",
                frontend_count_before.to_string().as_str(),
            ),
            (
                "frontend_count_after",
                frontend_count_after.to_string().as_str(),
            ),
            (
                "persisted_all_visible_new_items",
                persisted_all_visible_new_items.to_string().as_str(),
            ),
        ],
    );
}

fn emit_pagination_snapshot_compare(
    account_key: &str,
    request: &PaginateRoomTimelineRequest,
    returned_item_count: usize,
    new_event_count_after_diff: usize,
    cache_count_before: usize,
) {
    emit_pagination_diagnostic(
        "pagination.snapshot.compare",
        &[
            ("account_key", account_key),
            ("room_id", request.room_id.as_str()),
            ("request_id", request.request_id.as_str()),
            (
                "returned_item_count",
                returned_item_count.to_string().as_str(),
            ),
            (
                "known_event_count_before",
                request.known_event_ids.len().to_string().as_str(),
            ),
            (
                "new_event_count_after_diff",
                new_event_count_after_diff.to_string().as_str(),
            ),
            (
                "cache_count_before",
                cache_count_before.to_string().as_str(),
            ),
            (
                "frontend_visible_count_if_available",
                request.known_event_ids.len().to_string().as_str(),
            ),
        ],
    );
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
                "new_committed_item_count",
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
                "new_committed_item_count",
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
