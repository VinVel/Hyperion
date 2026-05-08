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

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use super::{
    super::types::{RoomThreadSummary, RoomTimelineItem, SpaceSummary},
    search::now_unix_ms,
};

// Hyperion only persists timeline view state here. Message/event persistence
// remains owned by the Matrix SDK event cache and is accessed through SDK APIs.
const HYPERION_CACHE_DIR_NAME: &str = "hyperion-cache";
// Keep timeline view state separate from search because it describes how much
// chat UI to restore, not searchable message projection content.
const TIMELINE_VIEW_STATE_DATABASE_NAME: &str = "timeline-view.sqlite3";
// Schema version for the small Hyperion-owned cache metadata database.
const TIMELINE_VIEW_STATE_SCHEMA_VERSION: u32 = 1;

pub(super) type CachedRoomTimeline = (Vec<RoomTimelineItem>, Option<String>);

pub(super) fn remembered_room_timeline_item_count(
    account_key: &str,
    store_dir: &Path,
    room_id: &str,
) -> Result<Option<usize>, String> {
    let connection = open_timeline_view_state_connection(store_dir)?;
    let count = connection
        .query_row(
            r"
            SELECT desired_visible_item_count
            FROM timeline_room_view_state
            WHERE account_key = ?1
              AND room_id = ?2
            ",
            params![account_key, room_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| format!("Failed to read timeline view cache state: {error}"))?;

    let Some(count) = count else {
        return Ok(None);
    };

    usize::try_from(count)
        .map(Some)
        .map_err(|error| format!("Stored timeline item count is invalid: {error}"))
}

pub(super) fn remember_room_timeline_item_count(
    account_key: &str,
    store_dir: &Path,
    room_id: &str,
    visible_item_count: usize,
) -> Result<(), String> {
    let visible_item_count = i64::try_from(visible_item_count)
        .map_err(|error| format!("Timeline item count is too large to persist: {error}"))?;
    let updated_at_unix_ms = i64::try_from(now_unix_ms())
        .map_err(|error| format!("Current timestamp is too large to persist: {error}"))?;
    let connection = open_timeline_view_state_connection(store_dir)?;

    connection
        .execute(
            r"
            INSERT INTO timeline_room_view_state (
                account_key,
                room_id,
                desired_visible_item_count,
                updated_at_unix_ms
            )
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(account_key, room_id) DO UPDATE SET
                desired_visible_item_count = max(
                    timeline_room_view_state.desired_visible_item_count,
                    excluded.desired_visible_item_count
                ),
                updated_at_unix_ms = excluded.updated_at_unix_ms
            ",
            params![account_key, room_id, visible_item_count, updated_at_unix_ms],
        )
        .map_err(|error| format!("Failed to persist timeline view cache state: {error}"))?;

    Ok(())
}

pub(super) fn cached_room_thread_summaries(
    account_key: &str,
    store_dir: &Path,
) -> Result<Vec<RoomThreadSummary>, String> {
    let connection = open_timeline_view_state_connection(store_dir)?;
    let mut statement = connection
        .prepare(
            r"
            SELECT
                room_id,
                title,
                preview,
                participant_label,
                last_activity_unix_ms,
                last_activity_label,
                message_count,
                unread_count,
                homeserver_label,
                avatar_label,
                is_direct
            FROM cached_room_threads
            WHERE account_key = ?1
            ORDER BY last_activity_unix_ms DESC
            ",
        )
        .map_err(|error| format!("Failed to prepare cached room-list query: {error}"))?;

    let rows = statement
        .query_map(params![account_key], |row| {
            Ok(RoomThreadSummary {
                room_id: row.get(0)?,
                title: row.get(1)?,
                preview: row.get(2)?,
                participant_label: row.get(3)?,
                last_activity_unix_ms: row.get(4)?,
                last_activity_label: row.get(5)?,
                message_count: row.get(6)?,
                unread_count: row.get(7)?,
                homeserver_label: row.get(8)?,
                avatar_label: row.get(9)?,
                is_direct: row.get(10)?,
            })
        })
        .map_err(|error| format!("Failed to read cached room-list rows: {error}"))?;

    let mut summaries = Vec::new();
    for row in rows {
        summaries.push(row.map_err(|error| format!("Cached room-list row is invalid: {error}"))?);
    }

    Ok(summaries)
}

pub(super) fn remember_room_thread_summaries(
    account_key: &str,
    store_dir: &Path,
    summaries: &[RoomThreadSummary],
) -> Result<(), String> {
    let mut connection = open_timeline_view_state_connection(store_dir)?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("Failed to start cached room-list transaction: {error}"))?;
    transaction
        .execute(
            "DELETE FROM cached_room_threads WHERE account_key = ?1",
            params![account_key],
        )
        .map_err(|error| format!("Failed to clear stale cached room-list rows: {error}"))?;

    let updated_at_unix_ms = i64::try_from(now_unix_ms())
        .map_err(|error| format!("Current timestamp is too large to persist: {error}"))?;
    {
        let mut statement = transaction
            .prepare(
                r"
                INSERT INTO cached_room_threads (
                    account_key,
                    room_id,
                    title,
                    preview,
                    participant_label,
                    last_activity_unix_ms,
                    last_activity_label,
                    message_count,
                    unread_count,
                    homeserver_label,
                    avatar_label,
                    is_direct,
                    updated_at_unix_ms
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                ",
            )
            .map_err(|error| format!("Failed to prepare cached room-list write: {error}"))?;

        for summary in summaries {
            let last_activity_unix_ms = i64::try_from(summary.last_activity_unix_ms)
                .map_err(|error| format!("Room activity timestamp is too large: {error}"))?;
            let message_count = i64::try_from(summary.message_count)
                .map_err(|error| format!("Room message count is too large: {error}"))?;
            let unread_count = i64::try_from(summary.unread_count)
                .map_err(|error| format!("Room unread count is too large: {error}"))?;
            statement
                .execute(params![
                    account_key,
                    summary.room_id,
                    summary.title,
                    summary.preview,
                    summary.participant_label,
                    last_activity_unix_ms,
                    summary.last_activity_label,
                    message_count,
                    unread_count,
                    summary.homeserver_label,
                    summary.avatar_label,
                    summary.is_direct,
                    updated_at_unix_ms,
                ])
                .map_err(|error| format!("Failed to write cached room-list row: {error}"))?;
        }
    }

    transaction
        .commit()
        .map_err(|error| format!("Failed to commit cached room-list transaction: {error}"))?;
    Ok(())
}

pub(super) fn cached_space_summaries(store_dir: &Path) -> Result<Vec<SpaceSummary>, String> {
    let connection = open_timeline_view_state_connection(store_dir)?;
    let mut statement = connection
        .prepare(
            r"
            SELECT
                space_id,
                name,
                description,
                member_label,
                activity_label,
                accent_label,
                is_official
            FROM cached_spaces
            ORDER BY name ASC
            ",
        )
        .map_err(|error| format!("Failed to prepare cached spaces query: {error}"))?;

    let rows = statement
        .query_map([], |row| {
            Ok(SpaceSummary {
                space_id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                member_label: row.get(3)?,
                activity_label: row.get(4)?,
                accent_label: row.get(5)?,
                is_official: row.get(6)?,
            })
        })
        .map_err(|error| format!("Failed to read cached spaces rows: {error}"))?;

    let mut summaries = Vec::new();
    for row in rows {
        summaries.push(row.map_err(|error| format!("Cached spaces row is invalid: {error}"))?);
    }

    Ok(summaries)
}

pub(super) fn remember_space_summaries(
    store_dir: &Path,
    summaries: &[SpaceSummary],
) -> Result<(), String> {
    let mut connection = open_timeline_view_state_connection(store_dir)?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("Failed to start cached spaces transaction: {error}"))?;
    transaction
        .execute("DELETE FROM cached_spaces", [])
        .map_err(|error| format!("Failed to clear stale cached spaces rows: {error}"))?;

    let updated_at_unix_ms = i64::try_from(now_unix_ms())
        .map_err(|error| format!("Current timestamp is too large to persist: {error}"))?;
    {
        let mut statement = transaction
            .prepare(
                r"
                INSERT INTO cached_spaces (
                    space_id,
                    name,
                    description,
                    member_label,
                    activity_label,
                    accent_label,
                    is_official,
                    updated_at_unix_ms
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ",
            )
            .map_err(|error| format!("Failed to prepare cached spaces write: {error}"))?;

        for summary in summaries {
            statement
                .execute(params![
                    summary.space_id,
                    summary.name,
                    summary.description,
                    summary.member_label,
                    summary.activity_label,
                    summary.accent_label,
                    summary.is_official,
                    updated_at_unix_ms,
                ])
                .map_err(|error| format!("Failed to write cached spaces row: {error}"))?;
        }
    }

    transaction
        .commit()
        .map_err(|error| format!("Failed to commit cached spaces transaction: {error}"))?;
    Ok(())
}

pub(super) fn cached_room_timeline(
    account_key: &str,
    store_dir: &Path,
    room_id: &str,
) -> Result<Option<CachedRoomTimeline>, String> {
    let connection = open_timeline_view_state_connection(store_dir)?;
    let next_before = connection
        .query_row(
            r"
            SELECT next_before
            FROM cached_room_timeline_state
            WHERE account_key = ?1
              AND room_id = ?2
            ",
            params![account_key, room_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|error| format!("Failed to read cached room timeline state: {error}"))?;
    let Some(next_before) = next_before else {
        return Ok(None);
    };

    let mut statement = connection
        .prepare(
            r"
            SELECT
                event_id,
                sender_id,
                sender_display_name,
                body,
                timestamp_unix_ms,
                is_edited,
                is_own_message
            FROM cached_room_timeline_items
            WHERE account_key = ?1
              AND room_id = ?2
            ORDER BY item_index ASC
            ",
        )
        .map_err(|error| format!("Failed to prepare cached room timeline query: {error}"))?;
    let rows = statement
        .query_map(params![account_key, room_id], |row| {
            Ok(RoomTimelineItem {
                event_id: row.get(0)?,
                sender_id: row.get(1)?,
                sender_display_name: row.get(2)?,
                body: row.get(3)?,
                timestamp_unix_ms: row.get(4)?,
                is_edited: row.get(5)?,
                is_own_message: row.get(6)?,
            })
        })
        .map_err(|error| format!("Failed to read cached room timeline rows: {error}"))?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|error| format!("Cached room timeline row is invalid: {error}"))?);
    }
    if items.is_empty() {
        return Ok(None);
    }

    Ok(Some((items, next_before)))
}

