/**
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
  accountScopedStorageKey,
  cachedRoomThreadsStoragePrefix,
  cachedSpacesStoragePrefix,
  cachedRoomSnapshotsStoragePrefix,
} from "./useAppShellState";

export function readCachedJson<T>(storageKey: string, fallback: T): T {
  try {
    const rawValue = window.localStorage.getItem(storageKey);
    if (!rawValue) {
      return fallback;
    }

    return JSON.parse(rawValue) as T;
  } catch {
    window.localStorage.removeItem(storageKey);
    return fallback;
  }
}
export function writeCachedJson<T>(storageKey: string, value: T): boolean {
  try {
    window.localStorage.setItem(storageKey, JSON.stringify(value));
    return true;
  } catch {
    // A cache failure must not escape a React state updater. The backend SDK is
    // still the timeline authority, so the application remains fully usable.
    return false;
  }
}
export function cachedRoomThreadsKey(accountKey: string): string {
  return accountScopedStorageKey(cachedRoomThreadsStoragePrefix, accountKey);
}
export function cachedSpacesKey(accountKey: string): string {
  return accountScopedStorageKey(cachedSpacesStoragePrefix, accountKey);
}
export function cachedRoomSnapshotKey(
  accountKey: string,
  roomId: string,
): string {
  return accountScopedStorageKey(
    cachedRoomSnapshotsStoragePrefix,
    `${accountKey}.${roomId}`,
  );
}
