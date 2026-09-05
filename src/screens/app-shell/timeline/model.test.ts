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
import type { RoomTimeline, RoomTimelineItem } from "../appShellAdapters";
import {
  applyTimelineSnapshot,
  applyTimelinePaginationStatus,
  createTimelineModel,
} from "./model";

const roomId = "!room:example.org";
const selection = { accountKey: "account-a", roomId, focusedEventId: null };

function opened(items: RoomTimelineItem[] = []) {
  const model = createTimelineModel(selection);
  return applyTimelineSnapshot(
    model,
    model.session,
    testTimeline(items),
    "initial",
  );
}

describe("SDK snapshot model", () => {
  test("replaces the entire projection, including older prefixes and empty snapshots", () => {
    const model = opened([
      testTimelineItem("$old", 0),
      testTimelineItem("$new", 1),
    ]);
    const next = applyTimelineSnapshot(
      model,
      model.session,
      { ...testTimeline([testTimelineItem("$new", 1)]), revision: 2 },
      "update",
    );
    expect(eventIds(next.timeline!.items)).toEqual(["$new"]);
    const empty = applyTimelineSnapshot(
      next,
      next.session,
      { ...testTimeline([]), revision: 3 },
      "refresh",
    );
    expect(empty.timeline!.items).toEqual([]);
  });

  test("preserves identical sends and SDK local echoes without body/time reconciliation", () => {
    const items = [
      testTimelineItem("transaction", 1),
      testTimelineItem("$one", 1),
      testTimelineItem("$two", 1),
    ].map((item) => ({ ...item, isOwnMessage: true }));
    const model = opened(items);
    expect(eventIds(model.timeline!.items)).toEqual([
      "transaction",
      "$one",
      "$two",
    ]);
  });

  test("rejects another account, room, context, instance, and stale refresh revisions", () => {
    const model = opened([testTimelineItem("$new", 1)]);
    const identity = model.timeline!.timelineIdentity;
    for (const override of [
      { accountKey: "other" },
      { roomId: "other" },
      { focusedEventId: "$focus" },
      { instanceId: "old" },
    ]) {
      const snapshot = {
        ...testTimeline([]),
        revision: 2,
        timelineIdentity: { ...identity, ...override },
      };
      expect(
        applyTimelineSnapshot(model, model.session, snapshot, "update"),
      ).toBe(model);
    }
    expect(
      applyTimelineSnapshot(
        model,
        model.session,
        { ...testTimeline([]), revision: 0 },
        "refresh",
      ),
    ).toBe(model);
  });

  test("binds using the opening response and then uses its newest pending subscription snapshot", () => {
    let model = createTimelineModel(selection);
    const newer = {
      ...testTimeline([testTimelineItem("$new", 2)]),
      revision: 3,
    };
    model = applyTimelineSnapshot(model, model.session, newer, "update");
    expect(model.timeline).toBeNull();
    model = applyTimelineSnapshot(
      model,
      model.session,
      {
        ...testTimeline([]),
        timelineIdentity: { ...newer.timelineIdentity, instanceId: "obsolete" },
        revision: 99,
      },
      "update",
    );
    model = applyTimelineSnapshot(
      model,
      model.session,
      { ...testTimeline([testTimelineItem("$old", 1)]), nextBefore: "cursor" },
      "initial",
    );
    expect(model.timeline!.revision).toBe(3);
    expect(eventIds(model.timeline!.items)).toEqual(["$new"]);
    expect(model.timeline!.nextBefore).toBe("cursor");
  });

  test("late work from room A cannot apply after A to B to A or a new account session", () => {
    const old = opened();
    const current = createTimelineModel(selection);
    expect(
      applyTimelineSnapshot(current, old.session, testTimeline([]), "initial"),
    ).toBe(current);
    const recreated = applyTimelineSnapshot(
      current,
      current.session,
      {
        ...testTimeline([]),
        timelineIdentity: {
          ...testTimeline([]).timelineIdentity,
          instanceId: "recreated",
        },
      },
      "initial",
    );
    expect(
      applyTimelineSnapshot(
        recreated,
        recreated.session,
        { ...testTimeline([]), revision: 99 },
        "update",
      ),
    ).toBe(recreated);
  });

  test("reuses unchanged item objects and the row array across serialized snapshots", () => {
    const model = opened([
      testTimelineItem("$same", 1),
      testTimelineItem("$edit", 2),
    ]);
    const copy = structuredClone(model.timeline!);
    copy.revision++;
    const unchanged = applyTimelineSnapshot(
      model,
      model.session,
      copy,
      "update",
    );
    expect(unchanged.timeline!.items).toBe(model.timeline!.items);
    copy.revision++;
    copy.items[1]!.body = "edited";
    const changed = applyTimelineSnapshot(
      unchanged,
      unchanged.session,
      copy,
      "update",
    );
    expect(changed.timeline!.items[0]).toBe(model.timeline!.items[0]);
    expect(changed.timeline!.items[1]).not.toBe(model.timeline!.items[1]);
  });

  test("SDK edits, reactions, receipts, redactions and decryption changes replace only changed rows", () => {
    const original = testTimelineItem("$event", 1);
    original.richText = [{ type: "text", text: "body" }];
    const model = opened([original, testTimelineItem("$untouched", 2)]);
    const unchanged = { ...structuredClone(model.timeline!), revision: 2 };
    expect(
      applyTimelineSnapshot(model, model.session, unchanged, "update").timeline!
        .items,
    ).toBe(model.timeline!.items);
    const changes: Partial<RoomTimelineItem>[] = [
      { body: "edit", isEdited: true },
      { reactions: [{ key: "👍", count: 1, reactedByMe: true }] },
      {
        receipts: [
          {
            userId: "@reader:example.org",
            displayName: "Reader",
            avatarUrl: "",
            timestampUnixMs: 3,
          },
        ],
      },
      { isRedacted: true, contentKind: "redacted", body: "" },
      { decryptionState: "decrypted", body: "decrypted text" },
      { sendState: "failed" },
      { richText: [{ type: "text", text: "updated markup" }] },
    ];
    for (const change of changes) {
      const snapshot = {
        ...testTimeline([
          { ...original, ...change },
          testTimelineItem("$untouched", 2),
        ]),
        revision: 2,
      };
      const next = applyTimelineSnapshot(
        model,
        model.session,
        snapshot,
        "update",
      );
      expect(next.timeline!.items[0]).not.toBe(original);
      expect(next.timeline!.items[0]).toMatchObject(change);
      expect(next.timeline!.items[1]).toBe(model.timeline!.items[1]);
    }
  });

  test("focused snapshots keep SDK order and accept focused subscription updates", () => {
    let model = createTimelineModel({ ...selection, focusedEventId: "$focus" });
    const snapshot = {
      ...testTimeline([
        testTimelineItem("$later", 9),
        testTimelineItem("$earlier", 1),
      ]),
      focusedEventId: "$focus",
      timelineIdentity: {
        ...testTimeline([]).timelineIdentity,
        focusedEventId: "$focus",
      },
    };
    model = applyTimelineSnapshot(model, model.session, snapshot, "initial");
    expect(eventIds(model.timeline!.items)).toEqual(["$later", "$earlier"]);
    model = applyTimelineSnapshot(
      model,
      model.session,
      { ...snapshot, revision: 2, items: [snapshot.items[1]!] },
      "update",
    );
    expect(eventIds(model.timeline!.items)).toEqual(["$earlier"]);
  });

  test("status and append updates preserve the current virtual index", () => {
    const model = opened([testTimelineItem("$current", 2)]);
    const appended = applyTimelineSnapshot(
      model,
      model.session,
      {
        ...testTimeline([
          ...model.timeline!.items,
          testTimelineItem("$arrival", 3),
        ]),
        revision: 2,
      },
      "update",
    );
    expect(appended.timeline!.firstItemIndex).toBe(
      model.timeline!.firstItemIndex,
    );
  });

  test("pagination completion changes status only, before or after snapshot arrival", () => {
    let model = opened([testTimelineItem("$current", 2)]);
    const rows = model.timeline!.items;
    const response = {
      roomId,
      nextBefore: "next",
      tokenChanged: true,
      items: [testTimelineItem("$wrong-source", 0)],
    };
    model = applyTimelinePaginationStatus(
      model,
      model.session,
      "instance-a",
      response,
    );
    expect(model.timeline!.items).toBe(rows);
    expect(model.timeline!.revision).toBe(1);
    model = applyTimelineSnapshot(
      model,
      model.session,
      {
        ...testTimeline([testTimelineItem("$older", 1), ...rows]),
        revision: 2,
      },
      "update",
    );
    expect(eventIds(model.timeline!.items)).toEqual(["$older", "$current"]);
    expect(model.timeline!.nextBefore).toBe("next");
    const after = applyTimelinePaginationStatus(
      model,
      model.session,
      "instance-a",
      { ...response, nextBefore: null },
    );
    expect(after.timeline!.items).toBe(model.timeline!.items);
    expect(after.timeline!.firstItemIndex).toBe(model.timeline!.firstItemIndex);
  });
});

