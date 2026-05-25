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

use super::interests::{
    ACTIVE_ROOM_OWNER, BACKGROUND_WARMUP_OWNER, EPHEMERAL_TYPING_OWNER, RoomInterestKey,
    TIMELINE_LATEST_OWNER,
};
use super::{RoomFocusMode, RoomInterestKind, ShellSyncCoordinator};
use crate::shell::types::RoomTimelineItem;

const ACCOUNT_KEY: &str = "@alice:example.org";
const ROOM_A: &str = "!a:example.org";
const ROOM_B: &str = "!b:example.org";

#[test]
fn observing_a_room_records_reason_keyed_interest() {
    let coordinator = ShellSyncCoordinator::new();

    coordinator.observe_room(
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::TimelineLatest,
        TIMELINE_LATEST_OWNER,
        "latest timeline",
        RoomFocusMode::Observed,
    );

    assert!(has_interest(
        &coordinator,
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::TimelineLatest,
        TIMELINE_LATEST_OWNER,
    ));
    assert_eq!(focused_room_id(&coordinator, ACCOUNT_KEY), None);
}

#[test]
fn repeated_observe_with_same_key_updates_interest_instead_of_duplicating() {
    let coordinator = ShellSyncCoordinator::new();

    coordinator.observe_room(
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::TimelineLatest,
        TIMELINE_LATEST_OWNER,
        "first reason",
        RoomFocusMode::Observed,
    );
    coordinator.observe_room(
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::TimelineLatest,
        TIMELINE_LATEST_OWNER,
        "updated reason",
        RoomFocusMode::Observed,
    );

    assert_eq!(interest_count(&coordinator, ACCOUNT_KEY, ROOM_A), 1);
    assert_eq!(
        interest_reason(
            &coordinator,
            ACCOUNT_KEY,
            ROOM_A,
            RoomInterestKind::TimelineLatest,
            TIMELINE_LATEST_OWNER,
        )
        .as_deref(),
        Some("updated reason"),
    );
}

#[test]
fn releasing_one_interest_leaves_other_interests_intact() {
    let coordinator = ShellSyncCoordinator::new();

    coordinator.observe_room(
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::TimelineLatest,
        TIMELINE_LATEST_OWNER,
        "latest timeline",
        RoomFocusMode::Observed,
    );
    coordinator.observe_room(
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::BackgroundWarmup,
        BACKGROUND_WARMUP_OWNER,
        "warmup",
        RoomFocusMode::Observed,
    );

    coordinator.release_room_interest(
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::TimelineLatest,
        TIMELINE_LATEST_OWNER,
    );

    assert!(!has_interest(
        &coordinator,
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::TimelineLatest,
        TIMELINE_LATEST_OWNER,
    ));
    assert!(has_interest(
        &coordinator,
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::BackgroundWarmup,
        BACKGROUND_WARMUP_OWNER,
    ));
}

#[test]
fn focus_switches_from_room_a_to_room_b_and_releases_old_active_interest() {
    let coordinator = ShellSyncCoordinator::new();

    coordinator.observe_room(
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::TimelineLatest,
        TIMELINE_LATEST_OWNER,
        "latest timeline",
        RoomFocusMode::Observed,
    );
    coordinator.observe_room(
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::ActiveRoomOpen,
        ACTIVE_ROOM_OWNER,
        "visible room",
        RoomFocusMode::Focused,
    );
    coordinator.observe_room(
        ACCOUNT_KEY,
        ROOM_B,
        RoomInterestKind::ActiveRoomOpen,
        ACTIVE_ROOM_OWNER,
        "visible room",
        RoomFocusMode::Focused,
    );

    assert_eq!(
        focused_room_id(&coordinator, ACCOUNT_KEY).as_deref(),
        Some(ROOM_B),
    );
    assert!(!has_interest(
        &coordinator,
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::ActiveRoomOpen,
        ACTIVE_ROOM_OWNER,
    ));
    assert!(has_interest(
        &coordinator,
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::TimelineLatest,
        TIMELINE_LATEST_OWNER,
    ));
    assert!(has_interest(
        &coordinator,
        ACCOUNT_KEY,
        ROOM_B,
        RoomInterestKind::ActiveRoomOpen,
        ACTIVE_ROOM_OWNER,
    ));
}

