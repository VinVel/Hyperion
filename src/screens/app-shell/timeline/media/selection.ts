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

import type { RoomTimelineAttachment } from "../../appShellAdapters";

export function inlineMediaHandle(
  attachment: RoomTimelineAttachment,
): string | null {
  if (attachment.mediaType === "audio") {
    return attachment.mediaHandle;
  }

  if (attachment.mediaType === "sticker") {
    return attachment.mediaHandle || attachment.thumbnailHandle;
  }

  if (attachment.mediaType === "image" && mediaIsAnimatedImage(attachment)) {
    return attachment.mediaHandle;
  }

  if (attachment.mediaType === "image" || attachment.mediaType === "video") {
    return attachment.thumbnailHandle;
  }

  return null;
}

export function attachmentIsPreloadable(
  attachment: RoomTimelineAttachment,
): boolean {
  return inlineMediaHandle(attachment) !== null;
}

function mediaIsAnimatedImage(attachment: RoomTimelineAttachment): boolean {
  const mimeType = attachment.mimeType.toLowerCase();
  return mimeType === "image/gif" || mimeType === "image/webp";
}
