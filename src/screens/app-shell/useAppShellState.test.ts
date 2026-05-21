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
import { mergeOlderTimelineItems, mergeTimelineRefresh } from "./timelineMerge";

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
