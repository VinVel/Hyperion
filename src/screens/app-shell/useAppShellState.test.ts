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

import { describe, expect, test } from "bun:test";
import type { RoomTimeline, RoomTimelineItem } from "./appShellAdapters";
import {
  mergeOlderTimelineItems,
  mergeOlderTimelineItemsWithCounts,
  mergeTimelineRefresh,
} from "./timelineMerge";
import {
  idlePaginationState,
  paginationIsLoading,
  paginationStateKey,
  timelineContextKey,
  type PaginationState,
} from "./paginationState";

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
    replyPreview: null,
  };
}

function eventIds(items: RoomTimelineItem[]): string[] {
  return items.map((item) => item.id);
}
