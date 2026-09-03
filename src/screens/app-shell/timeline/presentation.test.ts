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
import type {
  RoomTimelineItem,
  RoomTimelineReplyPreview,
} from "../appShellAdapters";
import {
  messageContextActions,
  timelineMessagePresentation,
} from "./presentation";

describe("timeline message presentation", () => {
  test("renders placeholders with metadata and never their unavailable body", () => {
    const item = testTimelineItem({
      body: "secret encrypted content",
      contentKind: "pendingDecryption",
      decryptionState: "pending",
      senderDisplayName: "Alice",
      timeLabel: "09:41",
    });

    const presentation = timelineMessagePresentation(item);

    expect(presentation.body).toBe("Message is waiting to be decrypted");
    expect(presentation.body).not.toContain("secret encrypted content");
    expect(presentation.senderDisplayName).toBe("Alice");
    expect(presentation.timeLabel).toBe("09:41");
  });

  test.each([
    ["pendingDecryption", "Message is waiting to be decrypted"],
    ["unableToDecrypt", "Unable to decrypt this message"],
    ["nonText", "Message type is not supported yet"],
    ["unsupported", "Unsupported message type"],
  ] as const)("uses the safe %s placeholder label", (contentKind, label) => {
    expect(
      timelineMessagePresentation(
        testTimelineItem({ contentKind, body: "unavailable body" }),
      ).body,
    ).toBe(label);
  });

  test("renders redactions as a tombstone without original content", () => {
    const presentation = timelineMessagePresentation(
      testTimelineItem({
        body: "removed confidential message",
        contentKind: "redacted",
        isRedacted: true,
      }),
    );

    expect(presentation.body).toBe("Message removed");
    expect(presentation.body).not.toContain("confidential");
    expect(presentation.isTombstone).toBe(true);
  });

  test("uses backend capability flags for placeholder message actions", () => {
    const placeholder = testTimelineItem({
      contentKind: "unsupported",
      canEdit: false,
      canReact: false,
      canRedact: false,
      canReply: false,
    });

    expect(messageContextActions(placeholder)).toEqual(["info"]);
  });

  test("does not override explicit backend placeholder capabilities", () => {
    const placeholder = testTimelineItem({
      contentKind: "unsupported",
      canReply: true,
      canReact: true,
    });

    expect(messageContextActions(placeholder)).toEqual([
      "reply",
      "react",
      "info",
    ]);
  });

  test.each([
    ["loading", "Loading replied message...", false],
    ["deletedRedacted", "Original message deleted", false],
    ["inaccessible", "Message not accessible", false],
    ["failedToLoad", "Failed to load replied message", false],
    ["invalidRelation", "Invalid reply", false],
    ["resolved", "Visible target", true],
  ] as const)(
    "keeps a stable %s reply card and only enables available navigation",
    (state, label, canNavigate) => {
      const replyPreview: RoomTimelineReplyPreview = {
        eventId: "$target",
        state,
        senderId: "@bob:example.org",
        senderDisplayName: "Bob",
        body: "Visible target",
        isRedacted: false,
      };

      const presentation = timelineMessagePresentation(
        testTimelineItem(),
        replyPreview,
      );

      expect(presentation.replyCard).toMatchObject({ label, canNavigate });
    },
  );

  test("keeps the canonical edited body and marker in one row", () => {
    const presentation = timelineMessagePresentation(
      testTimelineItem({ body: "latest canonical body", isEdited: true }),
    );

    expect(presentation.body).toBe("latest canonical body");
    expect(presentation.isEdited).toBe(true);
  });

  test("includes Info in the existing message context action list", () => {
    expect(messageContextActions(testTimelineItem())).toContain("info");
  });
});

function testTimelineItem(
  overrides: Partial<RoomTimelineItem> = {},
): RoomTimelineItem {
  return {
    id: "$event",
    transactionId: null,
    senderId: "@alice:example.org",
    senderDisplayName: "Alice",
    senderAvatarUrl: "",
    body: "Message body",
    formattedBody: "",
    contentKind: "text",
    timestampUnixMs: 1,
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
    ...overrides,
  };
}
