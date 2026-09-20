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

use std::{collections::HashSet, path::Path, sync::Arc};

use tauri::async_runtime::Mutex as AsyncMutex;

use super::{
    database::{open_search_connection, search_paths_for_store},
    extract,
    repository::SearchRepository,
    types::{SearchBackfillState, SearchRoomBackfillProgress},
};
use crate::{
    shell::types::{RoomThreadSummary, RoomTimelineItem, SpaceSummary},
    utils::time::now_unix_ms,
};

#[derive(Clone, Default)]
pub(in crate::shell::service) struct SearchIndexer {
    // SQLite writes are short, but serializing them prevents command races from
    // competing for the same per-account FTS tables.
    write_lock: Arc<AsyncMutex<()>>,
}

impl SearchIndexer {
    pub(in crate::shell::service) fn new() -> Self {
        Self::default()
    }

    pub(in crate::shell::service) async fn upsert_room_summary(
        &self,
        account_key: &str,
        store_dir: &Path,
        summary: &RoomThreadSummary,
    ) -> Result<(), String> {
        let _guard = self.write_lock.lock().await;
        let paths = search_paths_for_store(store_dir).map_err(|error| error.to_string())?;
        let connection = open_search_connection(&paths).map_err(|error| error.to_string())?;
        let repository = SearchRepository::new(&connection);
        repository
            .upsert_document(&extract::room_document(account_key, summary))
            .map_err(|error| error.to_string())
    }

    pub(in crate::shell::service) async fn upsert_space_summary(
        &self,
        account_key: &str,
        store_dir: &Path,
        summary: &SpaceSummary,
    ) -> Result<(), String> {
        let _guard = self.write_lock.lock().await;
        let paths = search_paths_for_store(store_dir).map_err(|error| error.to_string())?;
        let connection = open_search_connection(&paths).map_err(|error| error.to_string())?;
        let repository = SearchRepository::new(&connection);
        repository
            .upsert_document(&extract::space_document(account_key, summary))
            .map_err(|error| error.to_string())
    }

    pub(in crate::shell::service) async fn upsert_timeline_items(
        &self,
        account_key: &str,
        store_dir: &Path,
        room_id: &str,
        room_title: &str,
        items: &[RoomTimelineItem],
    ) -> Result<(), String> {
        let _guard = self.write_lock.lock().await;
        let paths = search_paths_for_store(store_dir).map_err(|error| error.to_string())?;
        let connection = open_search_connection(&paths).map_err(|error| error.to_string())?;
        let repository = SearchRepository::new(&connection);

        for item in items {
            let Some(document) = extract::message_document(account_key, room_id, room_title, item)
            else {
                continue;
            };
            repository
                .upsert_document(&document)
                .map_err(|error| error.to_string())?;
        }

        Ok(())
    }

    pub(in crate::shell::service) async fn delete_message_documents(
        &self,
        account_key: &str,
        store_dir: &Path,
        room_id: &str,
        event_ids: &[String],
    ) -> Result<(), String> {
        if event_ids.is_empty() {
            return Ok(());
        }

        let _guard = self.write_lock.lock().await;
        let paths = search_paths_for_store(store_dir).map_err(|error| error.to_string())?;
        let connection = open_search_connection(&paths).map_err(|error| error.to_string())?;
        let repository = SearchRepository::new(&connection);
        let deleted_at_unix_ms = now_unix_ms();

        for event_id in event_ids {
            let document_id = extract::message_document_id(account_key, room_id, event_id);
            repository
                .delete_document(
                    account_key,
                    &document_id,
                    super::types::SearchEntityType::Message,
                    deleted_at_unix_ms,
                )
                .map_err(|error| error.to_string())?;
        }

        Ok(())
    }