pub(super) fn merge_cached_room_timeline_refresh(
    account_key: &str,
    store_dir: &Path,
    room_id: &str,
    refreshed_items: &[RoomTimelineItem],
    next_before: Option<&str>,
    redacted_event_ids: &[String],
) -> Result<(), String> {
    let Some((cached_items, cached_next_before)) =
        cached_room_timeline(account_key, store_dir, room_id)?
    else {
        return write_cached_room_timeline(
            account_key,
            store_dir,
            room_id,
            refreshed_items,
            next_before,
        );
    };

    let redacted_event_ids = redacted_event_ids
        .iter()
        .map(String::as_str)
        .collect::<std::collections::HashSet<&str>>();
    let cached_event_ids = cached_items
        .iter()
        .map(|item| item.event_id.as_str())
        .collect::<std::collections::HashSet<&str>>();
    let refreshed_items_by_id = refreshed_items
        .iter()
        .map(|item| (item.event_id.as_str(), item))
        .collect::<std::collections::HashMap<&str, &RoomTimelineItem>>();
    let mut merged_items = cached_items
        .iter()
        .filter(|item| !redacted_event_ids.contains(item.event_id.as_str()))
        .map(|item| {
            refreshed_items_by_id
                .get(item.event_id.as_str())
                .map_or_else(|| item.clone(), |refreshed_item| (*refreshed_item).clone())
        })
        .collect::<Vec<RoomTimelineItem>>();

    merged_items.extend(
        refreshed_items
            .iter()
            .filter(|item| {
                !cached_event_ids.contains(item.event_id.as_str())
                    && !redacted_event_ids.contains(item.event_id.as_str())
            })
            .cloned(),
    );

    write_cached_room_timeline(
        account_key,
        store_dir,
        room_id,
        &merged_items,
        cached_next_before.as_deref(),
    )
}

