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
import type { RoomTimelineAttachment } from "../../appShellAdapters";
import { inlineMediaHandle } from "./selection";

describe("inlineMediaHandle", () => {
  test("uses thumbnails for static images and videos", () => {
    expect(inlineMediaHandle(testAttachment({ mediaType: "image" }))).toBe(
      "thumbnail-handle",
    );
    expect(inlineMediaHandle(testAttachment({ mediaType: "video" }))).toBe(
      "thumbnail-handle",
    );
  });

  test("keeps original media for animated images and audio", () => {
    expect(
      inlineMediaHandle(
        testAttachment({ mediaType: "image", mimeType: "image/gif" }),
      ),
    ).toBe("media-handle");
    expect(inlineMediaHandle(testAttachment({ mediaType: "audio" }))).toBe(
      "media-handle",
    );
  });

  test("does not download a static original without a thumbnail", () => {
    expect(
      inlineMediaHandle(
        testAttachment({ mediaType: "image", thumbnailHandle: null }),
      ),
    ).toBeNull();
  });
});

function testAttachment(
  overrides: Partial<RoomTimelineAttachment>,
): RoomTimelineAttachment {
  return {
    eventId: "$media",
    mediaType: "image",
    mediaHandle: "media-handle",
    thumbnailHandle: "thumbnail-handle",
    filename: "media",
    displayCaption: "",
    mimeType: "image/png",
    width: 640,
    height: 480,
    durationUnixMs: null,
    sizeBytes: 1_024,
    blurhash: "",
    requiresReveal: false,
    ...overrides,
  };
}
