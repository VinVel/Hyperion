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

use std::{fs, io, path::Path};

use rusqlite::Connection;

use super::{
    errors::{SearchError, SearchResult},
    types::{SearchConnection, SearchPaths},
};
use crate::utils::time::now_unix_ms;

const SEARCH_DATABASE_DIR_NAME: &str = "hyperion-search";
const SEARCH_DATABASE_FILE_NAME: &str = "search.sqlite3";
const SEARCH_SCHEMA_VERSION: i64 = 1;

pub(super) fn search_paths_for_store(store_dir: &Path) -> SearchResult<SearchPaths> {
    let account_root = store_dir.parent().ok_or_else(|| {
        SearchError::CorruptDatabase(String::from("account store path has no parent directory"))
    })?;
    let database_dir = account_root.join(SEARCH_DATABASE_DIR_NAME);

    Ok(SearchPaths {
        database_path: database_dir.join(SEARCH_DATABASE_FILE_NAME),
    })
}

pub(super) fn open_search_connection(paths: &SearchPaths) -> SearchResult<Connection> {
    Ok(open_search_connection_with_recovery(paths)?.connection)
}

pub(super) fn open_search_connection_with_recovery(
    paths: &SearchPaths,
) -> SearchResult<SearchConnection> {
    match open_search_connection_without_recovery(paths) {
        Ok(connection) => Ok(SearchConnection {
            connection,
            recovered: false,
        }),
        Err(error) if paths.database_path.exists() => {
            eprintln!("Search database could not be opened; recreating it: {error}");
            move_corrupt_database_aside(paths)?;
            let connection = open_search_connection_without_recovery(paths)?;
            Ok(SearchConnection {
                connection,
                recovered: true,
            })
        }
        Err(error) => Err(error),
    }
}

fn open_search_connection_without_recovery(paths: &SearchPaths) -> SearchResult<Connection> {
    if let Some(parent) = paths.database_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let connection = Connection::open(&paths.database_path)?;
    configure_connection(&connection)?;
    initialize_connection_schema(&connection)?;
    Ok(connection)
}

fn move_corrupt_database_aside(paths: &SearchPaths) -> SearchResult<()> {
    let Some(database_path) = paths.database_path.to_str() else {
        fs::remove_file(&paths.database_path)?;
        return Ok(());
    };

    let suffix = now_unix_ms();
    for path in [
        paths.database_path.clone(),
        format!("{database_path}-wal").into(),
        format!("{database_path}-shm").into(),
    ] {
        if !path.exists() {
            continue;
        }

        let aside_path = path.with_extension(format!(
            "{}.corrupt-{suffix}",
            path.extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or("sqlite3")
        ));
        if let Err(error) = fs::rename(&path, &aside_path)
            && error.kind() != io::ErrorKind::NotFound
        {
            fs::remove_file(&path)?;
        }
    }

    Ok(())
}

pub(super) fn initialize_connection_schema(connection: &Connection) -> SearchResult<()> {
    migrate_database(connection)
}

fn configure_connection(connection: &Connection) -> SearchResult<()> {
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

fn migrate_database(connection: &Connection) -> SearchResult<()> {
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS search_documents (
            account_key TEXT NOT NULL,
            document_id TEXT NOT NULL PRIMARY KEY,
            entity_type TEXT NOT NULL,
            room_id TEXT,
            space_id TEXT,
            event_id TEXT,
            sender_id TEXT,
            user_id TEXT,
            title TEXT NOT NULL DEFAULT '',
            subtitle TEXT NOT NULL DEFAULT '',
            body TEXT NOT NULL DEFAULT '',
            timestamp_unix_ms INTEGER NOT NULL DEFAULT 0,
            sort_timestamp_unix_ms INTEGER NOT NULL DEFAULT 0,
            is_deleted INTEGER NOT NULL DEFAULT 0,
            updated_at_unix_ms INTEGER NOT NULL DEFAULT 0
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS search_documents_fts USING fts5(
            document_id UNINDEXED,
            title,
            subtitle,
            body,
            sender_id,
            room_id
        );

        CREATE TABLE IF NOT EXISTS search_room_state (
            account_key TEXT NOT NULL,
            room_id TEXT NOT NULL,
            room_kind TEXT NOT NULL,
            last_seen_event_id TEXT,
            backfill_token TEXT,
            backfill_state TEXT NOT NULL DEFAULT 'not_started',
            indexed_event_count INTEGER NOT NULL DEFAULT 0,
            last_indexed_at_unix_ms INTEGER,
            last_error TEXT,
            PRIMARY KEY (account_key, room_id)
        );

        CREATE TABLE IF NOT EXISTS search_index_state (
            account_key TEXT PRIMARY KEY,
            schema_version INTEGER NOT NULL,
            index_generation INTEGER NOT NULL DEFAULT 0,
            last_compacted_at_unix_ms INTEGER
        );

        CREATE TABLE IF NOT EXISTS search_tombstones (
            account_key TEXT NOT NULL,
            document_id TEXT NOT NULL PRIMARY KEY,
            entity_type TEXT NOT NULL,
            deleted_at_unix_ms INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS search_documents_account_type_idx
            ON search_documents(account_key, entity_type, is_deleted);
        CREATE INDEX IF NOT EXISTS search_documents_room_idx
            ON search_documents(account_key, room_id, is_deleted);
        ",
    )?;

    let stored_version = connection
        .query_row(
            "SELECT MAX(schema_version) FROM search_index_state",
            [],
            |row| row.get::<_, Option<i64>>(0),
        )?
        .unwrap_or(SEARCH_SCHEMA_VERSION);
    if stored_version > SEARCH_SCHEMA_VERSION {
        return Err(SearchError::CorruptDatabase(format!(
            "schema version {stored_version} is newer than supported version {SEARCH_SCHEMA_VERSION}"
        )));
    }

    Ok(())
}
