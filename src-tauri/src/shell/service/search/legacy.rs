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

use std::time::{SystemTime, UNIX_EPOCH};

pub(in crate::shell::service) fn normalize_query(query: Option<&str>) -> Option<String> {
    query
        .map(str::trim)
        .filter(|query| !query.is_empty())
        .map(str::to_lowercase)
}

pub(in crate::shell::service) fn matches_query(query: Option<&str>, haystacks: &[&str]) -> bool {
    let Some(query) = query else {
        return true;
    };

    haystacks
        .iter()
        .any(|haystack| haystack.to_lowercase().contains(query))
}

pub(in crate::shell::service) fn first_visible_grapheme(value: &str) -> Option<String> {
    value
        .trim()
        .chars()
        .next()
        .map(|ch| ch.to_uppercase().collect())
}

// Shell timestamps only need a coarse "now" anchor for relative labels,
// search updates, and warmup throttling.
pub(in crate::shell::service) fn now_unix_ms() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

pub(in crate::shell::service) fn relative_time_label(timestamp_unix_ms: u64) -> String {
    if timestamp_unix_ms == 0 {
        return String::from("No recent activity");
    }

    let now = now_unix_ms();
    let delta_ms = now.saturating_sub(timestamp_unix_ms);
    let delta_minutes = delta_ms / 60_000;
    let delta_hours = delta_ms / 3_600_000;
    let delta_days = delta_ms / 86_400_000;

    if delta_minutes < 1 {
        String::from("Just now")
    } else if delta_minutes < 60 {
        format!(
            "{delta_minutes} minute{} ago",
            if delta_minutes == 1 { "" } else { "s" }
        )
    } else if delta_hours < 24 {
        format!(
            "{delta_hours} hour{} ago",
            if delta_hours == 1 { "" } else { "s" }
        )
    } else if delta_days == 1 {
        String::from("Yesterday")
    } else {
        format!("{delta_days} days ago")
    }
}
