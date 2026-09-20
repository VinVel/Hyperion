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
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::Connection;

use super::{
    commands,
    database::{initialize_connection_schema, open_search_connection_with_recovery},
    extract::{message_document_id, room_document_id},
    ranking::{fts_query_for_user_query, score_hit},
    repository::SearchRepository,
    types::{
        SearchBackfillState, SearchDocument, SearchEntityType, SearchHitRow, SearchPaths,
        SearchRoomBackfillProgress,
    },
};

const ACCOUNT_KEY: &str = "@alice:example.org";
const ROOM_ID: &str = "!room:example.org";
const SPACE_ID: &str = "!space:example.org";
const EVENT_ID: &str = "$event";
static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

#[test]
fn document_ids_are_deterministic() {
    assert_eq!(
        room_document_id("@a:example.org", "!room:example.org"),
        "room::@a:example.org::!room:example.org"
    );
    assert_eq!(
        message_document_id("@a:example.org", "!room:example.org", "$event"),
        "message::@a:example.org::!room:example.org::$event"
    );
}

#[test]
fn fts_query_uses_prefix_terms() {
    assert_eq!(
        fts_query_for_user_query("Hello, Matrix").as_deref(),
        Some("hello* AND matrix*")
    );
}

#[test]
fn message_body_matching_uses_all_prefix_terms() {
    assert!(super::ranking::body_matches_user_query(
        "hello matrix room",
        "hello mat",
    ));
    assert!(!super::ranking::body_matches_user_query(
        "hello matrix room",
        "hello missing",
    ));
    assert!(super::ranking::body_matches_user_query(
        "i find that llms tend to know/understand it fine",
        "i find that llms tend to know/u",
    ));
}

#[test]
fn title_match_scores_above_body_match() {
    let mut title_hit = test_hit("Matrix", "", "other");
    let body_hit = test_hit("other", "", "Matrix");
    title_hit.fts_rank = body_hit.fts_rank;

    assert!(score_hit(&title_hit, "matrix") > score_hit(&body_hit, "matrix"));
}

#[test]
fn searches_room_space_and_message_documents() {
    let connection = test_connection();
    let repository = SearchRepository::new(&connection);

    repository.upsert_document(&room_document()).unwrap();
    repository.upsert_document(&space_document()).unwrap();
    repository
        .upsert_document(&message_document("hello matrix"))
        .unwrap();

    let room_hits = repository
        .search_documents(ACCOUNT_KEY, "alice", 10)
        .unwrap();
    assert_eq!(room_hits.len(), 1);
    assert_eq!(room_hits[0].entity_type, SearchEntityType::Room);

    let space_hits = repository
        .search_documents(ACCOUNT_KEY, "coordination", 10)
        .unwrap();
    assert_eq!(space_hits.len(), 1);
    assert_eq!(space_hits[0].entity_type, SearchEntityType::Space);

    let message_hits = repository
        .search_documents(ACCOUNT_KEY, "hello", 10)
        .unwrap();
    assert_eq!(message_hits.len(), 1);
    assert_eq!(message_hits[0].entity_type, SearchEntityType::Message);

    let room_title_hits = repository
        .search_documents(ACCOUNT_KEY, "project", 10)
        .unwrap();
    assert!(
        room_title_hits
            .iter()
            .all(|hit| hit.entity_type != SearchEntityType::Message)
    );

    let message_body_hits = repository
        .search_documents(ACCOUNT_KEY, "hello", 10)
        .unwrap();
    assert!(
        message_body_hits
            .iter()
            .all(|hit| hit.entity_type != SearchEntityType::Room)
    );
}

