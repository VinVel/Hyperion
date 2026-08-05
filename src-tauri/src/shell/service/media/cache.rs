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

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use super::types::DEFAULT_THUMBNAIL_CACHE_ITEM_LIMIT;
use crate::utils::time::now_unix_ms;

// Keep thumbnail bytes separate from the account/session database lifecycle.
const MEDIA_CACHE_DATABASE_FILE_NAME: &str = "media.sqlite3";

type CachedThumbnail = (Vec<u8>, Option<String>);

pub(in crate::shell) fn remember_thumbnail(
    store_dir: &Path,
    cache_key: &str,
    bytes: &[u8],
    mime_type: Option<&str>,
) -> Result<(), String> {
    let connection = media_cache_connection(store_dir)?;
    connection
        .execute(
            "INSERT INTO media_thumbnails (cache_key, mime_type, bytes, last_accessed_unix_ms)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(cache_key) DO UPDATE SET
                mime_type = excluded.mime_type,
                bytes = excluded.bytes,
                last_accessed_unix_ms = excluded.last_accessed_unix_ms",
            params![cache_key, mime_type, bytes, now_unix_ms()],
        )
        .map_err(|error| format!("Failed to remember media thumbnail: {error}"))?;
    prune_thumbnail_cache(&connection, DEFAULT_THUMBNAIL_CACHE_ITEM_LIMIT)
}

pub(in crate::shell) fn cached_thumbnail(
    store_dir: &Path,
    cache_key: &str,
) -> Result<Option<CachedThumbnail>, String> {
    let connection = media_cache_connection(store_dir)?;
    let cached = connection
        .query_row(
            "SELECT bytes, mime_type FROM media_thumbnails WHERE cache_key = ?1",
            params![cache_key],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(|error| format!("Failed to read cached media thumbnail: {error}"))?;
    if cached.is_some() {
        connection
            .execute(
                "UPDATE media_thumbnails SET last_accessed_unix_ms = ?2 WHERE cache_key = ?1",
                params![cache_key, now_unix_ms()],
            )
            .map_err(|error| format!("Failed to update cached media thumbnail: {error}"))?;
    }
    Ok(cached)
}

fn media_cache_connection(store_dir: &Path) -> Result<Connection, String> {
    let database_dir = store_dir.join("hyperion");
    std::fs::create_dir_all(&database_dir)
        .map_err(|error| format!("Failed to create media cache directory: {error}"))?;
    let connection = Connection::open(database_dir.join(MEDIA_CACHE_DATABASE_FILE_NAME))
        .map_err(|error| format!("Failed to open media cache database: {error}"))?;
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS media_thumbnails (
                cache_key TEXT PRIMARY KEY NOT NULL,
                mime_type TEXT,
                bytes BLOB NOT NULL,
                last_accessed_unix_ms INTEGER NOT NULL
            );",
        )
        .map_err(|error| format!("Failed to initialize media cache database: {error}"))?;
    Ok(connection)
}

fn prune_thumbnail_cache(connection: &Connection, item_limit: usize) -> Result<(), String> {
    connection
        .execute(
            "DELETE FROM media_thumbnails
             WHERE cache_key IN (
                SELECT cache_key FROM media_thumbnails
                ORDER BY last_accessed_unix_ms DESC
                LIMIT -1 OFFSET ?1
             )",
            params![i64::try_from(item_limit).unwrap_or(i64::MAX)],
        )
        .map_err(|error| format!("Failed to prune media thumbnail cache: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn thumbnail_cache_prunes_to_limit() {
        let store_dir = test_store_dir();
        let connection = media_cache_connection(&store_dir).unwrap();
        for index in 0_u8..3 {
            connection
                .execute(
                    "INSERT INTO media_thumbnails
                        (cache_key, mime_type, bytes, last_accessed_unix_ms)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        format!("key-{index}"),
                        "image/png",
                        vec![index],
                        i64::from(index)
                    ],
                )
                .unwrap();
        }

        prune_thumbnail_cache(&connection, 2).unwrap();

        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM media_thumbnails", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 2);

        let _ = fs::remove_dir_all(store_dir);
    }

    fn test_store_dir() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let counter = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("hyperion-media-cache-test-{timestamp}-{counter}"));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