pub(super) fn prepend_cached_room_timeline_items(
    account_key: &str,
    store_dir: &Path,
    room_id: &str,
    older_items: &[RoomTimelineItem],
    next_before: Option<&str>,
) -> Result<(), String> {
    let Some((cached_items, _cached_next_before)) =
        cached_room_timeline(account_key, store_dir, room_id)?
    else {
        return write_cached_room_timeline(
            account_key,
            store_dir,
            room_id,
            older_items,
            next_before,
        );
    };

    let mut seen_event_ids = cached_items
        .iter()
        .map(|item| item.event_id.clone())
        .collect::<std::collections::HashSet<String>>();
    let mut merged_items: Vec<RoomTimelineItem> = Vec::new();
    for item in older_items {
        if seen_event_ids.insert(item.event_id.clone()) {
            merged_items.push(item.clone());
        }
    }
    merged_items.extend(cached_items);

    write_cached_room_timeline(account_key, store_dir, room_id, &merged_items, next_before)
}

fn write_cached_room_timeline(
    account_key: &str,
    store_dir: &Path,
    room_id: &str,
    items: &[RoomTimelineItem],
    next_before: Option<&str>,
) -> Result<(), String> {
    let mut connection = open_timeline_view_state_connection(store_dir)?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("Failed to start cached timeline transaction: {error}"))?;
    transaction
        .execute(
            r"
            INSERT INTO cached_room_timeline_state (
                account_key,
                room_id,
                next_before,
                updated_at_unix_ms
            )
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(account_key, room_id) DO UPDATE SET
                next_before = excluded.next_before,
                updated_at_unix_ms = excluded.updated_at_unix_ms
            ",
            params![
                account_key,
                room_id,
                next_before,
                i64::try_from(now_unix_ms())
                    .map_err(|error| format!("Current timestamp is too large: {error}"))?,
            ],
        )
        .map_err(|error| format!("Failed to write cached timeline state: {error}"))?;
    transaction
        .execute(
            r"
            DELETE FROM cached_room_timeline_items
            WHERE account_key = ?1
              AND room_id = ?2
            ",
            params![account_key, room_id],
        )
        .map_err(|error| format!("Failed to clear cached timeline items: {error}"))?;

    {
        let mut statement = transaction
            .prepare(
                r"
                INSERT INTO cached_room_timeline_items (
                    account_key,
                    room_id,
                    item_index,
                    event_id,
                    sender_id,
                    sender_display_name,
                    body,
                    timestamp_unix_ms,
                    is_edited,
                    is_own_message
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ",
            )
            .map_err(|error| format!("Failed to prepare cached timeline write: {error}"))?;
        for (item_index, item) in items.iter().enumerate() {
            statement
                .execute(params![
                    account_key,
                    room_id,
                    i64::try_from(item_index)
                        .map_err(|error| format!("Timeline item index is too large: {error}"))?,
                    item.event_id,
                    item.sender_id,
                    item.sender_display_name,
                    item.body,
                    i64::try_from(item.timestamp_unix_ms)
                        .map_err(|error| format!("Timeline timestamp is too large: {error}"))?,
                    item.is_edited,
                    item.is_own_message,
                ])
                .map_err(|error| format!("Failed to write cached timeline item: {error}"))?;
        }
    }

    transaction
        .commit()
        .map_err(|error| format!("Failed to commit cached timeline transaction: {error}"))?;
    Ok(())
}

