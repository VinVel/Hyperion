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

use std::collections::HashMap;

use crate::utils::time::now_unix_ms;

use super::{coordinator::ShellSyncCoordinator, diagnostics::emit_sync_diagnostic};

pub(super) const ACTIVE_ROOM_OWNER: &str = "shell.active_room";
pub(super) const TIMELINE_LATEST_OWNER: &str = "timeline.latest";
pub(super) const FOCUSED_CONTEXT_OWNER: &str = "timeline.focused_context";
pub(super) const BACKGROUND_WARMUP_OWNER: &str = "timeline.background_warmup";
pub(super) const EPHEMERAL_TYPING_OWNER: &str = "typing.ephemeral";
pub(super) const SEARCH_BACKFILL_OWNER: &str = "search.backfill";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[allow(
    dead_code,
    reason = "Stage 2 defines the complete room-interest vocabulary before every interest is produced by a migrated caller."
)]
pub(in crate::shell::service) enum RoomInterestKind {
    ActiveRoomOpen,
    TimelineLatest,
    FocusedEventContext,
    BackgroundWarmup,
    TimelinePreview,
    SearchBackfill,
    EphemeralTyping,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::shell::service) enum RoomFocusMode {
    Focused,
    Observed,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(super) struct RoomInterestKey {
    pub(super) kind: RoomInterestKind,
    pub(super) owner: String,
}

#[derive(Clone, Debug)]
pub(super) struct RoomInterest {
    pub(super) key: RoomInterestKey,
    pub(super) reason: String,
    pub(super) created_at_unix_ms: u64,
}

#[derive(Clone, Debug)]
pub(super) struct RoomInterestState {
    pub(super) interests: HashMap<RoomInterestKey, RoomInterest>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct AccountRoomInterestState {
    pub(super) focused_room_id: Option<String>,
    pub(super) rooms: HashMap<String, RoomInterestState>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct RoomInterestStore {
    pub(super) accounts: HashMap<String, AccountRoomInterestState>,
}

struct RoomObservationOutcome {
    previous_focus_room_id: Option<String>,
    focused_room_id: Option<String>,
    should_clear_previous_focus: bool,
    should_subscribe_focused_room: bool,
    previous_active_interest_was_released: bool,
    snapshot: String,
}

impl ShellSyncCoordinator {
    pub(in crate::shell::service) fn observe_room(
        &self,
        account_key: &str,
        room_id: &str,
        kind: RoomInterestKind,
        owner: &str,
        reason: &str,
        focus_mode: RoomFocusMode,
    ) {
        let outcome =
            self.record_room_observation(account_key, room_id, kind, owner, reason, focus_mode);
        emit_sync_diagnostic(
            "sync.room.observe",
            &[
                ("account_key", account_key),
                ("room_id", room_id),
                ("kind", kind.label()),
                ("owner", owner),
                ("focus", focus_mode.label()),
                (
                    "focused_room",
                    outcome.focused_room_id.as_deref().unwrap_or("none"),
                ),
            ],
        );
        if let Some(previous_focus_room_id) = outcome.previous_focus_room_id.as_deref()
            && outcome.should_clear_previous_focus
        {
            if outcome.previous_active_interest_was_released {
                emit_sync_diagnostic(
                    "sync.room.release",
                    &[
                        ("account_key", account_key),
                        ("room_id", previous_focus_room_id),
                        ("kind", RoomInterestKind::ActiveRoomOpen.label()),
                        ("owner", ACTIVE_ROOM_OWNER),
                    ],
                );
            }
            emit_sync_diagnostic(
                "sync.room.focus.clear",
                &[
                    ("account_key", account_key),
                    ("room_id", previous_focus_room_id),
                ],
            );
            self.sync_manager
                .clear_focused_room(account_key, previous_focus_room_id);
            self.release_ephemeral_typing_for_room(
                account_key,
                previous_focus_room_id,
                "focus_changed",
            );
        }
        if outcome.should_subscribe_focused_room {
            emit_sync_diagnostic(
                "sync.room.focus.set",
                &[("account_key", account_key), ("room_id", room_id)],
            );
            emit_sync_diagnostic(
                "sync.room.sdk.subscribe",
                &[
                    ("account_key", account_key),
                    ("room_id", room_id),
                    ("reason", "focused_room"),
                ],
            );
            self.sync_manager.set_focused_room(account_key, room_id);
        }
        emit_sync_diagnostic(
            "sync.room.interest.snapshot",
            &[
                ("account_key", account_key),
                ("room_id", room_id),
                (
                    "focused_room",
                    outcome.focused_room_id.as_deref().unwrap_or("none"),
                ),
                ("interests", &outcome.snapshot),
            ],
        );
    }
    fn record_room_observation(
        &self,
        account_key: &str,
        room_id: &str,
        kind: RoomInterestKind,
        owner: &str,
        reason: &str,
        focus_mode: RoomFocusMode,
    ) -> RoomObservationOutcome {
        let now = now_unix_ms();
        let mut store = self
            .room_interests
            .write()
            .expect("shell sync coordinator room-interest lock poisoned");
        let account_state = store.accounts.entry(account_key.to_owned()).or_default();
        let previous_focus_room_id = account_state.focused_room_id.clone();
        let previous_active_interest_was_released = if matches!(focus_mode, RoomFocusMode::Focused)
            && previous_focus_room_id
                .as_deref()
                .is_some_and(|previous_room_id| previous_room_id != room_id)
            && let Some(previous_focus_room_id) = previous_focus_room_id.as_deref()
        {
            Self::release_room_interest_locked(
                account_state,
                previous_focus_room_id,
                RoomInterestKind::ActiveRoomOpen,
                ACTIVE_ROOM_OWNER,
            )
        } else {
            false
        };

        let room_state = account_state
            .rooms
            .entry(room_id.to_owned())
            .or_insert_with(|| RoomInterestState {
                interests: HashMap::new(),
            });
        let key = RoomInterestKey {
            kind,
            owner: owner.to_owned(),
        };
        let created_at_unix_ms = room_state
            .interests
            .get(&key)
            .map_or(now, |interest| interest.created_at_unix_ms);
        room_state.interests.insert(
            key.clone(),
            RoomInterest {
                key,
                reason: reason.to_owned(),
                created_at_unix_ms,
            },
        );

        if matches!(focus_mode, RoomFocusMode::Focused) {
            account_state.focused_room_id = Some(room_id.to_owned());
        }

        let focused_room_id = account_state.focused_room_id.clone();
        let should_subscribe_focused_room = matches!(focus_mode, RoomFocusMode::Focused)
            && previous_focus_room_id.as_deref() != Some(room_id);
        let should_clear_previous_focus = previous_focus_room_id
            .as_deref()
            .is_some_and(|previous_room_id| previous_room_id != room_id);
        let snapshot = account_state
            .rooms
            .get(room_id)
            .map_or_else(String::new, Self::interest_snapshot);

        let outcome = RoomObservationOutcome {
            previous_focus_room_id,
            focused_room_id,
            should_clear_previous_focus,
            should_subscribe_focused_room,
            previous_active_interest_was_released,
            snapshot,
        };
        drop(store);

        outcome
    }
    fn release_room_interest_locked(
        account_state: &mut AccountRoomInterestState,
        room_id: &str,
        kind: RoomInterestKind,
        owner: &str,
    ) -> bool {
        let key = RoomInterestKey {
            kind,
            owner: owner.to_owned(),
        };
        let Some(room_state) = account_state.rooms.get_mut(room_id) else {
            return false;
        };

        let was_removed = room_state.interests.remove(&key).is_some();
        if room_state.interests.is_empty() {
            account_state.rooms.remove(room_id);
        }

        was_removed
    }

    #[allow(
        dead_code,
        reason = "Stage 2 defines release semantics before frontend close/background events are routed to the backend."
    )]
    pub(in crate::shell::service) fn release_room_interest(
        &self,
        account_key: &str,
        room_id: &str,
        kind: RoomInterestKind,
        owner: &str,
    ) {
        let mut store = self
            .room_interests
            .write()
            .expect("shell sync coordinator room-interest lock poisoned");
        let Some(account_state) = store.accounts.get_mut(account_key) else {
            return;
        };

        let was_removed = Self::release_room_interest_locked(account_state, room_id, kind, owner);
        let focused_room_was_released = account_state.focused_room_id.as_deref() == Some(room_id)
            && !account_state.rooms.contains_key(room_id);
        if focused_room_was_released {
            account_state.focused_room_id = None;
        }
        let focused_room_id = account_state.focused_room_id.clone();
        let snapshot = account_state
            .rooms
            .get(room_id)
            .map_or_else(String::new, Self::interest_snapshot);
        drop(store);

        if was_removed {
            emit_sync_diagnostic(
                "sync.room.release",
                &[
                    ("account_key", account_key),
                    ("room_id", room_id),
                    ("kind", kind.label()),
                    ("owner", owner),
                ],
            );
        }
        if focused_room_was_released {
            emit_sync_diagnostic(
                "sync.room.focus.clear",
                &[("account_key", account_key), ("room_id", room_id)],
            );
            self.sync_manager.clear_focused_room(account_key, room_id);
        }
        if was_removed && matches!(kind, RoomInterestKind::EphemeralTyping) {
            self.clear_typing_room_state(account_key, room_id, "room_interest_released");
        }
        emit_sync_diagnostic(
            "sync.room.interest.snapshot",
            &[
                ("account_key", account_key),
                ("room_id", room_id),
                ("focused_room", focused_room_id.as_deref().unwrap_or("none")),
                ("interests", &snapshot),
            ],
        );
    }
    fn interest_snapshot(room_state: &RoomInterestState) -> String {
        let mut interests = room_state
            .interests
            .values()
            .map(|interest| {
                format!(
                    "{}:{}:{}",
                    interest.key.kind.label(),
                    interest.key.owner,
                    interest.reason
                )
            })
            .collect::<Vec<String>>();
        interests.sort();
        interests.join(",")
    }
    pub(super) fn clear_inactive_account_interests(&self, active_account_key: &str) {
        let mut store = self
            .room_interests
            .write()
            .expect("shell sync coordinator room-interest lock poisoned");
        let inactive_account_keys = store
            .accounts
            .keys()
            .filter(|account_key| account_key.as_str() != active_account_key)
            .cloned()
            .collect::<Vec<String>>();

        for account_key in inactive_account_keys {
            emit_sync_diagnostic("sync.room.focus.clear", &[("account_key", &account_key)]);
            store.accounts.remove(&account_key);
        }
    }
    pub(super) fn clear_account_interests(&self, account_key: &str) {
        let mut store = self
            .room_interests
            .write()
            .expect("shell sync coordinator room-interest lock poisoned");
        let focused_room_id = store
            .accounts
            .get(account_key)
            .and_then(|account_state| account_state.focused_room_id.clone());
        if let Some(focused_room_id) = focused_room_id {
            emit_sync_diagnostic(
                "sync.room.focus.clear",
                &[("account_key", account_key), ("room_id", &focused_room_id)],
            );
        }
        store.accounts.remove(account_key);
    }
    pub(super) fn clear_all_account_interests(&self) {
        let mut store = self
            .room_interests
            .write()
            .expect("shell sync coordinator room-interest lock poisoned");
        for (account_key, account_state) in &store.accounts {
            if let Some(focused_room_id) = account_state.focused_room_id.as_deref() {
                emit_sync_diagnostic(
                    "sync.room.focus.clear",
                    &[("account_key", account_key), ("room_id", focused_room_id)],
                );
            }
        }
        store.accounts.clear();
    }
}

impl RoomInterestKind {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::ActiveRoomOpen => "active_room_open",
            Self::TimelineLatest => "timeline_latest",
            Self::FocusedEventContext => "focused_event_context",
            Self::BackgroundWarmup => "background_warmup",
            Self::TimelinePreview => "timeline_preview",
            Self::SearchBackfill => "search_backfill",
            Self::EphemeralTyping => "ephemeral_typing",
        }
    }
}

impl RoomFocusMode {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Focused => "focused",
            Self::Observed => "observed",
        }
    }
}
