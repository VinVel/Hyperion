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

import { useEffect, type RefObject } from "react";
import type { RoomTimeline } from "../appShellAdapters";
import { saveTimelineBookmark } from "./bookmarks";

export function useTimelineBookmark(
  rootRef: RefObject<HTMLDivElement | null>,
  timeline: RoomTimeline | null,
  bottomTolerancePixels: number,
): void {
  const accountKey = timeline?.timelineIdentity.accountKey;
  const roomId = timeline?.roomId;
  const focusedEventId = timeline?.focusedEventId ?? null;
  const hasItems = Boolean(timeline?.items.length);

  useEffect(() => {
    const root = rootRef.current;
    const scroller = root?.querySelector<HTMLDivElement>(
      ".room-timeline-scroller",
    );
    if (!accountKey || !roomId || !scroller) return;
    const selection = { accountKey, roomId, focusedEventId };
    const viewport = scroller;
    let interacted = false;
    let frameId: number | null = null;

    function markInteraction() {
      interacted = true;
    }
    function capturePosition() {
      frameId = null;
      const viewportRect = viewport.getBoundingClientRect();
      const rows = viewport.querySelectorAll<HTMLElement>("[data-event-id]");
      for (const row of rows) {
        const rowRect = row.getBoundingClientRect();
        if (
          rowRect.bottom <= viewportRect.top ||
          rowRect.top >= viewportRect.bottom
        )
          continue;
        const eventId = row.dataset.eventId;
        if (!eventId) continue;
        saveTimelineBookmark(selection, {
          eventId,
          offsetPixels: rowRect.top - viewportRect.top,
          wasAtBottom:
            viewport.scrollHeight -
              viewport.scrollTop -
              viewport.clientHeight <=
            bottomTolerancePixels,
        });
        break;
      }
    }
    function handleScroll() {
      // Initial/programmatic positioning must not overwrite a saved bookmark
      // before the measured restoration path has had a chance to use it.
      if (interacted && frameId === null)
        frameId = requestAnimationFrame(capturePosition);
    }
    viewport.addEventListener("wheel", markInteraction, { passive: true });
    viewport.addEventListener("touchstart", markInteraction, { passive: true });
    viewport.addEventListener("pointerdown", markInteraction, {
      passive: true,
    });
    viewport.addEventListener("keydown", markInteraction);
    viewport.addEventListener("scroll", handleScroll, { passive: true });
    return () => {
      if (frameId !== null) cancelAnimationFrame(frameId);
      viewport.removeEventListener("wheel", markInteraction);
      viewport.removeEventListener("touchstart", markInteraction);
      viewport.removeEventListener("pointerdown", markInteraction);
      viewport.removeEventListener("keydown", markInteraction);
      viewport.removeEventListener("scroll", handleScroll);
    };
  }, [
    accountKey,
    roomId,
    focusedEventId,
    hasItems,
    rootRef,
    bottomTolerancePixels,
  ]);
}
