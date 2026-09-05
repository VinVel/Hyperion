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

// Collection caches remain independent of the SDK-owned visible timeline.
const cachedRoomThreadsStoragePrefix = "hyperion.appShell.roomThreads";
const cachedSpacesStoragePrefix = "hyperion.appShell.spaces";
// Only this legacy namespace stored durable frontend timeline events.
const legacyRoomSnapshotsStoragePrefix = "hyperion.appShell.roomSnapshots.";

function accountScopedStorageKey(prefix: string, accountKey: string): string {
  return `${prefix}.${accountKey}`;
}

export function removeLegacyTimelineSnapshots(
  storage?: Pick<Storage, "length" | "key" | "removeItem">,
): void {
  try {
    const timelineStorage = storage ?? window.localStorage;
    for (let index = timelineStorage.length - 1; index >= 0; index--) {
      const key = timelineStorage.key(index);
      if (key?.startsWith(legacyRoomSnapshotsStoragePrefix))
        timelineStorage.removeItem(key);
    }
  } catch {
    // Storage can be unavailable in a WebView; the timeline does not read it.
  }
}

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
