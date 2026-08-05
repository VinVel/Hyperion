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
  RoomTimelineAttachment,
  RoomTimelineItem,
} from "../../appShellAdapters";
import { attachmentIsPreloadable, inlineMediaHandle } from "./selection";
import type { TimelineMediaItem } from "./types";

export type TimelineMediaPreloadCandidate = {
  mediaHandle: string;
};

type DirectionalPreloadAttachment = {
  attachment: RoomTimelineAttachment;
  distance: number;
};

export function mediaGalleryItems(
  items: RoomTimelineItem[],
): TimelineMediaItem[] {
  return items.flatMap((item) =>
    (item.attachments ?? [])
      .filter(
        (attachment) =>
          attachment.mediaType === "image" ||
          attachment.mediaType === "sticker" ||
          attachment.mediaType === "video",
      )
      .map((attachment) => ({ item, attachment })),
  );
}

export function timelineMediaPreloadCandidates(
  items: RoomTimelineItem[],
  visibleStartIndex: number,
  visibleEndIndex: number,
  mediaAttachmentLimit: number,
): TimelineMediaPreloadCandidate[] {
  const candidates = new Map<string, TimelineMediaPreloadCandidate>();
  const boundedStartIndex = Math.max(0, visibleStartIndex);
  const boundedEndIndex = Math.min(items.length - 1, visibleEndIndex);

  for (
    let itemIndex = boundedStartIndex;
    itemIndex <= boundedEndIndex;
    itemIndex += 1
  ) {
    for (const attachment of items[itemIndex]?.attachments ?? []) {
      addAttachmentPreloadCandidate(candidates, attachment);
    }
  }

  const olderAttachments = nearestMediaAttachments(
    items,
    boundedStartIndex - 1,
    -1,
    mediaAttachmentLimit,
  );
  const newerAttachments = nearestMediaAttachments(
    items,
    boundedEndIndex + 1,
    1,
    mediaAttachmentLimit,
  );
  const nearbyAttachments = [...olderAttachments, ...newerAttachments].sort(
    (firstAttachment, secondAttachment) =>
      firstAttachment.distance - secondAttachment.distance,
  );
  for (const { attachment } of nearbyAttachments) {
    addAttachmentPreloadCandidate(candidates, attachment);
  }

  return [...candidates.values()];
}

function nearestMediaAttachments(
  items: RoomTimelineItem[],
  startIndex: number,
  step: -1 | 1,
  mediaAttachmentLimit: number,
): DirectionalPreloadAttachment[] {
  const attachments: DirectionalPreloadAttachment[] = [];
  for (
    let itemIndex = startIndex;
    itemIndex >= 0 &&
    itemIndex < items.length &&
    attachments.length < mediaAttachmentLimit;
    itemIndex += step
  ) {
    for (const attachment of items[itemIndex]?.attachments ?? []) {
      if (!attachmentIsPreloadable(attachment)) {
        continue;
      }

      attachments.push({
        attachment,
        distance: Math.abs(itemIndex - startIndex),
      });
      if (attachments.length >= mediaAttachmentLimit) {
        break;
      }
    }
  }

  return attachments;
}

function addAttachmentPreloadCandidate(
  candidates: Map<string, TimelineMediaPreloadCandidate>,
  attachment: RoomTimelineAttachment,
) {
  const mediaHandle = inlineMediaHandle(attachment);
  if (!mediaHandle || candidates.has(mediaHandle)) {
    return;
  }

  candidates.set(mediaHandle, { mediaHandle });
}
