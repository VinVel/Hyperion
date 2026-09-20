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

import { survivingAnchor } from "./viewport";
import type { RoomTimeline } from "../appShellAdapters";
import type { TimelineSession } from "./model";
import { readTimelineBookmark, type TimelineBookmark } from "./bookmarks";

export type ReadingPosition = {
  selection: TimelineSession;
  bookmark: TimelineBookmark;
};

export async function resolveReadingPosition(
  selection: TimelineSession,
  initial: RoomTimeline,
  loadContext: (eventId: string) => Promise<RoomTimeline>,
): Promise<RoomTimeline> {
  const bookmark = readTimelineBookmark(selection);
  if (!bookmark || (bookmark.wasAtBottom && !selection.focusedEventId))
    return initial;
  let timeline = initial;
  if (!timeline.items.some((item) => item.id === bookmark.eventId)) {
    try {
      timeline = await loadContext(bookmark.eventId);
    } catch (error) {
      // A removed event may no longer have retrievable context. Ask the SDK for
      // one saved neighbor; never resurrect an obsolete projection or loop through history.
      const neighbor = bookmark.nearby?.[0];
      if (!neighbor) throw error;
      timeline = await loadContext(neighbor.eventId);
    }
  }
  const anchor = survivingAnchor(
    [bookmark, ...(bookmark.nearby ?? [])],
    timeline.items,
  );
  if (!anchor) throw new Error("The saved message is no longer available.");
  return {
    ...timeline,
    readingPosition: { selection, bookmark: { ...bookmark, ...anchor } },
  };
}
