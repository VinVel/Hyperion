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

use std::collections::{HashMap, HashSet};

use crate::shell::types::{RoomTimelineItem, RoomTimelineSendState, apply_timeline_presentation};

// Matrix SDK local echo and remote echo timestamps can differ slightly while
// the send queue reconciles transaction IDs, so body-based fallback matching is bounded.
const LOCAL_ECHO_RECONCILIATION_WINDOW_UNIX_MS: u64 = 120_000;

pub(in crate::shell) fn reconcile_authoritative_timeline_items(
    items: Vec<RoomTimelineItem>,
    room_id: &str,
    redacted_event_ids: &[String],
) -> Vec<RoomTimelineItem> {
    let redacted_event_ids = redacted_event_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<&str>>();
    let confirmed_remote_items = items
        .iter()
        .filter(|item| is_confirmed_own_remote_event(item))
        .cloned()
        .collect::<Vec<RoomTimelineItem>>();
    let latest_items_by_identity = latest_items_by_identity(&items);
    let mut seen_identities = HashSet::<String>::new();
    let mut reconciled_items = Vec::<RoomTimelineItem>::new();

    for item in items {
        if redacted_event_ids.contains(item.event_id()) {
            continue;
        }

        if is_transient_local_echo(&item)
            && confirmed_remote_items
                .iter()
                .any(|confirmed_item| timeline_items_represent_same_send(&item, confirmed_item))
        {
            continue;
        }

        let identity = timeline_item_identity(&item);
        if !seen_identities.insert(identity.clone()) {
            continue;
        }

        let item = latest_items_by_identity
            .get(&identity)
            .map_or(item, |latest_item| (*latest_item).clone());
        reconciled_items.push(item);
    }

    apply_timeline_presentation(&mut reconciled_items, room_id);
    reconciled_items
}

pub(in crate::shell) fn merge_cached_timeline_with_authoritative_refresh(
    cached_items: &[RoomTimelineItem],
    refreshed_items: &[RoomTimelineItem],
    room_id: &str,
    redacted_event_ids: &[String],
) -> Vec<RoomTimelineItem> {
    let refreshed_items = reconcile_authoritative_timeline_items(
        refreshed_items.to_vec(),
        room_id,
        redacted_event_ids,
    );
    if cached_items.is_empty() {
        return refreshed_items;
    }

    let refreshed_event_ids = refreshed_items
        .iter()
        .map(RoomTimelineItem::event_id)
        .collect::<HashSet<&str>>();
    let redacted_event_id_set = redacted_event_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<&str>>();
    let first_overlap_index = cached_items
        .iter()
        .position(|item| refreshed_event_ids.contains(item.event_id()));

    let Some(first_overlap_index) = first_overlap_index else {
        // No shared event means the cache cannot prove how its order relates to
        // the live canonical window. Rebuild from the authoritative refresh.
        return refreshed_items;
    };

    let older_cached_prefix = cached_items[..first_overlap_index]
        .iter()
        .filter(|item| {
            !redacted_event_id_set.contains(item.event_id())
                && !refreshed_event_ids.contains(item.event_id())
        })
        .cloned();
    let mut merged_items = older_cached_prefix.collect::<Vec<RoomTimelineItem>>();
    merged_items.extend(refreshed_items);
    reconcile_authoritative_timeline_items(merged_items, room_id, redacted_event_ids)
}

pub(in crate::shell) fn prepend_authoritative_timeline_page(
    current_items: &[RoomTimelineItem],
    older_items: &[RoomTimelineItem],
    room_id: &str,
) -> Vec<RoomTimelineItem> {
    let current_event_ids = current_items
        .iter()
        .map(RoomTimelineItem::event_id)
        .collect::<HashSet<&str>>();
    let mut merged_items = older_items
        .iter()
        .filter(|item| !current_event_ids.contains(item.event_id()))
        .cloned()
        .collect::<Vec<RoomTimelineItem>>();
    merged_items.extend(current_items.iter().cloned());
    reconcile_authoritative_timeline_items(merged_items, room_id, &[])
}

