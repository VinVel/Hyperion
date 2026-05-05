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

use super::{
    database::{open_search_connection_with_recovery, search_paths_for_store},
    repository::SearchRepository,
};
use crate::shell::types::{GlobalSearchIndexState, GlobalSearchIndexStatus};

#[derive(Clone, Default)]
pub(in crate::shell::service) struct SearchStatusReporter;

impl SearchStatusReporter {
    pub(in crate::shell::service) fn status_for_account(
        account_key: &str,
        store_dir: &Path,
    ) -> GlobalSearchIndexStatus {
        let Ok(paths) = search_paths_for_store(store_dir) else {
            return degraded_status("Search storage is unavailable.");
        };
        let Ok(search_connection) = open_search_connection_with_recovery(&paths) else {
            return degraded_status("Search index is being rebuilt.");
        };
        let repository = SearchRepository::new(&search_connection.connection);
        let mut status = repository
            .status(account_key)
            .unwrap_or_else(|_| degraded_status("Search index is being rebuilt."));
        if search_connection.recovered {
            status.state = GlobalSearchIndexState::Degraded;
            status.notice = Some(String::from("Search index is being rebuilt."));
        }
        status
    }
}

fn degraded_status(notice: &str) -> GlobalSearchIndexStatus {
    GlobalSearchIndexStatus {
        state: GlobalSearchIndexState::Degraded,
        notice: Some(notice.to_owned()),
        ..GlobalSearchIndexStatus::default()
    }
}
