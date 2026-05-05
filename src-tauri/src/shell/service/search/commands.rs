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

use crate::shell::types::{
    GlobalSearchMessageHit, GlobalSearchResponse, GlobalSearchRoomHit, GlobalSearchSpaceHit,
};

use super::{
    database::{open_search_connection_with_recovery, search_paths_for_store},
    ranking,
    repository::SearchRepository,
    types::{GroupedSearchHits, SearchEntityType, SearchHitRow},
};

// Search groups back the current shell UI; keeping them short avoids turning a
// lightweight command into a broad result surface on each keystroke.
pub(in crate::shell::service) const DEFAULT_SEARCH_LIMIT_PER_GROUP: usize = 5;
const SEARCH_CANDIDATE_MULTIPLIER: usize = 8;

pub(in crate::shell::service) fn global_search(
    account_key: &str,
    store_dir: &Path,
    query: &str,
    limit_per_group: usize,
) -> Result<GlobalSearchResponse, String> {
    let paths = search_paths_for_store(store_dir).map_err(|error| error.to_string())?;
    let search_connection =
        open_search_connection_with_recovery(&paths).map_err(|error| error.to_string())?;
    let repository = SearchRepository::new(&search_connection.connection);
    let candidate_limit = limit_per_group.saturating_mul(SEARCH_CANDIDATE_MULTIPLIER);
    let hits = repository
        .search_documents(account_key, query, candidate_limit.max(limit_per_group))
        .map_err(|error| error.to_string())?;
    let normalized_query = ranking::normalized_query_text(query);
    let grouped_hits = group_ranked_hits(hits, &normalized_query, limit_per_group);
    let mut status = repository
        .status(account_key)
        .map_err(|error| error.to_string())?;
    if search_connection.recovered {
        status.state = crate::shell::types::GlobalSearchIndexState::Degraded;
        status.notice = Some(String::from("Search index is being rebuilt."));
    }

    Ok(GlobalSearchResponse {
        rooms: grouped_hits.rooms,
        spaces: grouped_hits.spaces,
        messages: grouped_hits.messages,
        status,
    })
}

fn group_ranked_hits(
    mut hits: Vec<SearchHitRow>,
    normalized_query: &str,
    limit_per_group: usize,
) -> GroupedSearchHits {
    hits.sort_by(|left, right| {
        let right_score = ranking::score_hit(right, normalized_query);
        let left_score = ranking::score_hit(left, normalized_query);
        right_score
            .partial_cmp(&left_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut rooms = Vec::new();
    let mut spaces = Vec::new();
    let mut messages = Vec::new();

    for hit in hits {
        match hit.entity_type {
            SearchEntityType::Room if rooms.len() < limit_per_group => {
                if let Some(room_id) = hit.room_id {
                    rooms.push(GlobalSearchRoomHit {
                        room_id,
                        title: hit.title,
                        description: best_description(&hit.subtitle, &hit.body),
                    });
                }
            }
            SearchEntityType::Space if spaces.len() < limit_per_group => {
                if let Some(space_id) = hit.space_id {
                    spaces.push(GlobalSearchSpaceHit {
                        space_id,
                        title: hit.title,
                        description: best_description(&hit.subtitle, &hit.body),
                    });
                }
            }
            SearchEntityType::Message if messages.len() < limit_per_group => {
                if let Some(room_id) = hit.room_id {
                    messages.push(GlobalSearchMessageHit {
                        result_id: hit.document_id,
                        room_id,
                        title: hit.title,
                        description: hit.body,
                        event_id: hit.event_id,
                    });
                }
            }
            _ => {}
        }

        if rooms.len() >= limit_per_group
            && spaces.len() >= limit_per_group
            && messages.len() >= limit_per_group
        {
            break;
        }
    }

    GroupedSearchHits {
        rooms,
        spaces,
        messages,
    }
}

fn best_description(subtitle: &str, body: &str) -> String {
    if !subtitle.trim().is_empty() {
        return subtitle.to_owned();
    }

    body.to_owned()
}
