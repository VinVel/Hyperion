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

use std::{path::Path, sync::Arc};

use matrix_sdk::{Client, Room, ruma::OwnedEventId};

use crate::shell::{
    service::{
        ShellRoomListKind, paging, room::list::snapshot_room_list_for_account,
        search::SearchIndexer,
    },
    types::RoomTimelineItem,
};

use super::{
    coordinator::ShellSyncCoordinator,
    diagnostics::{emit_sync_diagnostic, emit_timeline_room_diagnostic},
};

impl ShellSyncCoordinator {
    pub(in crate::shell::service) async fn snapshot_room_list(
        &self,
        account_key: &str,
        list_kind: ShellRoomListKind,
    ) -> Result<Vec<Room>, String> {
        emit_sync_diagnostic(
            "sync.room_list.snapshot",
            &[
                ("account_key", account_key),
                ("list_kind", list_kind.label()),
            ],
        );
        snapshot_room_list_for_account(self, account_key, list_kind).await
    }
    pub(in crate::shell::service) fn schedule_room_timeline_warmup(
        &self,
        client: Client,
        account_key: &str,
        store_dir: &Path,
        room_id: String,
        search_indexer: SearchIndexer,
    ) {
        emit_sync_diagnostic(
            "timeline.warmup.schedule",
            &[("account_key", account_key), ("room_id", &room_id)],
        );
        self.timeline_service.schedule_room_timeline_warmup(
            client,
            account_key,
            store_dir,
            room_id,
            search_indexer,
        );
    }
    pub(in crate::shell::service) async fn stop_account_warmups(&self, account_key: &str) {
        emit_sync_diagnostic(
            "timeline.warmup.stop_account",
            &[("account_key", account_key)],
        );
        self.timeline_service
            .stop_account_warmups(account_key)
            .await;
    }
    pub(in crate::shell::service) async fn stop_all_warmups(&self) {
        emit_sync_diagnostic("timeline.warmup.stop_all", &[]);
        self.timeline_service.stop_all_warmups().await;
    }
    pub(in crate::shell::service) fn locally_read_room_state(
        &self,
    ) -> &Arc<std::sync::RwLock<std::collections::HashMap<String, String>>> {
        self.timeline_service.locally_read_room_state()
    }
    pub(in crate::shell::service) async fn open_timeline_view(
        &self,
        app: &tauri::AppHandle,
        account_key: &str,
        room: &Room,
        focused_event: Option<(OwnedEventId, u16)>,
    ) -> Result<Arc<crate::shell::engine::TimelineInstance>, String> {
        self.timeline_service
            .registry()
            .open_timeline_view(app, account_key, room, focused_event)
            .await
    }
    pub(in crate::shell::service) async fn focused_timeline_items(
        &self,
        account_key: &str,
        room: &Room,
        event_id: OwnedEventId,
        context_limit: u16,
    ) -> Result<Vec<RoomTimelineItem>, String> {
        emit_timeline_room_diagnostic("timeline.focused.items", account_key, room);
        self.timeline_service
            .registry()
            .focused_timeline_items(account_key, room, event_id, context_limit)
            .await
    }
    pub(in crate::shell::service) async fn load_live_room_timeline(
        &self,
        account_key: &str,
        room: &Room,
        visible_limit: u16,
        page_size: u16,
    ) -> Result<(Vec<RoomTimelineItem>, Option<String>), String> {
        emit_timeline_room_diagnostic("timeline.live.load", account_key, room);
        paging::load_live_room_timeline(self, account_key, room, visible_limit, page_size).await
    }
    pub(in crate::shell::service) async fn load_paginated_room_timeline(
        &self,
        account_key: &str,
        room: &Room,
        limit: u16,
        before: Option<&str>,
    ) -> Result<(Vec<RoomTimelineItem>, Option<String>), String> {
        emit_timeline_room_diagnostic("timeline.paginated.load", account_key, room);
        paging::load_paginated_room_timeline(self, account_key, room, limit, before).await
    }
    pub(in crate::shell::service) async fn ensure_live_timeline_window(
        &self,
        account_key: &str,
        room: &Room,
        visible_limit: u16,
        fetch_limit: u16,
    ) -> Result<(Vec<RoomTimelineItem>, bool), String> {
        self.timeline_service
            .registry()
            .ensure_live_timeline_window(account_key, room, visible_limit, fetch_limit)
            .await
    }
    pub(in crate::shell::service) async fn paginate_live_timeline_backwards(
        &self,
        account_key: &str,
        room: &Room,
        limit: u16,
        page_index: usize,
    ) -> Result<(Vec<RoomTimelineItem>, bool), String> {
        self.timeline_service
            .registry()
            .paginate_live_timeline_backwards(account_key, room, limit, page_index)
            .await
    }
    pub(in crate::shell::service) async fn paginate_focused_timeline_backwards(
        &self,
        account_key: &str,
        room: &Room,
        event_id: OwnedEventId,
        context_limit: u16,
        limit: u16,
        page_index: usize,
    ) -> Result<(Vec<RoomTimelineItem>, bool), String> {
        self.timeline_service
            .registry()
            .paginate_focused_timeline_backwards(
                account_key,
                room,
                event_id,
                context_limit,
                limit,
                page_index,
            )
            .await
    }
    pub(in crate::shell::service) async fn subscribe_live_timeline_updates(
        &self,
        app: tauri::AppHandle,
        account_key: &str,
        store_dir: &Path,
        room: &Room,
    ) -> Result<(), String> {
        emit_timeline_room_diagnostic("timeline.live.subscribe", account_key, room);
        self.timeline_service
            .registry()
            .subscribe_live_timeline_updates(app, account_key, store_dir, room)
            .await
    }
    pub(in crate::shell::service) async fn live_redacted_event_ids(
        &self,
        account_key: &str,
        room: &Room,
    ) -> Result<Vec<String>, String> {
        emit_timeline_room_diagnostic("timeline.live.redactions", account_key, room);
        self.timeline_service
            .registry()
            .live_redacted_event_ids(account_key, room)
            .await
    }
    pub(in crate::shell::service) async fn live_timeline_item_count(
        &self,
        account_key: &str,
        room: &Room,
    ) -> Result<usize, String> {
        emit_timeline_room_diagnostic("timeline.live.count", account_key, room);
        self.timeline_service
            .registry()
            .live_timeline_item_count(account_key, room)
            .await
    }
    pub(in crate::shell::service) async fn focused_redacted_event_ids(
        &self,
        account_key: &str,
        room: &Room,
        event_id: OwnedEventId,
        context_limit: u16,
    ) -> Result<Vec<String>, String> {
        emit_timeline_room_diagnostic("timeline.focused.redactions", account_key, room);
        self.timeline_service
            .registry()
            .focused_redacted_event_ids(account_key, room, event_id, context_limit)
            .await
    }
    pub(in crate::shell::service) async fn mark_live_timeline_as_read(
        &self,
        account_key: &str,
        room: &Room,
    ) -> Result<(), String> {
        emit_timeline_room_diagnostic("timeline.live.mark_read", account_key, room);
        self.timeline_service
            .registry()
            .mark_live_timeline_as_read(account_key, room)
            .await
    }
    pub(in crate::shell::service) async fn clear_account_timeline_state(&self, account_key: &str) {
        emit_sync_diagnostic(
            "timeline.registry.clear_account",
            &[("account_key", account_key)],
        );
        self.timeline_service
            .registry()
            .clear_account(account_key)
            .await;
    }
    pub(in crate::shell::service) async fn clear_all_timeline_state(&self) {
        emit_sync_diagnostic("timeline.registry.clear_all", &[]);
        self.timeline_service.registry().clear_all().await;
    }
}