#[test]
fn focused_room_is_always_observed() {
    let coordinator = ShellSyncCoordinator::new();

    coordinator.observe_room(
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::ActiveRoomOpen,
        ACTIVE_ROOM_OWNER,
        "visible room",
        RoomFocusMode::Focused,
    );

    assert_eq!(
        focused_room_id(&coordinator, ACCOUNT_KEY).as_deref(),
        Some(ROOM_A),
    );
    assert!(has_interest(
        &coordinator,
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::ActiveRoomOpen,
        ACTIVE_ROOM_OWNER,
    ));
}

#[test]
fn account_stop_clears_interest_state() {
    let coordinator = ShellSyncCoordinator::new();

    coordinator.observe_room(
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::ActiveRoomOpen,
        ACTIVE_ROOM_OWNER,
        "visible room",
        RoomFocusMode::Focused,
    );
    tauri::async_runtime::block_on(async {
        coordinator.stop_account(ACCOUNT_KEY).await;
    });

    assert_eq!(focused_room_id(&coordinator, ACCOUNT_KEY), None);
    assert_eq!(interest_count(&coordinator, ACCOUNT_KEY, ROOM_A), 0);
}

#[test]
fn observing_typing_state_for_room_creates_one_subscription_interest() {
    let coordinator = ShellSyncCoordinator::new();

    coordinator.observe_room(
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::EphemeralTyping,
        EPHEMERAL_TYPING_OWNER,
        "typing subscription",
        RoomFocusMode::Observed,
    );
    assert!(!coordinator.reserve_typing_subscription(None, ACCOUNT_KEY, ROOM_A));

    assert!(has_interest(
        &coordinator,
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::EphemeralTyping,
        EPHEMERAL_TYPING_OWNER,
    ));
    assert_eq!(typing_room_count(&coordinator, ACCOUNT_KEY), 1);
    assert!(typing_subscription_reserved(
        &coordinator,
        ACCOUNT_KEY,
        ROOM_A
    ));
}

#[test]
fn repeated_typing_observe_does_not_duplicate_state() {
    let coordinator = ShellSyncCoordinator::new();

    assert!(!coordinator.reserve_typing_subscription(None, ACCOUNT_KEY, ROOM_A));
    assert!(coordinator.reserve_typing_subscription(None, ACCOUNT_KEY, ROOM_A));

    assert_eq!(typing_room_count(&coordinator, ACCOUNT_KEY), 1);
}

#[test]
fn releasing_typing_interest_clears_typing_users() {
    let coordinator = ShellSyncCoordinator::new();

    coordinator.observe_room(
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::EphemeralTyping,
        EPHEMERAL_TYPING_OWNER,
        "typing subscription",
        RoomFocusMode::Observed,
    );
    assert!(!coordinator.reserve_typing_subscription(None, ACCOUNT_KEY, ROOM_A));
    record_test_typing_users(&coordinator, ACCOUNT_KEY, ROOM_A, &[("@bob:example.org")]);

    coordinator.release_room_interest(
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::EphemeralTyping,
        EPHEMERAL_TYPING_OWNER,
    );

    assert!(!has_interest(
        &coordinator,
        ACCOUNT_KEY,
        ROOM_A,
        RoomInterestKind::EphemeralTyping,
        EPHEMERAL_TYPING_OWNER,
    ));
    assert_eq!(typing_user_count(&coordinator, ACCOUNT_KEY, ROOM_A), 0);
}

#[test]
fn account_stop_clears_all_typing_state() {
    let coordinator = ShellSyncCoordinator::new();

    assert!(!coordinator.reserve_typing_subscription(None, ACCOUNT_KEY, ROOM_A));
    assert!(!coordinator.reserve_typing_subscription(None, ACCOUNT_KEY, ROOM_B));
    record_test_typing_users(&coordinator, ACCOUNT_KEY, ROOM_A, &[("@bob:example.org")]);
    record_test_typing_users(&coordinator, ACCOUNT_KEY, ROOM_B, &[("@carol:example.org")]);
    tauri::async_runtime::block_on(async {
        coordinator.stop_account(ACCOUNT_KEY).await;
    });

    assert_eq!(typing_room_count(&coordinator, ACCOUNT_KEY), 0);
}

#[test]
fn typing_update_for_room_a_does_not_affect_room_b() {
    let coordinator = ShellSyncCoordinator::new();

    record_test_typing_users(&coordinator, ACCOUNT_KEY, ROOM_A, &[("@bob:example.org")]);
    record_test_typing_users(
        &coordinator,
        ACCOUNT_KEY,
        ROOM_B,
        &[("@carol:example.org"), ("@dave:example.org")],
    );

    assert_eq!(typing_user_count(&coordinator, ACCOUNT_KEY, ROOM_A), 1);
    assert_eq!(typing_user_count(&coordinator, ACCOUNT_KEY, ROOM_B), 2);
}

