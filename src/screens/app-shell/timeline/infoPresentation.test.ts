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
import type { RoomTimelineItem } from "../appShellAdapters";
import {
  formatRawEventJson,
  timelineJsonSyntaxTheme,
  timelineInfoPresentation,
  timelineInfoViewLabels,
} from "./infoPresentation";

describe("timeline Info presentation", () => {
  test("includes parsed receipt, edit, and relation diagnostics", () => {
    const presentation = timelineInfoPresentation(
      testItem({
        transactionId: "txn-1",
        isEdited: true,
        receipts: [
          {
            userId: "@bob:example.org",
            displayName: "Bob",
            avatarUrl: "",
            timestampUnixMs: 2,
          },
        ],
        replyPreview: {
          eventId: "$reply",
          state: "resolved",
          senderId: "@bob:example.org",
          senderDisplayName: "Bob",
          body: "Target",
          isRedacted: false,
        },
        threadReplyTo: { rootEventId: "$root" },
      }),
    );

    expect(presentation.fields).toEqual(
      expect.arrayContaining([
        ["Transaction ID", "txn-1"],
        ["Edited", "Yes"],
        ["Reply to", "$reply"],
        ["Thread reply to", "$root"],
      ]),
    );
    expect(presentation.receipts).toEqual([["Bob", "2"]]);
  });

  test("shows a disabled thread indicator only when the SDK supplies a count", () => {
    expect(timelineInfoPresentation(testItem()).threadIndicator).toBeNull();
    expect(
      timelineInfoPresentation(
        testItem({
          thread: { rootEventId: "$event", latestEventId: null, replyCount: 3 },
        }),
      ).threadIndicator,
    ).toEqual({ label: "3 thread replies", disabled: true });
  });

  test("keeps raw event JSON hidden until Advanced is explicitly opened", () => {
    const presentation = timelineInfoPresentation(testItem(), '{"body":"raw"}');
    expect(presentation.rawEventJson).toBe('{"body":"raw"}');
    expect(presentation.rawEventIsVisibleByDefault).toBe(false);
  });

  test("formats raw event JSON for readable diagnostic inspection", () => {
    expect(formatRawEventJson('{"content":{"body":"Hello"}}')).toBe(
      '{\n  "content": {\n    "body": "Hello"\n  }\n}',
    );
  });

  test("uses concise Easy and Advanced labels for the information view switcher", () => {
    expect(timelineInfoViewLabels).toEqual(["Easy View", "Advanced View"]);
  });

  test("keeps JSON property names on the normal text color and values on palette accents", () => {
    expect(timelineJsonSyntaxTheme.property?.color).toBe("var(--on-surface)");
    expect(timelineJsonSyntaxTheme.string?.color).toBe("var(--primary)");
    expect(timelineJsonSyntaxTheme.number?.color).toBe("var(--secondary)");
  });
});

function testItem(overrides: Partial<RoomTimelineItem> = {}): RoomTimelineItem {
  return {
    id: "$event",
    transactionId: null,
    senderId: "@alice:example.org",
    senderDisplayName: "Alice",
    senderAvatarUrl: "",
    body: "Body",
    formattedBody: "",
    formattedBodyFormat: null,
    contentKind: "text",
    timestampUnixMs: 1,
    timeLabel: "01:00",
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
    ...overrides,
  };
}
