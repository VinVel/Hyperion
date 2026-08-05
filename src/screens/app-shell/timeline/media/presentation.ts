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

// Inline visual media should reserve space from Matrix dimensions but stay
// compact enough for message scanning.
const maximumVisualMediaWidthPixels = 384;
const maximumVisualMediaHeightPixels = 240;
const maximumStickerMediaWidthPixels = 128;
const maximumStickerMediaHeightPixels = 128;

export function mediaAspectRatio(attachment: RoomTimelineAttachment): string {
  if (!attachment.width || !attachment.height) {
    return attachment.mediaType === "sticker" ? "1 / 1" : "16 / 9";
  }

  return `${attachment.width} / ${attachment.height}`;
}

export function mediaReservedWidth(attachment: RoomTimelineAttachment): string {
  const maxWidth =
    attachment.mediaType === "sticker"
      ? maximumStickerMediaWidthPixels
      : maximumVisualMediaWidthPixels;
  const maxHeight =
    attachment.mediaType === "sticker"
      ? maximumStickerMediaHeightPixels
      : maximumVisualMediaHeightPixels;
  if (!attachment.width || !attachment.height) {
    return `${maxWidth}px`;
  }

  const widthFromHeight = maxHeight * (attachment.width / attachment.height);
  return `${Math.min(maxWidth, Math.max(1, widthFromHeight))}px`;
}

export function formatBytes(sizeBytes: number | null): string {
  if (!sizeBytes) {
    return "";
  }

  if (sizeBytes < 1024) {
    return `${sizeBytes} B`;
  }

  if (sizeBytes < 1024 * 1024) {
    return `${Math.round(sizeBytes / 1024)} KB`;
  }

  return `${(sizeBytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatDimensions(attachment: RoomTimelineAttachment): string {
  if (!attachment.width || !attachment.height) {
    return "";
  }

  return `${attachment.width} x ${attachment.height}`;
}

export function formatDuration(durationUnixMs: number | null): string {
  if (!durationUnixMs) {
    return "";
  }

  const totalSeconds = Math.round(durationUnixMs / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = String(totalSeconds % 60).padStart(2, "0");
  return `${minutes}:${seconds}`;
}
