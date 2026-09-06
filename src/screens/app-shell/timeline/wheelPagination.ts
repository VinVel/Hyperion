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

// A fresh wheel burst replenishes the batch budget only after this idle gap.
const wheelBurstIdleMilliseconds = 200;

export function createWheelPaginationIntent() {
  let lastInput: number | null = null;
  let consumed = false;
  return (
    timestamp: number,
    deltaY: number,
    atOldestEdge: boolean,
  ): boolean => {
    if (
      lastInput === null ||
      timestamp - lastInput >= wheelBurstIdleMilliseconds
    )
      consumed = false;
    lastInput = timestamp;
    if (consumed || deltaY >= 0 || !atOldestEdge) return false;
    consumed = true;
    return true;
  };
}
