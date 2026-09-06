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

import { expect, test } from "vitest";
import { createWheelPaginationIntent } from "./wheelPagination";
test("a continuous wheel burst cannot replenish a completed batch", () => {
  const intent = createWheelPaginationIntent();
  expect(intent(0, -1, true)).toBe(true);
  expect(intent(100, -1, true)).toBe(false);
  expect(intent(299, -1, true)).toBe(false);
  expect(intent(499, -1, true)).toBe(true);
});
test("wheel input needs upward intent at the oldest edge", () => {
  const intent = createWheelPaginationIntent();
  expect(intent(0, -1, false)).toBe(false);
  expect(intent(50, 1, true)).toBe(false);
  expect(intent(100, -1, true)).toBe(true);
});
