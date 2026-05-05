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

use std::{error::Error, fmt, io};

#[derive(Debug)]
pub(super) enum SearchError {
    Io(io::Error),
    Sqlite(rusqlite::Error),
    CorruptDatabase(String),
}

impl fmt::Display for SearchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "Search storage I/O failed: {error}"),
            Self::Sqlite(error) => write!(formatter, "Search database failed: {error}"),
            Self::CorruptDatabase(message) => {
                write!(formatter, "Search database is corrupt: {message}")
            }
        }
    }
}

impl Error for SearchError {}

impl From<io::Error> for SearchError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rusqlite::Error> for SearchError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

pub(super) type SearchResult<T> = Result<T, SearchError>;