function testTimeline(items: RoomTimelineItem[]): RoomTimeline {
  return {
    roomId,
    timelineIdentity: {
      accountKey: "account-a",
      roomId,
      instanceId: "instance-a",
      focusedEventId: null,
    },
    revision: 1,
    firstItemIndex: 100_000,
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
    roomId,
    richText: null,
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

// Historical rules removed by the SDK membership regressions above:
// Local echo timestamps and remote event timestamps can differ slightly, so
// reconcile provisional own messages within this bounded send window.

// A high initial index gives Virtuoso room to decrement for older pages.

// Browser storage is a startup convenience, not a second timeline authority.
// Keep its serialized room window bounded so WebKit localStorage cannot grow
// with every older page the user visits.

/**
 * Commits a backwards page as one virtual-list model change. Virtuoso needs
 * the inserted rows and their matching absolute-index shift in the same React
 * update to retain the visible rows as the page is prepended.
 */
// A cursor preceding discarded cache-only items is invalid. The SDK reload
// will provide the authoritative cursor before pagination can continue.

// Former cache and deferred-merge assumptions covered by the model regressions:
// Keep recently opened room views in memory so switching rooms is an immediate
// render operation while the backend refresh catches up.
// A pagination response must be the only timeline model update while
// Virtuoso applies its prepend index shift. SDK refreshes are retained and
// merged immediately after that atomic presentation update.
