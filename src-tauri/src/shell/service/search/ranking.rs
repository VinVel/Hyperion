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

use super::types::SearchHitRow;

const TITLE_MATCH_BOOST: f64 = 6.0;
const SUBTITLE_MATCH_BOOST: f64 = 3.0;
const BODY_MATCH_BOOST: f64 = 1.0;
const PHRASE_MATCH_BOOST: f64 = 4.0;
const PREFIX_MATCH_BOOST: f64 = 2.0;
// Recency is a tie-breaker bucketed by minute so exact timestamps do not
// overwhelm text relevance.
const RECENCY_BUCKET_SCORE_DIVISOR: f64 = 1_000_000.0;

pub(super) fn fts_query_for_user_query(query: &str) -> Option<String> {
    let terms = normalized_terms(query);
    if terms.is_empty() {
        return None;
    }

    Some(
        terms
            .into_iter()
            .map(|term| format!("{term}*"))
            .collect::<Vec<_>>()
            .join(" AND "),
    )
}

pub(super) fn normalized_query_text(query: &str) -> String {
    normalized_terms(query).join(" ")
}

pub(super) fn fields_match_user_query<'field>(
    fields: impl IntoIterator<Item = &'field str>,
    query: &str,
) -> bool {
    let field_terms = fields
        .into_iter()
        .flat_map(normalized_terms)
        .collect::<Vec<_>>();
    let query_terms = normalized_terms(query);

    !query_terms.is_empty()
        && query_terms.iter().all(|query_term| {
            field_terms
                .iter()
                .any(|field_term| field_term.starts_with(query_term))
        })
}

pub(super) fn body_matches_user_query(body: &str, query: &str) -> bool {
    fields_match_user_query([body], query)
}

pub(super) fn score_hit(hit: &SearchHitRow, normalized_query: &str) -> f64 {
    let mut score = -hit.fts_rank;
    let query = normalized_query.to_lowercase();
    let title = hit.title.to_lowercase();
    let subtitle = hit.subtitle.to_lowercase();
    let body = hit.body.to_lowercase();

    if title.contains(&query) {
        score += TITLE_MATCH_BOOST + PHRASE_MATCH_BOOST;
    }
    if subtitle.contains(&query) {
        score += SUBTITLE_MATCH_BOOST + PHRASE_MATCH_BOOST;
    }
    if body.contains(&query) {
        score += BODY_MATCH_BOOST + PHRASE_MATCH_BOOST;
    }
    if title.starts_with(&query) {
        score += PREFIX_MATCH_BOOST;
    }

    let recency_timestamp = hit.sort_timestamp_unix_ms.max(hit.timestamp_unix_ms);
    let recency_minute_bucket = u32::try_from(recency_timestamp / 60_000).unwrap_or(u32::MAX);
    score + f64::from(recency_minute_bucket) / RECENCY_BUCKET_SCORE_DIVISOR
}

fn normalized_terms(query: &str) -> Vec<String> {
    query
        .split(|character: char| !character.is_alphanumeric())
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .map(str::to_lowercase)
        .collect()
}
