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

import type { RoomTimelineItem } from "../../appShellAdapters";
import type { TimelineMediaItem } from "./types";

export function mediaGalleryItems(
  items: RoomTimelineItem[],
): TimelineMediaItem[] {
  return items.flatMap((item) =>
    (item.attachments ?? [])
      .filter(
        (attachment) =>
          attachment.mediaType === "image" ||
          attachment.mediaType === "sticker",
      )
      .map((attachment) => ({ item, attachment })),
  );
}

export function preloadableTimelineMediaHandles(
  items: RoomTimelineItem[],
): string[] {
  const mediaHandles = new Set<string>();
  for (const item of items) {
    for (const attachment of item.attachments ?? []) {
      if (
        attachment.mediaType !== "audio" &&
        attachment.mediaType !== "image" &&
        attachment.mediaType !== "sticker" &&
        attachment.mediaType !== "video"
      ) {
        continue;
      }

      mediaHandles.add(attachment.mediaHandle);
    }
  }

  return [...mediaHandles];
}