fn latest_items_by_identity(items: &[RoomTimelineItem]) -> HashMap<String, RoomTimelineItem> {
    let mut latest_items = HashMap::<String, RoomTimelineItem>::new();
    for item in items {
        latest_items.insert(timeline_item_identity(item), item.clone());
    }
    latest_items
}

fn timeline_item_identity(item: &RoomTimelineItem) -> String {
    if is_remote_event_id(item.event_id()) {
        return item.event_id().to_owned();
    }

    item.matrix
        .transaction_id
        .clone()
        .unwrap_or_else(|| item.event_id().to_owned())
}

fn is_transient_local_echo(item: &RoomTimelineItem) -> bool {
    item.is_own_message() && !is_remote_event_id(item.event_id())
}

fn is_confirmed_own_remote_event(item: &RoomTimelineItem) -> bool {
    item.is_own_message()
        && is_remote_event_id(item.event_id())
        && item.matrix.send_state == RoomTimelineSendState::Sent
}

fn timeline_items_represent_same_send(
    local_echo: &RoomTimelineItem,
    confirmed_item: &RoomTimelineItem,
) -> bool {
    if !is_transient_local_echo(local_echo) || !is_confirmed_own_remote_event(confirmed_item) {
        return false;
    }

    if let (Some(local_transaction_id), Some(confirmed_transaction_id)) = (
        local_echo.matrix.transaction_id.as_deref(),
        confirmed_item.matrix.transaction_id.as_deref(),
    ) && local_transaction_id == confirmed_transaction_id
    {
        return true;
    }

    let timestamp_delta = local_echo
        .timestamp_unix_ms()
        .abs_diff(confirmed_item.timestamp_unix_ms());
    local_echo.sender_id() == confirmed_item.sender_id()
        && local_echo.body() == confirmed_item.body()
        && timestamp_delta <= LOCAL_ECHO_RECONCILIATION_WINDOW_UNIX_MS
}

