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

use super::ShellSyncCoordinator;

const ACCOUNT_KEY: &str = "@alice:example.org";
const ROOM_A: &str = "!a:example.org";
const ROOM_B: &str = "!b:example.org";

#[test]
fn focus_switch_keeps_exactly_the_last_visible_room() {
    let coordinator = ShellSyncCoordinator::new();

    coordinator.set_focused_room(ACCOUNT_KEY, ROOM_A);
    coordinator.set_focused_room(ACCOUNT_KEY, ROOM_B);

    assert_eq!(
        coordinator.focused_room_id(ACCOUNT_KEY).as_deref(),
        Some(ROOM_B)
    );
}

#[test]
fn clearing_an_account_removes_its_focus() {
    let coordinator = ShellSyncCoordinator::new();
    coordinator.set_focused_room(ACCOUNT_KEY, ROOM_A);

    coordinator.clear_account_focus(ACCOUNT_KEY);

    assert_eq!(coordinator.focused_room_id(ACCOUNT_KEY), None);
}
