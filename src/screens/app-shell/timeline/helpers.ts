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
} from "../appShellAdapters";
import { ShellTimelineUpdatedPayload, TimelineJumpTarget } from "../types";

// Local echo timestamps and remote event timestamps can differ slightly, so
// reconcile provisional own messages within this bounded send window.
const localEchoReconciliationWindowMilliseconds = 120_000;

export function canonicalizeTimelineItems(
  items: RoomTimelineItem[],
): RoomTimelineItem[] {
  const confirmedItems = items.filter(isConfirmedOwnRemoteEvent);
  const seenItemIds = new Set<string>();

  return items.filter((item) => {
    if (seenItemIds.has(item.id)) {
      return false;
    }
    seenItemIds.add(item.id);

    if (!isTransientLocalEcho(item)) {
      return true;
    }

    return !confirmedItems.some((confirmedItem) =>
      timelineItemsRepresentSameSend(item, confirmedItem),
    );
  });
}

export function mergeOlderTimelineItems(
  currentItems: RoomTimelineItem[],
  olderItems: RoomTimelineItem[],
): RoomTimelineItem[] {
  return mergeOlderTimelineItemsWithCounts(currentItems, olderItems).items;
}

export function mergeOlderTimelineItemsWithCounts(
  currentItems: RoomTimelineItem[],
  olderItems: RoomTimelineItem[],
): {
  duplicateCount: number;
  insertedCount: number;
  items: RoomTimelineItem[];
} {
  const currentTimelineItems = currentItems ?? [];
  const olderTimelineItems = olderItems ?? [];
  const seenItemIds = new Set(currentTimelineItems.map((item) => item.id));
  const uniqueOlderItems = olderTimelineItems.filter(
    (item) => !seenItemIds.has(item.id),
  );

  const items = canonicalizeTimelineItems([
    ...uniqueOlderItems,
    ...currentTimelineItems,
  ]);

  return {
    duplicateCount: olderTimelineItems.length - uniqueOlderItems.length,
    insertedCount: uniqueOlderItems.length,
    items,
  };
}

export function mergeTimelineRefresh(
  currentTimeline: RoomTimeline | null,
  refreshedTimeline: RoomTimeline,
): RoomTimeline {
  if (
    !currentTimeline ||
    currentTimeline.roomId !== refreshedTimeline.roomId ||
    refreshedTimeline.focusedEventId
  ) {
    return refreshedTimeline;
  }

  const currentItems = canonicalizeTimelineItems(currentTimeline.items ?? []);
  const refreshedItems = reuseUnchangedTimelineItems(
    currentItems,
    canonicalizeTimelineItems(refreshedTimeline.items ?? []),
  );
  const redactedItemIds = new Set(refreshedTimeline.redactedEventIds ?? []);
  if (!refreshedItems.length && currentItems.length) {
    const preservedTimeline = {
      ...refreshedTimeline,
      items: currentItems.filter((item) => !redactedItemIds.has(item.id)),
      nextBefore: currentTimeline.nextBefore ?? refreshedTimeline.nextBefore,
    };
    return reuseUnchangedTimeline(currentTimeline, preservedTimeline);
  }

  const refreshedItemIds = new Set(refreshedItems.map((item) => item.id));
  const firstOverlapIndex = currentItems.findIndex((item) =>
    refreshedItemIds.has(item.id),
  );

  if (firstOverlapIndex < 0) {
    return {
      ...refreshedTimeline,
      items: refreshedItems,
    };
  }

  const olderPrefixItems = currentItems
    .slice(0, firstOverlapIndex)
    .filter(
      (item) =>
        !redactedItemIds.has(item.id) &&
        !refreshedItemIds.has(item.id) &&
        !hasReconciledRemoteItem(item, refreshedItems),
    );

  const mergedTimeline: RoomTimeline = {
    ...refreshedTimeline,
    items: [...olderPrefixItems, ...refreshedItems],
    nextBefore: currentTimeline.nextBefore ?? refreshedTimeline.nextBefore,
  };
  const canonicalTimeline = {
    ...mergedTimeline,
    items: canonicalizeTimelineItems(mergedTimeline.items),
  };
  return reuseUnchangedTimeline(currentTimeline, canonicalTimeline);
}

