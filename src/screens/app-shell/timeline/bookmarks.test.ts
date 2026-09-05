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
import { readTimelineBookmark, saveTimelineBookmark } from "./bookmarks";

test("session bookmarks contain only viewport data and are isolated by account, room, and context", () => {
  const selection = {
    accountKey: "account",
    roomId: "room",
    focusedEventId: null,
  };
  const bookmark = {
    eventId: "$event",
    offsetPixels: -12.5,
    wasAtBottom: false,
  };
  saveTimelineBookmark(selection, bookmark);
  expect(readTimelineBookmark(selection)).toEqual(bookmark);
  expect(
    readTimelineBookmark({ ...selection, accountKey: "other" }),
  ).toBeNull();
  expect(readTimelineBookmark({ ...selection, roomId: "other" })).toBeNull();
  expect(
    readTimelineBookmark({ ...selection, focusedEventId: "$anchor" }),
  ).toBeNull();
  // Caller-owned objects cannot mutate a saved reading position.
  bookmark.eventId = "$changed";
  expect(readTimelineBookmark(selection)?.eventId).toBe("$event");
});