fn is_remote_event_id(event_id: &str) -> bool {
    event_id.starts_with('$')
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOM_ID: &str = "!room:example.org";

    #[test]
    fn reply_target_stays_before_reply_when_authoritative_order_knows_both() {
        let mut reply = test_timeline_item("$reply", 2);
        reply.presentation.reply_preview = Some(crate::shell::types::RoomTimelineReplyPreview {
            event_id: String::from("$target"),
            state: crate::shell::types::RoomTimelineReplyPreviewState::Resolved,
            sender_id: Some(String::from("@alice:example.org")),
            sender_display_name: Some(String::from("Alice")),
            body: Some(String::from("target")),
            is_redacted: false,
        });

        let items = reconcile_authoritative_timeline_items(
            vec![test_timeline_item("$target", 1), reply],
            ROOM_ID,
            &[],
        );

        assert_eq!(event_ids(&items), vec!["$target", "$reply"]);
    }

    #[test]
    fn local_echo_is_replaced_by_remote_echo_with_matching_transaction_id() {
        let mut local_echo = test_timeline_item("txn-1", 1);
        local_echo.matrix.is_own_message = true;
        local_echo.matrix.transaction_id = Some(String::from("txn-1"));
        local_echo.matrix.send_state = RoomTimelineSendState::Sending;
        let mut remote_echo = test_timeline_item("$remote", 2);
        remote_echo.matrix.is_own_message = true;
        remote_echo.matrix.transaction_id = Some(String::from("txn-1"));

        let items =
            reconcile_authoritative_timeline_items(vec![local_echo, remote_echo], ROOM_ID, &[]);

        assert_eq!(event_ids(&items), vec!["$remote"]);
    }

    #[test]
    fn duplicate_local_and_remote_echo_does_not_render_twice() {
        let mut local_echo = test_timeline_item("txn-1", 1);
        local_echo.matrix.is_own_message = true;
        local_echo.matrix.transaction_id = Some(String::from("txn-1"));
        local_echo.matrix.send_state = RoomTimelineSendState::Sending;
        let mut remote_echo = test_timeline_item("$remote", 1);
        remote_echo.matrix.is_own_message = true;
        remote_echo.matrix.transaction_id = Some(String::from("txn-1"));

        let items = reconcile_authoritative_timeline_items(
            vec![local_echo, remote_echo.clone(), remote_echo],
            ROOM_ID,
            &[],
        );

        assert_eq!(event_ids(&items), vec!["$remote"]);
    }

    #[test]
    fn redaction_removes_existing_projected_event() {
        let items = reconcile_authoritative_timeline_items(
            vec![test_timeline_item("$a", 1), test_timeline_item("$b", 2)],
            ROOM_ID,
            &[String::from("$a")],
        );

        assert_eq!(event_ids(&items), vec!["$b"]);
    }

    #[test]
    fn edit_replacement_updates_original_event_projection_without_reordering() {
        let original = test_timeline_item("$a", 1);
        let mut edited = test_timeline_item("$a", 1);
        edited.set_body(String::from("edited"));
        edited.set_edited(true);

        let items = reconcile_authoritative_timeline_items(
            vec![original, test_timeline_item("$b", 2), edited],
            ROOM_ID,
            &[],
        );

        assert_eq!(event_ids(&items), vec!["$a", "$b"]);
        assert_eq!(items[0].body(), "edited");
        assert!(items[0].is_edited());
    }

    #[test]
    fn pagination_prepends_older_events_before_visible_window() {
        let current_items = vec![test_timeline_item("$3", 3), test_timeline_item("$4", 4)];
        let older_items = vec![test_timeline_item("$1", 1), test_timeline_item("$2", 2)];

        let items = prepend_authoritative_timeline_page(&current_items, &older_items, ROOM_ID);

        assert_eq!(event_ids(&items), vec!["$1", "$2", "$3", "$4"]);
    }

    #[test]
    fn stale_cache_order_is_repaired_by_overlapping_authoritative_refresh() {
        let cached_items = vec![
            test_timeline_item("$reply", 2),
            test_timeline_item("$target", 1),
        ];
        let refreshed_items = vec![
            test_timeline_item("$target", 1),
            test_timeline_item("$reply", 2),
        ];

        let items = merge_cached_timeline_with_authoritative_refresh(
            &cached_items,
            &refreshed_items,
            ROOM_ID,
            &[],
        );

        assert_eq!(event_ids(&items), vec!["$target", "$reply"]);
    }

    #[test]
    fn stale_cache_without_overlap_is_rebuilt_from_authoritative_refresh() {
        let cached_items = vec![
            test_timeline_item("$stale-a", 1),
            test_timeline_item("$stale-b", 2),
        ];
        let refreshed_items = vec![
            test_timeline_item("$live-a", 3),
            test_timeline_item("$live-b", 4),
        ];

        let items = merge_cached_timeline_with_authoritative_refresh(
            &cached_items,
            &refreshed_items,
            ROOM_ID,
            &[],
        );

        assert_eq!(event_ids(&items), vec!["$live-a", "$live-b"]);
    }

    #[test]
    fn live_refresh_reorders_overlapping_cached_window() {
        let cached_items = vec![
            test_timeline_item("$older", 0),
            test_timeline_item("$b", 2),
            test_timeline_item("$a", 1),
        ];
        let refreshed_items = vec![test_timeline_item("$a", 1), test_timeline_item("$b", 2)];

        let items = merge_cached_timeline_with_authoritative_refresh(
            &cached_items,
            &refreshed_items,
            ROOM_ID,
            &[],
        );

        assert_eq!(event_ids(&items), vec!["$older", "$a", "$b"]);
    }

    fn event_ids(items: &[RoomTimelineItem]) -> Vec<&str> {
        items.iter().map(RoomTimelineItem::event_id).collect()
    }

    fn test_timeline_item(event_id: &str, timestamp_unix_ms: u64) -> RoomTimelineItem {
        RoomTimelineItem::text_message(
            event_id.to_owned(),
            String::from("@alice:example.org"),
            None,
            String::from("body"),
            timestamp_unix_ms,
            false,
            false,
        )
    }
}
