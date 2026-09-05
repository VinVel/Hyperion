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

import {
  mapRoomTimeline,
  type RoomTimeline,
  type RoomTimelineItem,
  type BackendTimelineRichTextNode,
} from "../appShellAdapters";
import type { ShellTimelineUpdatedPayload, TimelineJumpTarget } from "../types";

export function reuseUnchangedTimelineItems(
  currentItems: RoomTimelineItem[],
  refreshedItems: RoomTimelineItem[],
): RoomTimelineItem[] {
  const currentItemsById = new Map(
    currentItems.map((item) => [item.id, item] as const),
  );

  return refreshedItems.map((refreshedItem) => {
    const currentItem = currentItemsById.get(refreshedItem.id);
    if (currentItem && timelineItemsAreEqual(currentItem, refreshedItem)) {
      return currentItem;
    }

    return refreshedItem;
  });
}

function timelineItemsAreEqual(
  currentItem: RoomTimelineItem,
  refreshedItem: RoomTimelineItem,
): boolean {
  return (
    currentItem.id === refreshedItem.id &&
    currentItem.transactionId === refreshedItem.transactionId &&
    currentItem.senderId === refreshedItem.senderId &&
    currentItem.roomId === refreshedItem.roomId &&
    currentItem.senderDisplayName === refreshedItem.senderDisplayName &&
    currentItem.senderAvatarUrl === refreshedItem.senderAvatarUrl &&
    currentItem.body === refreshedItem.body &&
    currentItem.formattedBody === refreshedItem.formattedBody &&
    currentItem.formattedBodyFormat === refreshedItem.formattedBodyFormat &&
    richTextIsEqual(currentItem.richText, refreshedItem.richText) &&
    currentItem.contentKind === refreshedItem.contentKind &&
    currentItem.timestampUnixMs === refreshedItem.timestampUnixMs &&
    currentItem.timeLabel === refreshedItem.timeLabel &&
    currentItem.isEdited === refreshedItem.isEdited &&
    currentItem.isRedacted === refreshedItem.isRedacted &&
    currentItem.isOwnMessage === refreshedItem.isOwnMessage &&
    currentItem.sendState === refreshedItem.sendState &&
    currentItem.decryptionState === refreshedItem.decryptionState &&
    currentItem.groupPosition === refreshedItem.groupPosition &&
    currentItem.permalink === refreshedItem.permalink &&
    currentItem.canEdit === refreshedItem.canEdit &&
    currentItem.canRedact === refreshedItem.canRedact &&
    currentItem.canReply === refreshedItem.canReply &&
    currentItem.canReact === refreshedItem.canReact &&
    reactionsAreEqual(currentItem.reactions, refreshedItem.reactions) &&
    receiptsAreEqual(currentItem.receipts, refreshedItem.receipts) &&
    threadRelationsAreEqual(currentItem.thread, refreshedItem.thread) &&
    threadReplyRelationsAreEqual(
      currentItem.threadReplyTo,
      refreshedItem.threadReplyTo,
    ) &&
    replyPreviewsAreEqual(currentItem.replyPreview, refreshedItem.replyPreview)
  );
}

function threadRelationsAreEqual(
  currentThread: RoomTimelineItem["thread"],
  refreshedThread: RoomTimelineItem["thread"],
): boolean {
  if (!currentThread || !refreshedThread) {
    return currentThread === refreshedThread;
  }

  return (
    currentThread.rootEventId === refreshedThread.rootEventId &&
    currentThread.latestEventId === refreshedThread.latestEventId &&
    currentThread.replyCount === refreshedThread.replyCount
  );
}

function threadReplyRelationsAreEqual(
  currentThreadReply: RoomTimelineItem["threadReplyTo"],
  refreshedThreadReply: RoomTimelineItem["threadReplyTo"],
): boolean {
  if (!currentThreadReply || !refreshedThreadReply) {
    return currentThreadReply === refreshedThreadReply;
  }

  return currentThreadReply.rootEventId === refreshedThreadReply.rootEventId;
}

function reactionsAreEqual(
  currentReactions: RoomTimelineItem["reactions"],
  refreshedReactions: RoomTimelineItem["reactions"],
): boolean {
  return (
    currentReactions.length === refreshedReactions.length &&
    currentReactions.every((reaction, index) => {
      const refreshedReaction = refreshedReactions[index];
      return (
        refreshedReaction !== undefined &&
        reaction.key === refreshedReaction.key &&
        reaction.count === refreshedReaction.count &&
        reaction.reactedByMe === refreshedReaction.reactedByMe
      );
    })
  );
}

function receiptsAreEqual(
  currentReceipts: RoomTimelineItem["receipts"],
  refreshedReceipts: RoomTimelineItem["receipts"],
): boolean {
  return (
    currentReceipts.length === refreshedReceipts.length &&
    currentReceipts.every((receipt, index) => {
      const refreshedReceipt = refreshedReceipts[index];
      return (
        refreshedReceipt !== undefined &&
        receipt.userId === refreshedReceipt.userId &&
        receipt.displayName === refreshedReceipt.displayName &&
        receipt.avatarUrl === refreshedReceipt.avatarUrl &&
        receipt.timestampUnixMs === refreshedReceipt.timestampUnixMs
      );
    })
  );
}

function replyPreviewsAreEqual(
  currentReplyPreview: RoomTimelineItem["replyPreview"],
  refreshedReplyPreview: RoomTimelineItem["replyPreview"],
): boolean {
  if (!currentReplyPreview || !refreshedReplyPreview) {
    return currentReplyPreview === refreshedReplyPreview;
  }

  return (
    currentReplyPreview.eventId === refreshedReplyPreview.eventId &&
    currentReplyPreview.state === refreshedReplyPreview.state &&
    currentReplyPreview.senderId === refreshedReplyPreview.senderId &&
    currentReplyPreview.senderDisplayName ===
      refreshedReplyPreview.senderDisplayName &&
    currentReplyPreview.body === refreshedReplyPreview.body &&
    currentReplyPreview.isRedacted === refreshedReplyPreview.isRedacted
  );
}

// Rich text is deserialized on every snapshot, so reference equality alone
// would replace every formatted row even when the SDK content is unchanged.
function richTextIsEqual(
  left: RoomTimelineItem["richText"],
  right: RoomTimelineItem["richText"],
): boolean {
  if (left === right) return true;
  if (!left || !right || left.length !== right.length) return false;
  return left.every((node, index) => richTextNodeIsEqual(node, right[index]!));
}

function richTextNodeIsEqual(
  left: BackendTimelineRichTextNode,
  right: BackendTimelineRichTextNode,
): boolean {
  if (left.type === "text")
    return right.type === "text" && left.text === right.text;
  if (right.type !== "element" || left.tag !== right.tag) return false;
  const attributeNames = Object.keys(
    left.attributes,
  ) as (keyof typeof left.attributes)[];
  return (
    attributeNames.length === Object.keys(right.attributes).length &&
    attributeNames.every(
      (name) => left.attributes[name] === right.attributes[name],
    ) &&
    richTextIsEqual(left.children, right.children)
  );
}

export function roomTimelineFromUpdatePayload(
  payload: ShellTimelineUpdatedPayload,
): RoomTimeline {
  return mapRoomTimeline({ ...payload, next_before: null });
}

export function timelineAnchorForRoom(
  roomId: string,
  timelineJumpTarget: TimelineJumpTarget | null,
): string | null {
  if (timelineJumpTarget?.roomId !== roomId) {
    return null;
  }

  if (timelineJumpTarget.eventId.trim().length === 0) {
    return null;
  }

  return timelineJumpTarget.eventId;
}
