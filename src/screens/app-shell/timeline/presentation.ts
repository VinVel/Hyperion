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

import type {
  RoomTimelineItem,
  RoomTimelineReplyPreview,
} from "../appShellAdapters";

export type TimelineContextAction =
  "reply" | "react" | "edit" | "redact" | "copyLink" | "info";

type TimelineReplyCardPresentation = {
  author: string;
  canNavigate: boolean;
  label: string;
};

export type TimelineMessagePresentation = {
  body: string;
  isEdited: boolean;
  isPlaceholder: boolean;
  isTombstone: boolean;
  replyCard: TimelineReplyCardPresentation | null;
  senderDisplayName: string;
  timeLabel: string;
};

// These labels deliberately ignore SDK event bodies when the content is unavailable.
const unavailableContentLabels: Partial<
  Record<RoomTimelineItem["contentKind"], string>
> = {
  pendingDecryption: "Message is waiting to be decrypted",
  unableToDecrypt: "Unable to decrypt this message",
  nonText: "Message type is not supported yet",
  unsupported: "Unsupported message type",
};

export function timelineMessagePresentation(
  item: RoomTimelineItem,
  replyPreview: RoomTimelineReplyPreview | null = item.replyPreview,
): TimelineMessagePresentation {
  const isTombstone = item.isRedacted || item.contentKind === "redacted";
  const unavailableLabel = unavailableContentLabels[item.contentKind];
  const body = isTombstone
    ? "Message removed"
    : (unavailableLabel ?? item.body);

  return {
    body,
    isEdited: item.isEdited,
    isPlaceholder: isTombstone || unavailableLabel !== undefined,
    isTombstone,
    replyCard: replyPreview
      ? {
          author: replyPreview.senderDisplayName,
          canNavigate: replyPreview.state === "resolved",
          label: replyPreviewLabel(replyPreview),
        }
      : null,
    senderDisplayName: item.senderDisplayName,
    timeLabel: item.timeLabel,
  };
}

export function messageContextActions(
  item: RoomTimelineItem,
): TimelineContextAction[] {
  const actions: TimelineContextAction[] = [];
  if (item.canReply) {
    actions.push("reply");
  }
  if (item.canReact) {
    actions.push("react");
  }
  if (item.canEdit) {
    actions.push("edit");
  }
  if (item.canRedact) {
    actions.push("redact");
  }
  if (item.permalink) {
    actions.push("copyLink");
  }

  // Diagnostic detail is safe for every event-backed timeline row.
  actions.push("info");
  return actions;
}

function replyPreviewLabel(replyPreview: RoomTimelineReplyPreview): string {
  if (replyPreview.state === "loading") {
    return "Loading replied message...";
  }
  if (replyPreview.state === "deletedRedacted" || replyPreview.isRedacted) {
    return "Original message deleted";
  }
  if (replyPreview.state === "inaccessible") {
    return "Message not accessible";
  }
  if (replyPreview.state === "failedToLoad") {
    return "Failed to load replied message";
  }
  if (replyPreview.state === "invalidRelation") {
    return "Invalid reply";
  }

  return replyPreview.body;
}
