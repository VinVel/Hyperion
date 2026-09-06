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
import { createTimelineScrollSeek } from "./scrollSeek";

test("a prepend invalidates the next position delta even after idle samples", () => {
  const seek = createTimelineScrollSeek();
  seek.reset();
  expect(seek.configuration.enter(0)).toBe(false);
  expect(seek.configuration.enter(0)).toBe(false);
  expect(seek.configuration.enter(4000)).toBe(false);
  expect(seek.configuration.enter(-10)).toBe(false);
  expect(seek.configuration.enter(-1500)).toBe(true);
});

test("ordinary fast scrolling retains entry and exit thresholds", () => {
  const seek = createTimelineScrollSeek();
  expect(seek.configuration.enter(1500)).toBe(true);
  expect(seek.configuration.exit(100)).toBe(false);
  expect(seek.configuration.exit(20)).toBe(true);
});

test("a prepend also exits existing placeholder mode and consumes only one delta", () => {
  const seek = createTimelineScrollSeek();
  seek.reset();
  expect(seek.configuration.exit(0)).toBe(true);
  expect(seek.configuration.enter(5000)).toBe(false);
  expect(seek.configuration.enter(1500)).toBe(true);
  seek.reset();
  expect(seek.configuration.exit(5000)).toBe(true);
  expect(seek.configuration.enter(1500)).toBe(true);
});
