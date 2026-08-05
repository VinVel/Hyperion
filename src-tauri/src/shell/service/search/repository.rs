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

use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use super::{
    errors::{SearchError, SearchResult},
    ranking,
    types::{
        SearchBackfillState, SearchDocument, SearchEntityType, SearchHitRow,
        SearchRoomBackfillProgress,
    },
};
use crate::shell::types::{GlobalSearchIndexState, GlobalSearchIndexStatus};

const DEFAULT_LAST_INDEXED_TIMESTAMP: u64 = 0;

pub(super) struct SearchRepository<'connection> {
    connection: &'connection Connection,
}

impl<'connection> SearchRepository<'connection> {
    pub(super) fn new(connection: &'connection Connection) -> Self {
        Self { connection }
    }

    pub(super) fn upsert_document(&self, document: &SearchDocument) -> SearchResult<()> {
        let transaction = self.connection.unchecked_transaction()?;
        if document.entity_type == SearchEntityType::Message
            && message_document_is_tombstoned(&transaction, document)?
        {
            transaction.commit()?;
            return Ok(());
        }
        upsert_search_document(&transaction, document)?;
        replace_fts_document(&transaction, document)?;
        update_room_state(&transaction, document)?;
        transaction.commit()?;
        Ok(())
    }

    // Redaction handling will call this once Matrix timeline redaction events are
    // wired into the search indexer.
    pub(super) fn delete_document(
        &self,
        account_key: &str,
        document_id: &str,
        entity_type: SearchEntityType,
        deleted_at_unix_ms: u64,
    ) -> SearchResult<()> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "
            UPDATE search_documents
            SET is_deleted = 1,
                updated_at_unix_ms = ?1
            WHERE account_key = ?2
                AND document_id = ?3
            ",
            params![deleted_at_unix_ms, account_key, document_id],
        )?;
        transaction.execute(
            "DELETE FROM search_documents_fts WHERE document_id = ?1",
            params![document_id],
        )?;
        transaction.execute(
            "
            INSERT INTO search_tombstones (
                account_key,
                document_id,
                entity_type,
                deleted_at_unix_ms
            ) VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(document_id) DO UPDATE SET
                deleted_at_unix_ms = excluded.deleted_at_unix_ms
            ",
            params![
                account_key,
                document_id,
                entity_type.as_str(),
                deleted_at_unix_ms,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub(super) fn record_room_error(
        &self,
        account_key: &str,
        room_id: &str,
        room_kind: &str,
        error_message: &str,
        updated_at_unix_ms: u64,
    ) -> SearchResult<()> {
        self.connection.execute(
            "
            INSERT INTO search_room_state (
                account_key,
                room_id,
                room_kind,
                backfill_state,
                last_indexed_at_unix_ms,
                last_error
            ) VALUES (?1, ?2, ?3, 'error', ?4, ?5)
            ON CONFLICT(account_key, room_id) DO UPDATE SET
                backfill_state = 'error',
                last_indexed_at_unix_ms = excluded.last_indexed_at_unix_ms,
                last_error = excluded.last_error
            ",
            params![
                account_key,
                room_id,
                room_kind,
                updated_at_unix_ms,
                error_message,
            ],
        )?;
        Ok(())
    }

    pub(super) fn update_room_backfill_progress(
        &self,
        progress: &SearchRoomBackfillProgress,
    ) -> SearchResult<()> {
        self.connection.execute(
            "
            INSERT INTO search_room_state (
                account_key,
                room_id,
                room_kind,
                backfill_token,
                backfill_state,
                indexed_event_count,
                last_indexed_at_unix_ms,
                last_error
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(account_key, room_id) DO UPDATE SET
                room_kind = excluded.room_kind,
                backfill_token = excluded.backfill_token,
                backfill_state = excluded.backfill_state,
                indexed_event_count = excluded.indexed_event_count,
                last_indexed_at_unix_ms = excluded.last_indexed_at_unix_ms,
                last_error = excluded.last_error
            ",
            params![
                progress.account_key,
                progress.room_id,
                progress.room_kind,
                progress.backfill_token,
                progress.backfill_state.as_str(),
                progress.indexed_event_count,
                progress.last_indexed_at_unix_ms,
                progress.last_error,
            ],
        )?;
        Ok(())
    }

    pub(super) fn room_backfill_progress(
        &self,
        account_key: &str,
        room_id: &str,
    ) -> SearchResult<Option<SearchRoomBackfillProgress>> {
        let progress = self
            .connection
            .query_row(
                "
                SELECT
                    account_key,
                    room_id,
                    room_kind,
                    backfill_token,
                    backfill_state,
                    indexed_event_count,
                    last_indexed_at_unix_ms,
                    last_error
                FROM search_room_state
                WHERE account_key = ?1
                    AND room_id = ?2
                ",
                params![account_key, room_id],
                |row| {
                    let backfill_state_value: String = row.get(4)?;
                    let backfill_state = SearchBackfillState::from_str(&backfill_state_value)
                        .ok_or_else(|| {
                            rusqlite::Error::FromSqlConversionFailure(
                                4,
                                rusqlite::types::Type::Text,
                                Box::new(SearchError::CorruptDatabase(format!(
                                    "unknown backfill state {backfill_state_value}"
                                ))),
                            )
                        })?;

                    Ok(SearchRoomBackfillProgress {
                        account_key: row.get(0)?,
                        room_id: row.get(1)?,
                        room_kind: row.get(2)?,
                        backfill_token: row.get(3)?,
                        backfill_state,
                        indexed_event_count: row.get(5)?,
                        last_indexed_at_unix_ms: row.get(6)?,
                        last_error: row.get(7)?,
                    })
                },
            )
            .optional()?;
        Ok(progress)
    }

    pub(super) fn tombstone_stale_entity_documents(
        &self,
        account_key: &str,
        entity_type: SearchEntityType,
        active_entity_ids: &HashSet<String>,
        deleted_at_unix_ms: u64,
    ) -> SearchResult<()> {
        let indexed_documents = self.indexed_entity_documents(account_key, entity_type)?;

        for indexed_document in indexed_documents {
            if active_entity_ids.contains(&indexed_document.entity_id) {
                continue;
            }

            self.delete_document(
                account_key,
                &indexed_document.document_id,
                entity_type,
                deleted_at_unix_ms,
            )?;
        }

        Ok(())
    }

    pub(super) fn search_documents(
        &self,
        account_key: &str,
        query: &str,
        limit: usize,
    ) -> SearchResult<Vec<SearchHitRow>> {
        let Some(fts_query) = ranking::fts_query_for_user_query(query) else {
            return Ok(Vec::new());
        };
        let sql_limit = i64::try_from(limit).unwrap_or(i64::MAX);

        let mut statement = self.connection.prepare(
            "
            SELECT
                d.document_id,
                d.entity_type,
                d.room_id,
                d.space_id,
                d.event_id,
                d.title,
                d.subtitle,
                d.body,
                d.timestamp_unix_ms,
                d.sort_timestamp_unix_ms,
                bm25(search_documents_fts) AS rank
            FROM search_documents_fts
            JOIN search_documents d ON d.document_id = search_documents_fts.document_id
            WHERE search_documents_fts MATCH ?1
                AND d.account_key = ?2
                AND d.is_deleted = 0
            ORDER BY rank ASC, d.sort_timestamp_unix_ms DESC
            LIMIT ?3
            ",
        )?;

        let rows = statement.query_map(params![fts_query, account_key, sql_limit], |row| {
            let entity_type_value: String = row.get(1)?;
            let entity_type = SearchEntityType::from_str(&entity_type_value).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    Box::new(SearchError::CorruptDatabase(format!(
                        "unknown search entity type {entity_type_value}"
                    ))),
                )
            })?;

            Ok(SearchHitRow {
                document_id: row.get(0)?,
                entity_type,
                room_id: row.get(2)?,
                space_id: row.get(3)?,
                event_id: row.get(4)?,
                title: row.get(5)?,
                subtitle: row.get(6)?,
                body: row.get(7)?,
                timestamp_unix_ms: row.get(8)?,
                sort_timestamp_unix_ms: row.get(9)?,
                fts_rank: row.get(10)?,
            })
        })?;

        let mut hits = Vec::new();
        for row in rows {
            match row {
                Ok(hit) if search_hit_matches_query(&hit, query) => hits.push(hit),
                Ok(_hit) => {}
                Err(error) => crate::utils::tracing::report_recoverable_error(
                    "shell.search",
                    "read_search_row",
                    "shell.search_row_malformed",
                    "search",
                    &error,
                ),
            }
        }

        Ok(hits)
    }

    pub(super) fn status(&self, account_key: &str) -> SearchResult<GlobalSearchIndexStatus> {
        let indexed_room_count = self.count_documents(account_key, SearchEntityType::Room)?;
        let total_room_count = self.count_room_state(account_key)?;
        let message_count = self.count_documents(account_key, SearchEntityType::Message)?;
        let last_indexed_at_unix_ms = self.last_indexed_at(account_key)?;
        let state = if total_room_count > indexed_room_count {
            GlobalSearchIndexState::Indexing
        } else {
            GlobalSearchIndexState::Idle
        };
        let notice = if state == GlobalSearchIndexState::Indexing {
            Some(String::from("Older history is still being indexed."))
        } else {
            None
        };

        Ok(GlobalSearchIndexStatus {
            state,
            indexed_room_count,
            total_room_count,
            message_count,
            last_indexed_at_unix_ms,
            notice,
        })
    }

    fn count_documents(
        &self,
        account_key: &str,
        entity_type: SearchEntityType,
    ) -> SearchResult<u64> {
        let count = self.connection.query_row(
            "
            SELECT COUNT(*)
            FROM search_documents
            WHERE account_key = ?1
                AND entity_type = ?2
                AND is_deleted = 0
            ",
            params![account_key, entity_type.as_str()],
            |row| row.get::<_, u64>(0),
        )?;
        Ok(count)
    }

    fn count_room_state(&self, account_key: &str) -> SearchResult<u64> {
        let count = self.connection.query_row(
            "SELECT COUNT(*) FROM search_room_state WHERE account_key = ?1",
            params![account_key],
            |row| row.get::<_, u64>(0),
        )?;
        Ok(count)
    }

    fn last_indexed_at(&self, account_key: &str) -> SearchResult<Option<u64>> {
        let timestamp = self
            .connection
            .query_row(
                "
                SELECT MAX(updated_at_unix_ms)
                FROM search_documents
                WHERE account_key = ?1
                    AND is_deleted = 0
                ",
                params![account_key],
                |row| row.get::<_, Option<u64>>(0),
            )
            .optional()?
            .flatten()
            .filter(|timestamp| *timestamp > DEFAULT_LAST_INDEXED_TIMESTAMP);
        Ok(timestamp)
    }

    fn indexed_entity_documents(
        &self,
        account_key: &str,
        entity_type: SearchEntityType,
    ) -> SearchResult<Vec<IndexedEntityDocument>> {
        let entity_id_column = match entity_type {
            SearchEntityType::Room | SearchEntityType::Message => "room_id",
            SearchEntityType::Space => "space_id",
        };
        let sql = format!(
            "
            SELECT document_id, {entity_id_column}
            FROM search_documents
            WHERE account_key = ?1
                AND entity_type = ?2
                AND is_deleted = 0
                AND {entity_id_column} IS NOT NULL
            "
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map(params![account_key, entity_type.as_str()], |row| {
            Ok(IndexedEntityDocument {
                document_id: row.get(0)?,
                entity_id: row.get(1)?,
            })
        })?;

        let mut documents = Vec::new();
        for row in rows {
            documents.push(row?);
        }
        Ok(documents)
    }
}

