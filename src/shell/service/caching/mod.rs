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

mod cache_state;

pub(in crate::shell) use cache_state::ShellCacheState;

const HYPERION_CACHE_DIR_NAME: &str = "hyperion-cache";
const TIMELINE_VIEW_STATE_DATABASE_NAME: &str = "timeline-view.sqlite3";

/// Keeps the command's pagination contract while no longer persisting a
/// separate Hyperion view-depth snapshot.
pub(super) fn restored_timeline_limit(
    requested_limit: u16,
    _remembered_count: Option<usize>,
    _maximum_restored_count: usize,
) -> u16 {
    requested_limit
}

/// Removes the pre-v3 plaintext shell cache without touching Matrix SDK stores.
/// The SDK state, crypto and encrypted `EventCache` stores live outside this directory.
pub(in crate::shell::service) fn remove_legacy_timeline_view_cache(store_dir: &Path) {
    let Some(account_root) = store_dir.parent() else {
        return;
    };
    let database_path = account_root
        .join(HYPERION_CACHE_DIR_NAME)
        .join(TIMELINE_VIEW_STATE_DATABASE_NAME);
    let legacy_paths = [
        database_path.clone(),
        PathBuf::from(format!("{}-wal", database_path.display())),
        PathBuf::from(format!("{}-shm", database_path.display())),
    ];
    for path in legacy_paths {
        if !path.exists() {
            continue;
        }
        if let Err(error) = std::fs::remove_file(&path) {
            crate::utils::tracing::report_recoverable_error(
                "shell.cache",
                "remove_legacy_timeline_view_cache",
                "shell.cache_cleanup_failed",
                "cache",
                &error,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn removes_legacy_database_and_journals() {
        let root = std::env::temp_dir().join(format!("hyperion-cache-test-{}", std::process::id()));
        let store_dir = root.join("store");
        let cache_dir = root.join(HYPERION_CACHE_DIR_NAME);
        fs::create_dir_all(&store_dir).expect("create store directory");
        fs::create_dir_all(&cache_dir).expect("create cache directory");
        for suffix in ["", "-wal", "-shm"] {
            fs::write(
                cache_dir.join(format!("{TIMELINE_VIEW_STATE_DATABASE_NAME}{suffix}")),
                "plaintext",
            )
            .expect("write legacy cache");
        }

        remove_legacy_timeline_view_cache(&store_dir);

        assert!(!cache_dir.join(TIMELINE_VIEW_STATE_DATABASE_NAME).exists());
        assert!(
            !cache_dir
                .join(format!("{TIMELINE_VIEW_STATE_DATABASE_NAME}-wal"))
                .exists()
        );
        assert!(
            !cache_dir
                .join(format!("{TIMELINE_VIEW_STATE_DATABASE_NAME}-shm"))
                .exists()
        );
        fs::remove_dir_all(root).expect("remove test directory");
    }
}
