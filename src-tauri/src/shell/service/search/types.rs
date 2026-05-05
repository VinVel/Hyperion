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

use std::{fmt, path::PathBuf};

use rusqlite::Connection;

use super::super::super::types::{
    GlobalSearchMessageHit, GlobalSearchRoomHit, GlobalSearchSpaceHit,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SearchEntityType {
    Room,
    Space,
    Message,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::shell::service) enum SearchBackfillState {
    NotStarted,
    Queued,
    Indexing,
    PausedPower,
    PausedNetwork,
    RateLimited,
    Complete,
    Error,
}

#[allow(dead_code)]
impl SearchBackfillState {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::Queued => "queued",
            Self::Indexing => "indexing",
            Self::PausedPower => "paused_power",
            Self::PausedNetwork => "paused_network",
            Self::RateLimited => "rate_limited",
            Self::Complete => "complete",
            Self::Error => "error",
        }
    }

    pub(super) fn from_str(value: &str) -> Option<Self> {
        match value {
            "not_started" => Some(Self::NotStarted),
            "queued" => Some(Self::Queued),
            "indexing" => Some(Self::Indexing),
            "paused_power" => Some(Self::PausedPower),
            "paused_network" => Some(Self::PausedNetwork),
            "rate_limited" => Some(Self::RateLimited),
            "complete" => Some(Self::Complete),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

impl SearchEntityType {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Room => "room",
            Self::Space => "space",
            Self::Message => "message",
        }
    }

    pub(super) fn from_str(value: &str) -> Option<Self> {
        match value {
            "room" => Some(Self::Room),
            "space" => Some(Self::Space),
            "message" => Some(Self::Message),
            _ => None,
        }
    }
}

impl fmt::Display for SearchEntityType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub(super) struct SearchDocument {
    pub(super) account_key: String,
    pub(super) document_id: String,
    pub(super) entity_type: SearchEntityType,
    pub(super) room_id: Option<String>,
    pub(super) space_id: Option<String>,
    pub(super) event_id: Option<String>,
    pub(super) sender_id: Option<String>,
    pub(super) user_id: Option<String>,
    pub(super) title: String,
    pub(super) subtitle: String,
    pub(super) body: String,
    pub(super) timestamp_unix_ms: u64,
    pub(super) sort_timestamp_unix_ms: u64,
    pub(super) is_deleted: bool,
    pub(super) updated_at_unix_ms: u64,
}

#[derive(Debug, Clone)]
pub(super) struct SearchPaths {
    pub(super) database_path: PathBuf,
}

pub(super) struct SearchConnection {
    pub(super) connection: Connection,
    pub(super) recovered: bool,
}

#[derive(Debug, Clone)]
pub(super) struct SearchHitRow {
    pub(super) document_id: String,
    pub(super) entity_type: SearchEntityType,
    pub(super) room_id: Option<String>,
    pub(super) space_id: Option<String>,
    pub(super) event_id: Option<String>,
    pub(super) title: String,
    pub(super) subtitle: String,
    pub(super) body: String,
    pub(super) timestamp_unix_ms: u64,
    pub(super) sort_timestamp_unix_ms: u64,
    pub(super) fts_rank: f64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::shell::service) struct SearchRoomBackfillProgress {
    pub(in crate::shell::service) account_key: String,
    pub(in crate::shell::service) room_id: String,
    pub(in crate::shell::service) room_kind: String,
    pub(in crate::shell::service) backfill_token: Option<String>,
    pub(in crate::shell::service) backfill_state: SearchBackfillState,
    pub(in crate::shell::service) indexed_event_count: u64,
    pub(in crate::shell::service) last_indexed_at_unix_ms: Option<u64>,
    pub(in crate::shell::service) last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct GroupedSearchHits {
    pub(super) rooms: Vec<GlobalSearchRoomHit>,
    pub(super) spaces: Vec<GlobalSearchSpaceHit>,
    pub(super) messages: Vec<GlobalSearchMessageHit>,
}