fn search_hit_matches_query(hit: &SearchHitRow, query: &str) -> bool {
    match hit.entity_type {
        SearchEntityType::Room => ranking::fields_match_user_query(
            [
                hit.title.as_str(),
                hit.subtitle.as_str(),
                hit.room_id.as_deref().unwrap_or_default(),
            ],
            query,
        ),
        SearchEntityType::Message => ranking::body_matches_user_query(&hit.body, query),
        SearchEntityType::Space => true,
    }
}

struct IndexedEntityDocument {
    document_id: String,
    entity_id: String,
}

fn upsert_search_document(
    transaction: &Transaction<'_>,
    document: &SearchDocument,
) -> rusqlite::Result<()> {
    transaction.execute(
        "
        INSERT INTO search_documents (
            account_key,
            document_id,
            entity_type,
            room_id,
            space_id,
            event_id,
            sender_id,
            user_id,
            title,
            subtitle,
            body,
            timestamp_unix_ms,
            sort_timestamp_unix_ms,
            is_deleted,
            updated_at_unix_ms
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
        ON CONFLICT(document_id) DO UPDATE SET
            account_key = excluded.account_key,
            entity_type = excluded.entity_type,
            room_id = excluded.room_id,
            space_id = excluded.space_id,
            event_id = excluded.event_id,
            sender_id = excluded.sender_id,
            user_id = excluded.user_id,
            title = excluded.title,
            subtitle = excluded.subtitle,
            body = excluded.body,
            timestamp_unix_ms = excluded.timestamp_unix_ms,
            sort_timestamp_unix_ms = excluded.sort_timestamp_unix_ms,
            is_deleted = excluded.is_deleted,
            updated_at_unix_ms = excluded.updated_at_unix_ms
        ",
        params![
            document.account_key,
            document.document_id,
            document.entity_type.as_str(),
            document.room_id,
            document.space_id,
            document.event_id,
            document.sender_id,
            document.user_id,
            document.title,
            document.subtitle,
            document.body,
            document.timestamp_unix_ms,
            document.sort_timestamp_unix_ms,
            i64::from(document.is_deleted),
            document.updated_at_unix_ms,
        ],
    )?;
    Ok(())
}

fn message_document_is_tombstoned(
    transaction: &Transaction<'_>,
    document: &SearchDocument,
) -> rusqlite::Result<bool> {
    let is_tombstoned = transaction
        .query_row(
            "
            SELECT 1
            FROM search_tombstones
            WHERE account_key = ?1
                AND document_id = ?2
                AND entity_type = 'message'
            LIMIT 1
            ",
            params![document.account_key, document.document_id],
            |_row| Ok(()),
        )
        .optional()?
        .is_some();

    Ok(is_tombstoned)
}

fn replace_fts_document(
    transaction: &Transaction<'_>,
    document: &SearchDocument,
) -> rusqlite::Result<()> {
    transaction.execute(
        "DELETE FROM search_documents_fts WHERE document_id = ?1",
        params![document.document_id],
    )?;

    if document.is_deleted {
        return Ok(());
    }

    transaction.execute(
        "
        INSERT INTO search_documents_fts (
            document_id,
            title,
            subtitle,
            body,
            sender_id,
            room_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ",
        params![
            document.document_id,
            fts_title(document),
            fts_subtitle(document),
            fts_body(document),
            document.sender_id.as_deref().unwrap_or_default(),
            fts_room_id(document),
        ],
    )?;
    Ok(())
}

fn fts_title(document: &SearchDocument) -> &str {
    if document.entity_type == SearchEntityType::Message {
        return "";
    }

    &document.title
}

fn fts_subtitle(document: &SearchDocument) -> &str {
    if document.entity_type == SearchEntityType::Message {
        return "";
    }

    &document.subtitle
}

fn fts_body(document: &SearchDocument) -> &str {
    if document.entity_type == SearchEntityType::Room {
        return "";
    }

    &document.body
}

fn fts_room_id(document: &SearchDocument) -> &str {
    if document.entity_type == SearchEntityType::Message {
        return "";
    }

    document.room_id.as_deref().unwrap_or_default()
}

fn update_room_state(
    transaction: &Transaction<'_>,
    document: &SearchDocument,
) -> rusqlite::Result<()> {
    let Some(room_id) = &document.room_id else {
        return Ok(());
    };

    let room_kind = match document.entity_type {
        SearchEntityType::Room => "conversation",
        SearchEntityType::Message => "message_room",
        SearchEntityType::Space => "space",
    };
    let indexed_event_increment = i64::from(document.entity_type == SearchEntityType::Message);
    transaction.execute(
        "
        INSERT INTO search_room_state (
            account_key,
            room_id,
            room_kind,
            last_seen_event_id,
            indexed_event_count,
            last_indexed_at_unix_ms
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(account_key, room_id) DO UPDATE SET
            room_kind = excluded.room_kind,
            last_seen_event_id = COALESCE(excluded.last_seen_event_id, search_room_state.last_seen_event_id),
            indexed_event_count = search_room_state.indexed_event_count + ?5,
            last_indexed_at_unix_ms = excluded.last_indexed_at_unix_ms,
            last_error = NULL
        ",
        params![
            document.account_key,
            room_id,
            room_kind,
            document.event_id,
            indexed_event_increment,
            document.updated_at_unix_ms,
        ],
    )?;
    Ok(())
}
