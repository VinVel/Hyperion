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

// Tune these values per platform; WebKitGTK benefits from a higher entry speed.
const timelineScrollSeekEnterVelocity = 1_000;
const timelineScrollSeekExitVelocity = 50;

export function createTimelineScrollSeek() {
  let discardNextDelta = false;

  function discardStaleDelta(velocity: number): boolean {
    if (!discardNextDelta) return false;
    // Virtuoso retains its sampled scrollTop across idle periods. Prepend
    // anchoring changes that coordinate without moving the visible messages.
    // Zero is an idle notification, so only a nonzero sample rebases the delta.
    if (velocity !== 0) discardNextDelta = false;
    return true;
  }

  return {
    reset() {
      discardNextDelta = true;
    },
    configuration: {
      enter: (velocity: number) =>
        !discardStaleDelta(velocity) &&
        Math.abs(velocity) > timelineScrollSeekEnterVelocity,
      exit: (velocity: number) =>
        discardStaleDelta(velocity) ||
        Math.abs(velocity) < timelineScrollSeekExitVelocity,
    },
  };
}
