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

import { describe, expect, test } from "vitest";
import type { RoomTimeline, RoomTimelineItem } from "./appShellAdapters";
import {
  durableRoomTimeline,
  mergeOlderTimelineItems,
  mergeOlderTimelineItemsWithCounts,
  mergeTimelineRefresh,
  prependTimelinePage,
} from "./timeline/helpers";
import {
  idlePaginationState,
  paginationBackoffDelayMilliseconds,
  paginationCanAutomaticallyContinue,
  paginationCanAdvanceCursor,
  paginationCanLoadAtTimelineStart,
  paginationErrorIsRateLimited,
  paginationIsLoading,
  paginationStateKey,
  timelineContextKey,
  type PaginationState,
} from "./pagination";

const roomId = "!room:example.org";

describe("timeline reconciliation helpers", () => {
  test("live refresh does not preserve stale bad order", () => {
    const currentTimeline = testTimeline([
      testTimelineItem("$reply", 2),
      testTimelineItem("$target", 1),
    ]);
    const refreshedTimeline = testTimeline([
      testTimelineItem("$target", 1),
      testTimelineItem("$reply", 2),
    ]);

    const mergedTimeline = mergeTimelineRefresh(
      currentTimeline,
      refreshedTimeline,
    );

    expect(eventIds(mergedTimeline.items)).toEqual(["$target", "$reply"]);
  });

  test("live refresh preserves older prefix before authoritative window", () => {
    const currentTimeline = testTimeline([
      testTimelineItem("$older", 0),
      testTimelineItem("$reply", 2),
      testTimelineItem("$target", 1),
    ]);
    const refreshedTimeline = testTimeline([
      testTimelineItem("$target", 1),
      testTimelineItem("$reply", 2),
    ]);

    const mergedTimeline = mergeTimelineRefresh(
      currentTimeline,
      refreshedTimeline,
    );

    expect(eventIds(mergedTimeline.items)).toEqual([
      "$older",
      "$target",
      "$reply",
    ]);
  });

  test("transient empty live refresh preserves current timeline", () => {
    const currentTimeline = testTimeline([
      testTimelineItem("$older", 0),
      testTimelineItem("$current", 1),
    ]);
    const refreshedTimeline = testTimeline([]);

    const mergedTimeline = mergeTimelineRefresh(
      currentTimeline,
      refreshedTimeline,
    );

    expect(eventIds(mergedTimeline.items)).toEqual(["$older", "$current"]);
  });

  test("durable timeline snapshots retain only a bounded recent window", () => {
    const items = Array.from({ length: 101 }, (_, index) =>
      testTimelineItem(`$${index}`, index),
    );

    const durableTimeline = durableRoomTimeline({
      ...testTimeline(items),
      nextBefore: "timeline-ui-page:101",
    });

    expect(durableTimeline.items).toHaveLength(100);
    expect(durableTimeline.items[0]?.id).toBe("$1");
    expect(durableTimeline.nextBefore).toBeNull();
  });

  test("unchanged live refresh preserves timeline and item identities", () => {
    const currentItem = testTimelineItem("$current", 1);
    const currentTimeline = testTimeline([currentItem]);
    const refreshedTimeline = testTimeline([
      { ...currentItem, reactions: [], receipts: [] },
    ]);

    const mergedTimeline = mergeTimelineRefresh(
      currentTimeline,
      refreshedTimeline,
    );

    expect(mergedTimeline).toBe(currentTimeline);
    expect(mergedTimeline.items[0]).toBe(currentItem);
  });

  test("changed live refresh only replaces the changed item", () => {
    const unchangedItem = testTimelineItem("$unchanged", 1);
    const changedItem = testTimelineItem("$changed", 2);
    const currentTimeline = testTimeline([unchangedItem, changedItem]);
    const refreshedTimeline = testTimeline([
      testTimelineItem("$unchanged", 1),
      { ...testTimelineItem("$changed", 2), body: "updated body" },
    ]);

    const mergedTimeline = mergeTimelineRefresh(
      currentTimeline,
      refreshedTimeline,
    );

    expect(mergedTimeline.items[0]).toBe(unchangedItem);
    expect(mergedTimeline.items[1]).not.toBe(changedItem);
    expect(mergedTimeline.items[1]?.body).toBe("updated body");
  });

  test("older timeline pages prepend before visible window", () => {
    const currentItems = [testTimelineItem("$3", 3), testTimelineItem("$4", 4)];
    const olderItems = [testTimelineItem("$1", 1), testTimelineItem("$2", 2)];

    const mergedItems = mergeOlderTimelineItems(currentItems, olderItems);

    expect(eventIds(mergedItems)).toEqual(["$1", "$2", "$3", "$4"]);
  });

  test("older timeline merge reports duplicate-only retryable pages", () => {
    const currentItems = [testTimelineItem("$1", 1), testTimelineItem("$2", 2)];
    const olderItems = [testTimelineItem("$1", 1), testTimelineItem("$2", 2)];

    const result = mergeOlderTimelineItemsWithCounts(currentItems, olderItems);

    expect(result.insertedCount).toBe(0);
    expect(result.duplicateCount).toBe(2);
    expect(eventIds(result.items)).toEqual(["$1", "$2"]);
  });

  test("older timeline merge reports empty retryable pages", () => {
    const currentItems = [testTimelineItem("$1", 1)];

    const result = mergeOlderTimelineItemsWithCounts(currentItems, []);

    expect(result.insertedCount).toBe(0);
    expect(result.duplicateCount).toBe(0);
    expect(eventIds(result.items)).toEqual(["$1"]);
  });

  test("pagination prepends items and shifts the virtual index in one model update", () => {
    const currentTimeline = {
      ...testTimeline([testTimelineItem("$3", 3), testTimelineItem("$4", 4)]),
      firstItemIndex: 100_000,
      nextBefore: "before-4",
    };

    const result = prependTimelinePage(
      currentTimeline,
      [testTimelineItem("$1", 1), testTimelineItem("$2", 2)],
      "before-2",
      true,
    );

    expect(eventIds(result.timeline.items)).toEqual(["$1", "$2", "$3", "$4"]);
    expect(result.timeline.firstItemIndex).toBe(99_998);
    expect(result.timeline.nextBefore).toBe("before-2");
    expect(result.insertedCount).toBe(2);
  });

  test("duplicate pagination pages leave the virtual index untouched", () => {
    const currentTimeline = {
      ...testTimeline([testTimelineItem("$1", 1), testTimelineItem("$2", 2)]),
      firstItemIndex: 99_998,
    };

    const result = prependTimelinePage(
      currentTimeline,
      [testTimelineItem("$1", 1), testTimelineItem("$2", 2)],
      "before-2",
      true,
    );

    expect(result.timeline.firstItemIndex).toBe(99_998);
    expect(result.insertedCount).toBe(0);
  });
});