#[test]
fn delete_document_removes_fts_hit_and_records_tombstone() {
    let connection = test_connection();
    let repository = SearchRepository::new(&connection);
    let document = message_document("remove this message");

    repository.upsert_document(&document).unwrap();
    assert_eq!(
        repository
            .search_documents(ACCOUNT_KEY, "remove", 10)
            .unwrap()
            .len(),
        1
    );

    repository
        .delete_document(
            ACCOUNT_KEY,
            &document.document_id,
            SearchEntityType::Message,
            2_000,
        )
        .unwrap();
    repository.upsert_document(&document).unwrap();

    assert!(
        repository
            .search_documents(ACCOUNT_KEY, "remove", 10)
            .unwrap()
            .is_empty()
    );
    let tombstone_count: u64 = connection
        .query_row("SELECT COUNT(*) FROM search_tombstones", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(tombstone_count, 1);
}

#[test]
fn upsert_replaces_message_body_for_edits() {
    let connection = test_connection();
    let repository = SearchRepository::new(&connection);
    let original = message_document("old body");
    let mut edited = original.clone();
    edited.body = String::from("new body");
    edited.updated_at_unix_ms = 2_000;

    repository.upsert_document(&original).unwrap();
    repository.upsert_document(&edited).unwrap();

    assert!(
        repository
            .search_documents(ACCOUNT_KEY, "old", 10)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repository
            .search_documents(ACCOUNT_KEY, "new", 10)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn search_limit_is_applied() {
    let connection = test_connection();
    let repository = SearchRepository::new(&connection);

    for index in 0..3 {
        let mut document = message_document("repeatable body");
        document.document_id = format!("message::{ACCOUNT_KEY}::{ROOM_ID}::${index}");
        document.event_id = Some(format!("${index}"));
        repository.upsert_document(&document).unwrap();
    }

    assert_eq!(
        repository
            .search_documents(ACCOUNT_KEY, "repeatable", 2)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn status_counts_indexed_documents() {
    let connection = test_connection();
    let repository = SearchRepository::new(&connection);

    repository.upsert_document(&room_document()).unwrap();
    repository
        .upsert_document(&message_document("body"))
        .unwrap();

    let status = repository.status(ACCOUNT_KEY).unwrap();
    assert_eq!(status.indexed_room_count, 1);
    assert_eq!(status.total_room_count, 1);
    assert_eq!(status.message_count, 1);
    assert_eq!(status.last_indexed_at_unix_ms, Some(1_000));
}

#[test]
fn stale_room_documents_are_tombstoned() {
    let connection = test_connection();
    let repository = SearchRepository::new(&connection);
    let active_room = room_document();
    let mut stale_room = room_document();
    stale_room.room_id = Some(String::from("!stale:example.org"));
    stale_room.document_id = format!("room::{ACCOUNT_KEY}::!stale:example.org");
    stale_room.title = String::from("Stale Room");

    repository.upsert_document(&active_room).unwrap();
    repository.upsert_document(&stale_room).unwrap();

    repository
        .tombstone_stale_entity_documents(
            ACCOUNT_KEY,
            SearchEntityType::Room,
            &HashSet::from([ROOM_ID.to_owned()]),
            2_000,
        )
        .unwrap();

    assert!(
        repository
            .search_documents(ACCOUNT_KEY, "stale", 10)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repository
            .search_documents(ACCOUNT_KEY, "alice", 10)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn backfill_progress_persists_and_resumes() {
    let connection = test_connection();
    let repository = SearchRepository::new(&connection);
    let progress = SearchRoomBackfillProgress {
        account_key: ACCOUNT_KEY.to_owned(),
        room_id: ROOM_ID.to_owned(),
        room_kind: String::from("conversation"),
        backfill_token: Some(String::from("prev-token")),
        backfill_state: SearchBackfillState::Queued,
        indexed_event_count: 42,
        last_indexed_at_unix_ms: Some(2_000),
        last_error: None,
    };

    repository.update_room_backfill_progress(&progress).unwrap();

    let persisted = repository
        .room_backfill_progress(ACCOUNT_KEY, ROOM_ID)
        .unwrap()
        .unwrap();
    assert_eq!(persisted, progress);
}

#[test]
fn backfill_constants_remain_conservative() {
    assert_eq!(super::backfill::BACKFILL_PAGE_SIZE, 20);
    assert_eq!(super::backfill::BACKFILL_CONCURRENT_ROOM_LIMIT, 1);
    assert_eq!(super::backfill::BACKFILL_MAX_ROOM_BATCHES_PER_TICK, 2);
    assert_eq!(super::backfill::BACKFILL_SESSION_EVENT_BUDGET, 1_000);
}

#[test]
fn corrupt_database_is_recreated_and_marked_recovered() {
    let database_dir = unique_test_dir();
    fs::create_dir_all(&database_dir).unwrap();
    let database_path = database_dir.join("search.sqlite3");
    fs::write(&database_path, b"not sqlite").unwrap();

    let paths = SearchPaths { database_path };
    let search_connection = open_search_connection_with_recovery(&paths).unwrap();

    assert!(search_connection.recovered);
    {
        let repository = SearchRepository::new(&search_connection.connection);
        repository.upsert_document(&room_document()).unwrap();
        assert_eq!(
            repository
                .search_documents(ACCOUNT_KEY, "alice", 10)
                .unwrap()
                .len(),
            1
        );
    }

    drop(search_connection);
    remove_test_dir(&database_dir).unwrap();
}

#[test]
fn command_global_search_returns_grouped_results_and_status() {
    let store_root = unique_test_dir();
    let store_dir = store_root.join("store");
    fs::create_dir_all(&store_dir).unwrap();
    let paths = super::database::search_paths_for_store(&store_dir).unwrap();
    let search_connection = open_search_connection_with_recovery(&paths).unwrap();
    {
        let repository = SearchRepository::new(&search_connection.connection);
        repository.upsert_document(&room_document()).unwrap();
        repository.upsert_document(&space_document()).unwrap();
        repository
            .upsert_document(&message_document("shared command query"))
            .unwrap();
    }
    drop(search_connection);

    let response = commands::global_search(ACCOUNT_KEY, &store_dir, "query", 2).unwrap();

    assert!(response.rooms.is_empty());
    assert!(response.spaces.is_empty());
    assert_eq!(response.messages.len(), 1);
    assert_eq!(response.status.message_count, 1);

    remove_test_dir(&store_root).unwrap();
}

#[test]
fn command_global_search_applies_per_group_limits() {
    let store_root = unique_test_dir();
    let store_dir = store_root.join("store");
    fs::create_dir_all(&store_dir).unwrap();
    let paths = super::database::search_paths_for_store(&store_dir).unwrap();
    let search_connection = open_search_connection_with_recovery(&paths).unwrap();
    {
        let repository = SearchRepository::new(&search_connection.connection);
        for index in 0..3 {
            let mut document = message_document("limited command body");
            document.document_id = format!("message::{ACCOUNT_KEY}::{ROOM_ID}::$cmd{index}");
            document.event_id = Some(format!("$cmd{index}"));
            repository.upsert_document(&document).unwrap();
        }
    }
    drop(search_connection);

    let response = commands::global_search(ACCOUNT_KEY, &store_dir, "limited", 2).unwrap();

    assert_eq!(response.messages.len(), 2);

    remove_test_dir(&store_root).unwrap();
}

fn test_connection() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    initialize_connection_schema(&connection).unwrap();
    connection
}

fn unique_test_dir() -> PathBuf {
    let counter = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let unique_suffix = format!(
        "hyperion-search-test-{}-{timestamp}-{counter}",
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

fn room_document() -> SearchDocument {
    SearchDocument {
        account_key: ACCOUNT_KEY.to_owned(),
        document_id: format!("room::{ACCOUNT_KEY}::{ROOM_ID}"),
        entity_type: SearchEntityType::Room,
        room_id: Some(ROOM_ID.to_owned()),
        space_id: None,
        event_id: None,
        sender_id: None,
        user_id: None,
        title: String::from("Project Room"),
        subtitle: String::from("Alice and Bob"),
        body: String::from("discussion"),
        timestamp_unix_ms: 1_000,
        sort_timestamp_unix_ms: 1_000,
        is_deleted: false,
        updated_at_unix_ms: 1_000,
    }
}

fn space_document() -> SearchDocument {
    SearchDocument {
        account_key: ACCOUNT_KEY.to_owned(),
        document_id: format!("space::{ACCOUNT_KEY}::{SPACE_ID}"),
        entity_type: SearchEntityType::Space,
        room_id: None,
        space_id: Some(SPACE_ID.to_owned()),
        event_id: None,
        sender_id: None,
        user_id: None,
        title: String::from("Coordination Space"),
        subtitle: String::from("2 members"),
        body: String::from("planning"),
        timestamp_unix_ms: 1_000,
        sort_timestamp_unix_ms: 1_000,
        is_deleted: false,
        updated_at_unix_ms: 1_000,
    }
}

fn message_document(body: &str) -> SearchDocument {
    SearchDocument {
        account_key: ACCOUNT_KEY.to_owned(),
        document_id: format!("message::{ACCOUNT_KEY}::{ROOM_ID}::{EVENT_ID}"),
        entity_type: SearchEntityType::Message,
        room_id: Some(ROOM_ID.to_owned()),
        space_id: None,
        event_id: Some(EVENT_ID.to_owned()),
        sender_id: Some(String::from("@bob:example.org")),
        user_id: None,
        title: String::from("Message in Project Room"),
        subtitle: String::from("Bob"),
        body: body.to_owned(),
        timestamp_unix_ms: 1_000,
        sort_timestamp_unix_ms: 1_000,
        is_deleted: false,
        updated_at_unix_ms: 1_000,
    }
}

fn test_hit(title: &str, subtitle: &str, body: &str) -> SearchHitRow {
    SearchHitRow {
        document_id: String::from("doc"),
        entity_type: SearchEntityType::Room,
        room_id: None,
        space_id: None,
        event_id: None,
        title: title.to_owned(),
        subtitle: subtitle.to_owned(),
        body: body.to_owned(),
        timestamp_unix_ms: 0,
        sort_timestamp_unix_ms: 0,
        fts_rank: 0.0,
    }
}