    pub(in crate::shell::service) async fn record_room_error(
        &self,
        account_key: &str,
        store_dir: &Path,
        room_id: &str,
        room_kind: &str,
        error_message: &str,
    ) -> Result<(), String> {
        let _guard = self.write_lock.lock().await;
        let paths = search_paths_for_store(store_dir).map_err(|error| error.to_string())?;
        let connection = open_search_connection(&paths).map_err(|error| error.to_string())?;
        let repository = SearchRepository::new(&connection);
        repository
            .record_room_error(
                account_key,
                room_id,
                room_kind,
                error_message,
                now_unix_ms(),
            )
            .map_err(|error| error.to_string())
    }

    pub(in crate::shell::service) async fn update_backfill_progress(
        &self,
        store_dir: &Path,
        progress: &SearchRoomBackfillProgress,
    ) -> Result<(), String> {
        let _guard = self.write_lock.lock().await;
        let paths = search_paths_for_store(store_dir).map_err(|error| error.to_string())?;
        let connection = open_search_connection(&paths).map_err(|error| error.to_string())?;
        let repository = SearchRepository::new(&connection);
        repository
            .update_room_backfill_progress(progress)
            .map_err(|error| error.to_string())
    }

    pub(in crate::shell::service) async fn room_backfill_progress(
        &self,
        account_key: &str,
        store_dir: &Path,
        room_id: &str,
    ) -> Result<Option<SearchRoomBackfillProgress>, String> {
        let _guard = self.write_lock.lock().await;
        let paths = search_paths_for_store(store_dir).map_err(|error| error.to_string())?;
        let connection = open_search_connection(&paths).map_err(|error| error.to_string())?;
        let repository = SearchRepository::new(&connection);
        repository
            .room_backfill_progress(account_key, room_id)
            .map_err(|error| error.to_string())
    }

    pub(in crate::shell::service) async fn mark_backfill_state(
        &self,
        account_key: &str,
        store_dir: &Path,
        room_id: &str,
        state: SearchBackfillState,
        error: Option<String>,
    ) -> Result<(), String> {
        let existing = self
            .room_backfill_progress(account_key, store_dir, room_id)
            .await?;
        let progress = existing.unwrap_or_else(|| SearchRoomBackfillProgress {
            account_key: account_key.to_owned(),
            room_id: room_id.to_owned(),
            room_kind: String::from("conversation"),
            backfill_token: None,
            backfill_state: SearchBackfillState::NotStarted,
            indexed_event_count: 0,
            last_indexed_at_unix_ms: None,
            last_error: None,
        });

        self.update_backfill_progress(
            store_dir,
            &SearchRoomBackfillProgress {
                backfill_state: state,
                last_error: error,
                last_indexed_at_unix_ms: Some(now_unix_ms()),
                ..progress
            },
        )
        .await
    }

    pub(in crate::shell::service) async fn tombstone_stale_rooms(
        &self,
        account_key: &str,
        store_dir: &Path,
        active_room_ids: &HashSet<String>,
    ) -> Result<(), String> {
        self.tombstone_stale_entities(
            account_key,
            store_dir,
            super::types::SearchEntityType::Room,
            active_room_ids,
        )
        .await
    }

    pub(in crate::shell::service) async fn tombstone_stale_spaces(
        &self,
        account_key: &str,
        store_dir: &Path,
        active_space_ids: &HashSet<String>,
    ) -> Result<(), String> {
        self.tombstone_stale_entities(
            account_key,
            store_dir,
            super::types::SearchEntityType::Space,
            active_space_ids,
        )
        .await
    }

    async fn tombstone_stale_entities(
        &self,
        account_key: &str,
        store_dir: &Path,
        entity_type: super::types::SearchEntityType,
        active_entity_ids: &HashSet<String>,
    ) -> Result<(), String> {
        let _guard = self.write_lock.lock().await;
        let paths = search_paths_for_store(store_dir).map_err(|error| error.to_string())?;
        let connection = open_search_connection(&paths).map_err(|error| error.to_string())?;
        let repository = SearchRepository::new(&connection);
        repository
            .tombstone_stale_entity_documents(
                account_key,
                entity_type,
                active_entity_ids,
                now_unix_ms(),
            )
            .map_err(|error| error.to_string())
    }
}
