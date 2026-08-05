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

// Background thumbnails stay below the visible-media lane so quick viewport
// changes cannot flood WebKitGTK with IPC completions and image decodes.
const maximumConcurrentMediaPreloads = 2;

const preparedMediaCache = new Map<string, PreparedMedia>();
const pendingPreparedMedia = new Map<string, PendingPreparedMedia>();
const queuedPreloadMedia = new Map<string, QueuedPreloadMedia>();
let activeMediaPreloadCount = 0;

type PreparedMediaPriority = "visible" | "preload";

type CachedPreparedRoomMediaOptions = {
  priority?: PreparedMediaPriority;
};

type PendingPreparedMedia = {
  promise: Promise<PreparedMedia>;
};

type QueuedPreloadMedia = {
  cacheScope: string;
  mediaHandle: string;
  promise: Promise<PreparedMedia>;
  reject: (error: Error) => void;
  resolve: (media: PreparedMedia) => void;
};

// Canceled queue entries reject their existing callers without being reported
// as media failures by the visible lane.
const canceledPreloadError = new Error("Media preload canceled");

export function prepareRoomMedia(mediaHandle: string): Promise<PreparedMedia> {
  return invoke<PreparedMedia>("prepare_room_media", {
    request: { media_handle: mediaHandle },
  });
}

export function cachedPreparedRoomMedia(
  cacheScope: string,
  mediaHandle: string,
  options: CachedPreparedRoomMediaOptions = {},
): Promise<PreparedMedia> {
  const cacheKey = preparedMediaCacheKey(cacheScope, mediaHandle);
  const cachedMedia = preparedMediaCache.get(cacheKey);
  if (cachedMedia) {
    return Promise.resolve(cachedMedia);
  }

  const pendingMedia = pendingPreparedMedia.get(cacheKey);
  if (pendingMedia) {
    return pendingMedia.promise;
  }

  if (options.priority === "preload") {
    return queuedPreparedRoomMedia(cacheScope, mediaHandle, cacheKey);
  }

  const queuedMedia = queuedPreloadMedia.get(cacheKey);
  if (queuedMedia) {
    queuedPreloadMedia.delete(cacheKey);
    queuedMedia.reject(canceledPreloadError);
  }

  return startPreparedMedia(cacheKey, mediaHandle);
}

function startPreparedMedia(
  cacheKey: string,
  mediaHandle: string,
): Promise<PreparedMedia> {
  const preparedMedia = prepareRoomMedia(mediaHandle)
    .then((media) => {
      rememberPreparedMedia(cacheKey, media);
      return media;
    })
    .finally(() => {
      const pendingMedia = pendingPreparedMedia.get(cacheKey);
      if (pendingMedia?.promise === preparedMedia) {
        pendingPreparedMedia.delete(cacheKey);
      }
    });
  pendingPreparedMedia.set(cacheKey, { promise: preparedMedia });
  return preparedMedia;
}

export function cancelQueuedMediaPreloads(cacheScope: string) {
  for (const [cacheKey, preloadMedia] of queuedPreloadMedia) {
    if (preloadMedia.cacheScope !== cacheScope) {
      continue;
    }

    queuedPreloadMedia.delete(cacheKey);
    preloadMedia.reject(canceledPreloadError);
  }
}

function queuedPreparedRoomMedia(
  cacheScope: string,
  mediaHandle: string,
  cacheKey: string,
): Promise<PreparedMedia> {
  const queuedMedia = queuedPreloadMedia.get(cacheKey);
  if (queuedMedia) {
    return queuedMedia.promise;
  }

  let resolveQueuedMedia: (media: PreparedMedia) => void = () => {};
  let rejectQueuedMedia: (error: Error) => void = () => {};
  const promise = new Promise<PreparedMedia>((resolve, reject) => {
    resolveQueuedMedia = resolve;
    rejectQueuedMedia = reject;
  });
  queuedPreloadMedia.set(cacheKey, {
    cacheScope,
    mediaHandle,
    promise,
    reject: rejectQueuedMedia,
    resolve: resolveQueuedMedia,
  });
  drainMediaPreloadQueue();
  return promise;
}

function drainMediaPreloadQueue() {
  while (
    activeMediaPreloadCount < maximumConcurrentMediaPreloads &&
    queuedPreloadMedia.size > 0
  ) {
    const nextCacheKey = queuedPreloadMedia.keys().next().value;
    if (!nextCacheKey) {
      return;
    }

    const queuedMedia = queuedPreloadMedia.get(nextCacheKey);
    if (!queuedMedia) {
      queuedPreloadMedia.delete(nextCacheKey);
      continue;
    }

    queuedPreloadMedia.delete(nextCacheKey);
    activeMediaPreloadCount += 1;
    const preparedMedia = prepareRoomMedia(queuedMedia.mediaHandle)
      .then((media) => {
        rememberPreparedMedia(nextCacheKey, media);
        queuedMedia.resolve(media);
        return media;
      })
      .catch((error: unknown) => {
        queuedMedia.reject(mediaPreparationError(error));
        throw error;
      })
      .finally(() => {
        const pendingMedia = pendingPreparedMedia.get(nextCacheKey);
        if (pendingMedia?.promise === preparedMedia) {
          pendingPreparedMedia.delete(nextCacheKey);
        }
        activeMediaPreloadCount -= 1;
        drainMediaPreloadQueue();
      });
    pendingPreparedMedia.set(nextCacheKey, { promise: preparedMedia });
    void preparedMedia.catch(() => {});
  }
}

function mediaPreparationError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
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