describe("pagination state helpers", () => {
  test("pagination state is keyed by account, room, and timeline context", () => {
    const roomAKey = paginationStateKey({
      accountKey: "account-a",
      roomId: "!room-a:example.org",
      timelineContext: "live",
    });
    const roomBKey = paginationStateKey({
      accountKey: "account-a",
      roomId: "!room-b:example.org",
      timelineContext: "live",
    });

    expect(roomAKey).not.toBe(roomBKey);
  });

  test("loading state is explicit and retry states remain clickable", () => {
    const loadingState: PaginationState = {
      status: "loading",
      requestId: "request-1",
      startedAt: 1,
    };
    const errorState: PaginationState = {
      status: "error",
      message: "network",
    };

    expect(paginationIsLoading(loadingState)).toBe(true);
    expect(paginationIsLoading(errorState)).toBe(false);
    expect(paginationIsLoading(idlePaginationState)).toBe(false);
  });

  test("focused timelines use their own pagination context", () => {
    expect(timelineContextKey(null)).toBe("live");
    expect(timelineContextKey("$event")).toBe("focused:$event");
  });

  test("empty and duplicate-only pages with an advanced cursor retry with bounded backoff", () => {
    expect(
      paginationCanAutomaticallyContinue(
        {
          hadNewItems: false,
          nextBefore: "timeline-ui-page:2",
          reachedStart: false,
          tokenChanged: true,
        },
        1,
      ),
    ).toBe(true);
    expect(
      paginationCanAutomaticallyContinue(
        {
          hadNewItems: false,
          nextBefore: null,
          reachedStart: true,
          tokenChanged: true,
        },
        1,
      ),
    ).toBe(false);
    expect(
      paginationCanAutomaticallyContinue(
        {
          hadNewItems: true,
          nextBefore: "timeline-ui-page:2",
          reachedStart: false,
          tokenChanged: true,
        },
        1,
      ),
    ).toBe(false);

    expect(paginationBackoffDelayMilliseconds(0)).toBe(500);
    expect(paginationBackoffDelayMilliseconds(1)).toBe(1_000);
    expect(paginationBackoffDelayMilliseconds(10)).toBe(8_000);
  });

  test("top-boundary pagination requires an upward attempt at the clamped start", () => {
    expect(
      paginationCanLoadAtTimelineStart(false, "timeline-ui-page:2", true, true),
    ).toBe(true);
    expect(
      paginationCanLoadAtTimelineStart(
        false,
        "timeline-ui-page:2",
        true,
        false,
      ),
    ).toBe(false);
    expect(
      paginationCanLoadAtTimelineStart(
        false,
        "timeline-ui-page:2",
        false,
        true,
      ),
    ).toBe(false);
    expect(
      paginationCanLoadAtTimelineStart(true, "timeline-ui-page:2", true, true),
    ).toBe(false);
    expect(paginationCanLoadAtTimelineStart(false, null, true, true)).toBe(
      false,
    );
  });

  test("a top-boundary pagination session caps cursor advances", () => {
    expect(paginationCanAdvanceCursor(0)).toBe(true);
    expect(paginationCanAdvanceCursor(2)).toBe(true);
    expect(paginationCanAdvanceCursor(3)).toBe(false);
    expect(
      paginationCanAutomaticallyContinue(
        {
          hadNewItems: false,
          nextBefore: "timeline-ui-page:4",
          reachedStart: false,
          tokenChanged: true,
        },
        3,
      ),
    ).toBe(false);
  });

  test("rate-limited pagination errors are distinguished for feedback", () => {
    expect(
      paginationErrorIsRateLimited("Matrix error M_LIMIT_EXCEEDED (429)"),
    ).toBe(true);
    expect(paginationErrorIsRateLimited("network connection lost")).toBe(false);
  });
});

function testTimeline(items: RoomTimelineItem[]): RoomTimeline {
  return {
    roomId,
    items,
    nextBefore: null,
    focusedEventId: null,
    redactedEventIds: [],
  };
}

function testTimelineItem(
  id: string,
  timestampUnixMs: number,
): RoomTimelineItem {
  return {
    id,
    transactionId: null,
    senderId: "@alice:example.org",
    senderDisplayName: "Alice",
    senderAvatarUrl: "",
    body: "body",
    formattedBody: "",
    formattedBodyFormat: null,
    contentKind: "text",
    timestampUnixMs,
    timeLabel: "",
    isEdited: false,
    isRedacted: false,
    isOwnMessage: false,
    sendState: "sent",
    decryptionState: "unencrypted",
    groupPosition: "standalone",
    permalink: "",
    canEdit: false,
    canRedact: false,
    canReply: true,
    canReact: true,
    reactions: [],
    receipts: [],
    thread: null,
    threadReplyTo: null,
    replyPreview: null,
  };
}

function eventIds(items: RoomTimelineItem[]): string[] {
  return items.map((item) => item.id);
}