fn open_timeline_view_state_connection(store_dir: &Path) -> Result<Connection, String> {
    let database_path = timeline_view_state_database_path(store_dir)?;
    if let Some(database_dir) = database_path.parent() {
        std::fs::create_dir_all(database_dir)
            .map_err(|error| format!("Failed to create timeline cache directory: {error}"))?;
    }

    let connection = Connection::open(&database_path)
        .map_err(|error| format!("Failed to open timeline view cache database: {error}"))?;
    initialize_timeline_view_state_schema(&connection)?;
    Ok(connection)
}

fn timeline_view_state_database_path(store_dir: &Path) -> Result<PathBuf, String> {
    let account_root = store_dir
        .parent()
        .ok_or_else(|| String::from("Account store directory has no parent"))?;

    Ok(account_root
        .join(HYPERION_CACHE_DIR_NAME)
        .join(TIMELINE_VIEW_STATE_DATABASE_NAME))
}

fn initialize_timeline_view_state_schema(connection: &Connection) -> Result<(), String> {
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(|error| format!("Failed to configure timeline cache journal mode: {error}"))?;
    connection
        .execute_batch(&format!(
            r"
            CREATE TABLE IF NOT EXISTS timeline_cache_state (
                key TEXT PRIMARY KEY,
                value INTEGER NOT NULL
            );

            INSERT INTO timeline_cache_state (key, value)
            VALUES ('schema_version', {TIMELINE_VIEW_STATE_SCHEMA_VERSION})
            ON CONFLICT(key) DO NOTHING;

            CREATE TABLE IF NOT EXISTS timeline_room_view_state (
                account_key TEXT NOT NULL,
                room_id TEXT NOT NULL,
                desired_visible_item_count INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY (account_key, room_id)
            );

            CREATE TABLE IF NOT EXISTS cached_room_threads (
                account_key TEXT NOT NULL,
                room_id TEXT NOT NULL,
                title TEXT NOT NULL,
                preview TEXT NOT NULL,
                participant_label TEXT NOT NULL,
                last_activity_unix_ms INTEGER NOT NULL,
                last_activity_label TEXT NOT NULL,
                message_count INTEGER NOT NULL,
                unread_count INTEGER NOT NULL,
                homeserver_label TEXT NOT NULL,
                avatar_label TEXT,
                is_direct INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY (account_key, room_id)
            );

            CREATE INDEX IF NOT EXISTS cached_room_threads_activity_idx
                ON cached_room_threads(account_key, last_activity_unix_ms DESC);

            CREATE TABLE IF NOT EXISTS cached_room_timeline_state (
                account_key TEXT NOT NULL,
                room_id TEXT NOT NULL,
                next_before TEXT,
                updated_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY (account_key, room_id)
            );

            CREATE TABLE IF NOT EXISTS cached_room_timeline_items (
                account_key TEXT NOT NULL,
                room_id TEXT NOT NULL,
                item_index INTEGER NOT NULL,
                event_id TEXT NOT NULL,
                sender_id TEXT NOT NULL,
                sender_display_name TEXT,
                body TEXT NOT NULL,
                timestamp_unix_ms INTEGER NOT NULL,
                is_edited INTEGER,
                is_own_message INTEGER NOT NULL,
                PRIMARY KEY (account_key, room_id, event_id)
            );

            CREATE INDEX IF NOT EXISTS cached_room_timeline_items_order_idx
                ON cached_room_timeline_items(account_key, room_id, item_index);

            CREATE TABLE IF NOT EXISTS cached_spaces (
                space_id TEXT NOT NULL PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL,
                member_label TEXT NOT NULL,
                activity_label TEXT NOT NULL,
                accent_label TEXT,
                is_official INTEGER,
                updated_at_unix_ms INTEGER NOT NULL
            );
            "
        ))
        .map_err(|error| format!("Failed to initialize timeline cache schema: {error}"))?;

    Ok(())
}

