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

import { beforeEach, expect, test, vi } from "vitest";
import { resolveReadingPosition } from "./restoration";
import { saveTimelineBookmark } from "./bookmarks";

const selection = {
  accountKey: "restore",
  roomId: "room",
  focusedEventId: null,
};
const bookmark = { eventId: "saved", offsetPixels: -7, wasAtBottom: false };
beforeEach(() => saveTimelineBookmark(selection, bookmark));
function snapshot(ids: string[], focusedEventId: string | null = null) {
  return {
    timelineIdentity: { ...selection, focusedEventId, instanceId: "instance" },
    focusedEventId,
    roomId: "room",
    revision: 1,
    items: ids.map((id) => ({ id })),
    nextBefore: "more",
    redactedEventIds: [],
  };
}
test("retained history restores without another SDK load", async () => {
  saveTimelineBookmark(selection, bookmark);
  const load = vi.fn();
  const result = await resolveReadingPosition(
    selection,
    snapshot(["saved"]) as never,
    load,
  );
  expect(load).not.toHaveBeenCalled();
  expect(result.readingPosition?.bookmark).toEqual(bookmark);
});
test("a missing saved event uses SDK context and retains the original bookmark namespace", async () => {
  const load = vi.fn().mockResolvedValue(snapshot(["saved"], "saved"));
  const result = await resolveReadingPosition(
    selection,
    snapshot(["new"]) as never,
    load,
  );
  expect(load).toHaveBeenCalledWith("saved");
  expect(result.readingPosition?.selection).toEqual(selection);
  expect(result.timelineIdentity.focusedEventId).toBe("saved");
});
test("missing anchors and load failures remain explicit failures", async () => {
  await expect(
    resolveReadingPosition(
      selection,
      snapshot(["new"]) as never,
      async () => snapshot(["other"], "saved") as never,
    ),
  ).rejects.toThrow();
  await expect(
    resolveReadingPosition(selection, snapshot(["new"]) as never, async () => {
      throw Error("offline");
    }),
  ).rejects.toThrow("offline");
});
test("a saved live bottom does not fetch historical context", async () => {
  saveTimelineBookmark(
    { ...selection, roomId: "bottom" },
    { ...bookmark, wasAtBottom: true },
  );
  const load = vi.fn();
  await resolveReadingPosition(
    { ...selection, roomId: "bottom" },
    snapshot(["new"]) as never,
    load,
  );
  expect(load).not.toHaveBeenCalled();
});

test("a removed saved anchor preserves a surviving neighbor at its own offset", async () => {
  const other = { ...selection, roomId: "removed" };
  saveTimelineBookmark(other, {
    ...bookmark,
    nearby: [{ eventId: "neighbor", offsetPixels: 70 }],
  });
  const result = await resolveReadingPosition(
    other,
    snapshot(["new"]) as never,
    async () => snapshot(["neighbor"], "saved") as never,
  );
  expect(result.readingPosition?.bookmark.eventId).toBe("neighbor");
  expect(result.readingPosition?.bookmark.offsetPixels).toBe(70);
});

test("context failure loads the nearest saved neighbor through the SDK", async () => {
  saveTimelineBookmark(selection, {
    ...bookmark,
    nearby: [{ eventId: "neighbor", offsetPixels: 70 }],
  });
  const load = vi
    .fn()
    .mockRejectedValueOnce(Error("saved event removed"))
    .mockResolvedValueOnce(snapshot(["neighbor"], "neighbor"));
  const result = await resolveReadingPosition(
    selection,
    snapshot(["new"]) as never,
    load,
  );
  expect(load.mock.calls).toEqual([["saved"], ["neighbor"]]);
  expect(result.readingPosition?.bookmark.eventId).toBe("neighbor");
  expect(result.readingPosition?.bookmark.offsetPixels).toBe(70);
});