#[test]
fn typing_state_is_separate_from_timeline_reconciliation() {
    let coordinator = ShellSyncCoordinator::new();

    record_test_typing_users(&coordinator, ACCOUNT_KEY, ROOM_A, &[("@bob:example.org")]);
    let items =
        crate::shell::service::timeline_reconciliation::reconcile_authoritative_timeline_items(
            vec![test_timeline_item(String::from("$event"))],
            ROOM_A,
            &[],
        );

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].event_id(), "$event");
    assert_eq!(typing_user_count(&coordinator, ACCOUNT_KEY, ROOM_A), 1);
}

fn focused_room_id(coordinator: &ShellSyncCoordinator, account_key: &str) -> Option<String> {
    coordinator
        .room_interests
        .read()
        .expect("test room-interest lock should not be poisoned")
        .accounts
        .get(account_key)
        .and_then(|account_state| account_state.focused_room_id.clone())
}

fn interest_count(coordinator: &ShellSyncCoordinator, account_key: &str, room_id: &str) -> usize {
    coordinator
        .room_interests
        .read()
        .expect("test room-interest lock should not be poisoned")
        .accounts
        .get(account_key)
        .and_then(|account_state| account_state.rooms.get(room_id))
        .map_or(0, |room_state| room_state.interests.len())
}

fn has_interest(
    coordinator: &ShellSyncCoordinator,
    account_key: &str,
    room_id: &str,
    kind: RoomInterestKind,
    owner: &str,
) -> bool {
    interest_reason(coordinator, account_key, room_id, kind, owner).is_some()
}

fn interest_reason(
    coordinator: &ShellSyncCoordinator,
    account_key: &str,
    room_id: &str,
    kind: RoomInterestKind,
    owner: &str,
) -> Option<String> {
    let key = RoomInterestKey {
        kind,
        owner: owner.to_owned(),
    };
    coordinator
        .room_interests
        .read()
        .expect("test room-interest lock should not be poisoned")
        .accounts
        .get(account_key)
        .and_then(|account_state| account_state.rooms.get(room_id))
        .and_then(|room_state| room_state.interests.get(&key))
        .map(|interest| interest.reason.clone())
}

fn typing_room_count(coordinator: &ShellSyncCoordinator, account_key: &str) -> usize {
    coordinator
        .typing_ephemeral_state
        .read()
        .expect("test typing-state lock should not be poisoned")
        .accounts
        .get(account_key)
        .map_or(0, |account_state| account_state.rooms.len())
}

fn typing_subscription_reserved(
    coordinator: &ShellSyncCoordinator,
    account_key: &str,
    room_id: &str,
) -> bool {
    coordinator
        .typing_ephemeral_state
        .read()
        .expect("test typing-state lock should not be poisoned")
        .accounts
        .get(account_key)
        .and_then(|account_state| account_state.rooms.get(room_id))
        .is_some_and(|room_state| room_state.subscription_reserved)
}

fn typing_user_count(
    coordinator: &ShellSyncCoordinator,
    account_key: &str,
    room_id: &str,
) -> usize {
    coordinator
        .typing_ephemeral_state
        .read()
        .expect("test typing-state lock should not be poisoned")
        .accounts
        .get(account_key)
        .and_then(|account_state| account_state.rooms.get(room_id))
        .map_or(0, |room_state| room_state.users.len())
}

fn record_test_typing_users(
    coordinator: &ShellSyncCoordinator,
    account_key: &str,
    room_id: &str,
    users: &[&str],
) {
    let mut store = coordinator
        .typing_ephemeral_state
        .write()
        .expect("test typing-state lock should not be poisoned");
    let room_state = store
        .accounts
        .entry(account_key.to_owned())
        .or_default()
        .rooms
        .entry(room_id.to_owned())
        .or_default();
    room_state.users = users.iter().map(|user| (*user).to_owned()).collect();
    drop(store);
}

fn test_timeline_item(event_id: String) -> RoomTimelineItem {
    RoomTimelineItem::text_message(
        event_id,
        String::from("@alice:example.org"),
        None,
        String::from("body"),
        0,
        false,
        false,
    )
}
