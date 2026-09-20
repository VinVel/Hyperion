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
import { classifyTimelineChange, survivingAnchor } from "./viewport";
import { keyboardPaginationIntent } from "./keyboardPagination";

const rows = (...ids: string[]) => ids.map((id) => ({ id }));
test("only pure prepends use Virtuoso anchoring", () => {
  expect(classifyTimelineChange(rows("a", "b"), rows("x", "a", "b"))).toBe(
    "prepend",
  );
  expect(classifyTimelineChange(rows("a", "b"), rows("a", "b", "c"))).toBe(
    "append",
  );
  expect(classifyTimelineChange(rows("a", "b"), rows("a", "b"))).toBe("same");
  expect(classifyTimelineChange(rows("a", "b"), rows("x", "a", "b", "c"))).toBe(
    "structural",
  );
  expect(classifyTimelineChange(rows("a", "b"), rows("b"))).toBe("structural");
});
test("a removed anchor selects the nearest surviving visible row at its own offset", () => {
  const visible = [
    { eventId: "a", offsetPixels: -12 },
    { eventId: "b", offsetPixels: 40 },
    { eventId: "c", offsetPixels: 90 },
  ];
  expect(survivingAnchor(visible, rows("b", "c"))).toEqual(visible[1]);
  expect(survivingAnchor(visible, rows("x"))).toBeNull();
});
test("keyboard pagination needs a fresh upward key at the edge", () => {
  const key = {
    key: "ArrowUp",
    repeat: false,
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
  };
  expect(keyboardPaginationIntent(key, true, false)).toBe(true);
  expect(keyboardPaginationIntent({ ...key, key: "PageUp" }, true, false)).toBe(
    true,
  );
  expect(keyboardPaginationIntent({ ...key, key: "Home" }, true, false)).toBe(
    true,
  );
  expect(keyboardPaginationIntent({ ...key, repeat: true }, true, false)).toBe(
    false,
  );
  expect(keyboardPaginationIntent(key, false, false)).toBe(false);
  expect(keyboardPaginationIntent(key, true, true)).toBe(false);
  expect(
    keyboardPaginationIntent({ ...key, key: "ArrowDown" }, true, false),
  ).toBe(false);
});

test("initial positioning honors saved focused offsets and explicit event navigation", async () => {
  const { initialViewportLocation } = await import("./viewport");
  const timeline = { items: rows("a", "b", "c"), focusedEventId: "b" };
  expect(initialViewportLocation(timeline, null)).toEqual({
    index: 1,
    align: "center",
  });
  expect(
    initialViewportLocation({ ...timeline, focusedEventId: null }, null),
  ).toEqual({ index: 2, align: "end" });
  expect(
    initialViewportLocation(timeline, {
      eventId: "c",
      offsetPixels: -17,
      wasAtBottom: true,
    }),
  ).toEqual({ index: 2, align: "start", offset: 17 });
});
