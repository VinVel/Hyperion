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

import { invoke } from "@tauri-apps/api/core";
import type { PreparedMedia, SaveRoomMediaResponse } from "./types";

// Keep prepared local URLs across virtualization unmounts so returning to a
// media row does not trigger another Matrix download during the same app run.
const maximumPreparedMediaCacheItems = 64;

const preparedMediaCache = new Map<string, PreparedMedia>();
const pendingPreparedMedia = new Map<string, Promise<PreparedMedia>>();

export function prepareRoomMedia(mediaHandle: string): Promise<PreparedMedia> {
  return invoke<PreparedMedia>("prepare_room_media", {
    request: { media_handle: mediaHandle },
  });
}

export function cachedPreparedRoomMedia(
  cacheScope: string,
  mediaHandle: string,
): Promise<PreparedMedia> {
  const cacheKey = preparedMediaCacheKey(cacheScope, mediaHandle);
  const cachedMedia = preparedMediaCache.get(cacheKey);
  if (cachedMedia) {
    return Promise.resolve(cachedMedia);
  }

  const pendingMedia = pendingPreparedMedia.get(cacheKey);
  if (pendingMedia) {
    return pendingMedia;
  }

  const preparedMedia = prepareRoomMedia(mediaHandle)
    .then((media) => {
      rememberPreparedMedia(cacheKey, media);
      return media;
    })
    .finally(() => {
      pendingPreparedMedia.delete(cacheKey);
    });
  pendingPreparedMedia.set(cacheKey, preparedMedia);
  return preparedMedia;
}

function rememberPreparedMedia(cacheKey: string, preparedMedia: PreparedMedia) {
  preparedMediaCache.set(cacheKey, preparedMedia);
  while (preparedMediaCache.size > maximumPreparedMediaCacheItems) {
    const oldestMediaHandle = preparedMediaCache.keys().next().value;
    if (!oldestMediaHandle) {
      return;
    }
    preparedMediaCache.delete(oldestMediaHandle);
  }
}

function preparedMediaCacheKey(
  cacheScope: string,
  mediaHandle: string,
): string {
  return `${cacheScope}::${mediaHandle}`;
}

export function saveRoomMedia(
  mediaHandle: string,
): Promise<SaveRoomMediaResponse> {
  return invoke<SaveRoomMediaResponse>("save_room_media", {
    request: { media_handle: mediaHandle },
  });
}

export function copyMediaLink(permalink: string): Promise<void> {
  if (!permalink) {
    return Promise.resolve();
  }

  return navigator.clipboard.writeText(permalink);
}
