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

import type { TimelineSession } from "./model";

export type TimelineBookmark = {
  eventId: string;
  offsetPixels: number;
  wasAtBottom: boolean;
};

// Session-only reading positions are independent of SDK instances and contain
// no timeline events. They deliberately never enter browser storage.
const timelineBookmarks = new Map<string, TimelineBookmark>();

function bookmarkKey(selection: TimelineSession): string {
  return JSON.stringify([
    selection.accountKey,
    selection.roomId,
    selection.focusedEventId,
  ]);
}

export function saveTimelineBookmark(
  selection: TimelineSession,
  bookmark: TimelineBookmark,
): void {
  timelineBookmarks.set(bookmarkKey(selection), { ...bookmark });
}

export function readTimelineBookmark(
  selection: TimelineSession,
): TimelineBookmark | null {
  const bookmark = timelineBookmarks.get(bookmarkKey(selection));
  return bookmark ? { ...bookmark } : null;
}