function reuseUnchangedTimelineItems(
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

function reuseUnchangedTimeline(
  currentTimeline: RoomTimeline,
  refreshedTimeline: RoomTimeline,
): RoomTimeline {
  const itemsAreUnchanged =
    currentTimeline.items.length === refreshedTimeline.items.length &&
    currentTimeline.items.every(
      (item, index) => item === refreshedTimeline.items[index],
    );
  const redactedEventIdsAreUnchanged = stringArraysAreEqual(
    currentTimeline.redactedEventIds,
    refreshedTimeline.redactedEventIds,
  );
  if (
    itemsAreUnchanged &&
    redactedEventIdsAreUnchanged &&
    currentTimeline.roomId === refreshedTimeline.roomId &&
    currentTimeline.nextBefore === refreshedTimeline.nextBefore &&
    currentTimeline.focusedEventId === refreshedTimeline.focusedEventId
  ) {
    return currentTimeline;
  }

  return refreshedTimeline;
}

function timelineItemsAreEqual(
  currentItem: RoomTimelineItem,
  refreshedItem: RoomTimelineItem,
): boolean {
  return (
    currentItem.id === refreshedItem.id &&
    currentItem.transactionId === refreshedItem.transactionId &&
    currentItem.senderId === refreshedItem.senderId &&
    currentItem.senderDisplayName === refreshedItem.senderDisplayName &&
    currentItem.senderAvatarUrl === refreshedItem.senderAvatarUrl &&
    currentItem.body === refreshedItem.body &&
    currentItem.formattedBody === refreshedItem.formattedBody &&
    currentItem.formattedBodyFormat === refreshedItem.formattedBodyFormat &&
    currentItem.richText === refreshedItem.richText &&
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

function stringArraysAreEqual(
  currentValues: string[],
  refreshedValues: string[],
): boolean {
  return (
    currentValues.length === refreshedValues.length &&
    currentValues.every((value, index) => value === refreshedValues[index])
  );
}

function isRemoteEventId(eventId: string | null | undefined): boolean {
  return eventId?.startsWith("$") === true;
}

function isTransientLocalEcho(item: RoomTimelineItem): boolean {
  return item.isOwnMessage && !isRemoteEventId(item.id);
}

function isConfirmedOwnRemoteEvent(item: RoomTimelineItem): boolean {
  return (
    item.isOwnMessage && isRemoteEventId(item.id) && item.sendState === "sent"
  );
}

function timelineItemsRepresentSameSend(
  localEcho: RoomTimelineItem,
  confirmedItem: RoomTimelineItem,
): boolean {
  if (
    !isTransientLocalEcho(localEcho) ||
    !isConfirmedOwnRemoteEvent(confirmedItem)
  ) {
    return false;
  }

  if (
    localEcho.transactionId &&
    confirmedItem.transactionId &&
    localEcho.transactionId === confirmedItem.transactionId
  ) {
    return true;
  }

  const timestampDelta = Math.abs(
    localEcho.timestampUnixMs - confirmedItem.timestampUnixMs,
  );
  return (
    localEcho.senderId === confirmedItem.senderId &&
    localEcho.body === confirmedItem.body &&
    timestampDelta <= localEchoReconciliationWindowMilliseconds
  );
}

function isProvisionalLocalEcho(item: RoomTimelineItem): boolean {
  return (
    item.isOwnMessage &&
    (item.sendState === "pending" ||
      item.sendState === "sending" ||
      item.sendState === "retrying")
  );
}

function canReconcileLocalEcho(
  localEcho: RoomTimelineItem,
  refreshedItem: RoomTimelineItem,
): boolean {
  if (
    !isProvisionalLocalEcho(localEcho) ||
    refreshedItem.sendState !== "sent"
  ) {
    return false;
  }

  if (
    localEcho.transactionId &&
    refreshedItem.transactionId &&
    localEcho.transactionId === refreshedItem.transactionId
  ) {
    return true;
  }

  const timestampDelta = Math.abs(
    localEcho.timestampUnixMs - refreshedItem.timestampUnixMs,
  );
  return (
    localEcho.isOwnMessage === refreshedItem.isOwnMessage &&
    localEcho.senderId === refreshedItem.senderId &&
    localEcho.body === refreshedItem.body &&
    timestampDelta <= localEchoReconciliationWindowMilliseconds
  );
}

function hasReconciledRemoteItem(
  localEcho: RoomTimelineItem,
  refreshedItems: RoomTimelineItem[],
): boolean {
  return refreshedItems.some((refreshedItem) =>
    canReconcileLocalEcho(localEcho, refreshedItem),
  );
}
export function emptyRoomTimeline(roomId: string): RoomTimeline {
  return {
    roomId,
    items: [],
    nextBefore: null,
    focusedEventId: null,
    redactedEventIds: [],
  };
}
export function normalizeRoomTimeline(timeline: RoomTimeline): RoomTimeline {
  return {
    ...timeline,
    items: canonicalizeTimelineItems(
      (timeline.items ?? []).map(normalizeRoomTimelineItem),
    ),
    nextBefore: timeline.nextBefore ?? null,
    focusedEventId: timeline.focusedEventId ?? null,
    redactedEventIds: timeline.redactedEventIds ?? [],
  };
}
function isDurableTimelineItem(item: RoomTimelineItem): boolean {
  return isRemoteEventId(item.id);
}
export function durableRoomTimeline(timeline: RoomTimeline): RoomTimeline {
  return {
    ...timeline,
    items: timeline.items.filter(isDurableTimelineItem),
  };
}

function normalizeRoomTimelineItem(item: RoomTimelineItem): RoomTimelineItem {
  const senderId = item.senderId ?? "";
  return {
    ...item,
    id: item.id ?? item.transactionId ?? "",
    transactionId: item.transactionId ?? null,
    senderId,
    senderDisplayName: item.senderDisplayName ?? senderId,
    senderAvatarUrl: item.senderAvatarUrl ?? "",
    body: item.body ?? "",
    formattedBody: item.formattedBody ?? "",
    formattedBodyFormat: item.formattedBodyFormat ?? null,
    richText: item.richText ?? null,
    contentKind: item.contentKind ?? "text",
    timestampUnixMs: item.timestampUnixMs ?? 0,
    timeLabel: item.timeLabel ?? "",
    isEdited: item.isEdited ?? false,
    isRedacted: item.isRedacted ?? false,
    isOwnMessage: item.isOwnMessage ?? false,
    sendState: item.sendState ?? "sent",
    decryptionState: item.decryptionState ?? "unencrypted",
    groupPosition: item.groupPosition ?? "standalone",
    permalink: item.permalink ?? "",
    canEdit: item.canEdit ?? false,
    canRedact: item.canRedact ?? false,
    canReply: item.canReply ?? false,
    canReact: item.canReact ?? false,
    reactions: item.reactions ?? [],
    receipts: item.receipts ?? [],
    thread: item.thread ?? null,
    threadReplyTo: item.threadReplyTo ?? null,
    replyPreview: item.replyPreview ?? null,
  };
}
export function roomTimelineFromUpdatePayload(
  payload: ShellTimelineUpdatedPayload,
): RoomTimeline {
  return normalizeRoomTimeline(
    mapRoomTimeline({
      room_id: payload.room_id,
      items: payload.items,
      next_before: null,
      focused_event_id: null,
      redacted_event_ids: payload.redacted_event_ids,
    }),
  );
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