pub(super) fn restored_timeline_limit(
    requested_limit: u16,
    remembered_count: Option<usize>,
    maximum_restored_count: usize,
) -> u16 {
    let requested_limit = usize::from(requested_limit);
    let Some(remembered_count) = remembered_count else {
        return requested_limit.try_into().unwrap_or(u16::MAX);
    };

    let restored_count = remembered_count
        .min(maximum_restored_count)
        .max(requested_limit);
    restored_count.try_into().unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use std::{
        fs, io,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use super::*;
    use crate::shell::service::paging::timeline_page_token;

    const ACCOUNT_KEY: &str = "@alice:example.org";
    const ROOM_ID: &str = "!room:example.org";
    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn timeline_view_state_keeps_largest_visible_count() {
        let store_root = unique_test_dir();
        let store_dir = store_root.join("store");
        fs::create_dir_all(&store_dir).unwrap();

        remember_room_timeline_item_count(ACCOUNT_KEY, &store_dir, ROOM_ID, 200).unwrap();
        remember_room_timeline_item_count(ACCOUNT_KEY, &store_dir, ROOM_ID, 60).unwrap();

        let count = remembered_room_timeline_item_count(ACCOUNT_KEY, &store_dir, ROOM_ID).unwrap();
        assert_eq!(count, Some(200));

        remove_test_dir(&store_root).unwrap();
    }

    #[test]
    fn cached_room_threads_round_trip() {
        let store_root = unique_test_dir();
        let store_dir = store_root.join("store");
        fs::create_dir_all(&store_dir).unwrap();
        let summaries = vec![
            test_room_summary("!old:example.org", 1_000),
            test_room_summary("!new:example.org", 2_000),
        ];

        remember_room_thread_summaries(ACCOUNT_KEY, &store_dir, &summaries).unwrap();

        let cached = cached_room_thread_summaries(ACCOUNT_KEY, &store_dir).unwrap();
        assert_eq!(cached.len(), 2);
        assert_eq!(cached[0].room_id, "!new:example.org");
        assert_eq!(cached[1].room_id, "!old:example.org");

        remove_test_dir(&store_root).unwrap();
    }

    #[test]
    fn restored_timeline_limit_uses_remembered_count_with_startup_cap() {
        assert_eq!(restored_timeline_limit(30, Some(200), 1_000), 200);
        assert_eq!(restored_timeline_limit(30, None, 1_000), 30);
        assert_eq!(restored_timeline_limit(30, Some(2_000), 1_000), 1_000);
        assert_eq!(restored_timeline_limit(30, Some(12), 1_000), 30);
    }

    #[test]
    fn cached_room_timeline_merges_older_items() {
        let store_root = unique_test_dir();
        let store_dir = store_root.join("store");
        fs::create_dir_all(&store_dir).unwrap();
        let newer_items = vec![test_timeline_item("$2"), test_timeline_item("$3")];
        let older_items = vec![test_timeline_item("$1"), test_timeline_item("$2")];

        merge_cached_room_timeline_refresh(
            ACCOUNT_KEY,
            &store_dir,
            ROOM_ID,
            &newer_items,
            Some(&timeline_page_token(1)),
            &[],
        )
        .unwrap();
        prepend_cached_room_timeline_items(
            ACCOUNT_KEY,
            &store_dir,
            ROOM_ID,
            &older_items,
            Some(&timeline_page_token(2)),
        )
        .unwrap();

        let (items, next_before) = cached_room_timeline(ACCOUNT_KEY, &store_dir, ROOM_ID)
            .unwrap()
            .unwrap();
        assert_eq!(
            items
                .iter()
                .map(|item| item.event_id.as_str())
                .collect::<Vec<&str>>(),
            vec!["$1", "$2", "$3"]
        );
        assert_eq!(next_before, Some(timeline_page_token(2)));

        remove_test_dir(&store_root).unwrap();
    }

    #[test]
    fn refreshed_room_timeline_updates_and_redacts_cached_items() {
        let store_root = unique_test_dir();
        let store_dir = store_root.join("store");
        fs::create_dir_all(&store_dir).unwrap();
        let cached_items = vec![
            test_timeline_item("$1"),
            test_timeline_item("$2"),
            test_timeline_item("$3"),
        ];
        let mut edited_item = test_timeline_item("$2");
        edited_item.body = String::from("Edited body");
        edited_item.is_edited = Some(true);
        let refreshed_items = vec![edited_item, test_timeline_item("$4")];

        merge_cached_room_timeline_refresh(
            ACCOUNT_KEY,
            &store_dir,
            ROOM_ID,
            &cached_items,
            Some(&timeline_page_token(4)),
            &[],
        )
        .unwrap();
        merge_cached_room_timeline_refresh(
            ACCOUNT_KEY,
            &store_dir,
            ROOM_ID,
            &refreshed_items,
            Some(&timeline_page_token(1)),
            &[String::from("$3")],
        )
        .unwrap();

        let (items, next_before) = cached_room_timeline(ACCOUNT_KEY, &store_dir, ROOM_ID)
            .unwrap()
            .unwrap();
        assert_eq!(
            items
                .iter()
                .map(|item| (item.event_id.as_str(), item.body.as_str(), item.is_edited))
                .collect::<Vec<(&str, &str, Option<bool>)>>(),
            vec![
                ("$1", "Body", Some(false)),
                ("$2", "Edited body", Some(true)),
                ("$4", "Body", Some(false)),
            ]
        );
        assert_eq!(next_before, Some(timeline_page_token(4)));

        remove_test_dir(&store_root).unwrap();
    }

    fn unique_test_dir() -> PathBuf {
        let counter = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let unique_suffix = format!(
            "hyperion-timeline-cache-test-{}-{timestamp}-{counter}",
            std::process::id(),
        );
        std::env::temp_dir().join(unique_suffix)
    }

    fn remove_test_dir(path: &Path) -> io::Result<()> {
        for attempt in 0..5 {
            match fs::remove_dir_all(path) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(error) if attempt == 4 => return Err(error),
                Err(_error) => thread::sleep(Duration::from_millis(50)),
            }
        }

        Ok(())
    }

    fn test_room_summary(room_id: &str, last_activity_unix_ms: u64) -> RoomThreadSummary {
        RoomThreadSummary {
            room_id: room_id.to_owned(),
            title: String::from("Room"),
            preview: String::from("Preview"),
            participant_label: String::from("Alice"),
            last_activity_unix_ms,
            last_activity_label: String::from("Now"),
            message_count: 10,
            unread_count: 1,
            homeserver_label: String::from("example.org"),
            avatar_label: Some(String::from("R")),
            is_direct: false,
        }
    }

    fn test_timeline_item(event_id: &str) -> RoomTimelineItem {
        RoomTimelineItem {
            event_id: event_id.to_owned(),
            sender_id: String::from("@alice:example.org"),
            sender_display_name: None,
            body: String::from("Body"),
            timestamp_unix_ms: 1_000,
            is_edited: Some(false),
            is_own_message: false,
        }
    }
}
