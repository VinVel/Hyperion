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
 * along with this program. If not, see <https://www.gnu.org/licenses/>
 *
 * Project home: hyperion.velcore.net
 */

pub(super) mod backfill;
pub(super) mod commands;
mod database;
mod errors;
pub(super) mod extract;
pub(super) mod indexer;
mod legacy;
mod ranking;
mod repository;
mod status;
#[cfg(test)]
mod testing;
pub(super) mod types;

pub(super) use backfill::SearchBackfillCoordinator;
pub(super) use indexer::SearchIndexer;
pub(super) use legacy::{
    first_visible_grapheme, matches_query, normalize_query, now_unix_ms, relative_time_label,
};
pub(super) use status::SearchStatusReporter;
