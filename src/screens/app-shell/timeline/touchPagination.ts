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

// Ignore touch jitter and taps before interpreting a pull as a history request.
const minimumHistoryPullPixels = 8;

type HistoryDrag = {
  identifier: number;
  startX: number;
  startY: number;
  consumed: boolean;
};

export function attachTimelineTouchPagination(
  scroller: HTMLElement,
  onTopScrollIntent: () => void,
): () => void {
  let drag: HistoryDrag | null = null;

  function handleTouchStart(event: TouchEvent) {
    const touch = event.touches[0];
    drag =
      event.touches.length === 1 && touch
        ? {
            identifier: touch.identifier,
            startX: touch.clientX,
            startY: touch.clientY,
            consumed: false,
          }
        : null;
  }

  function handleTouchMove(event: TouchEvent) {
    const touch = event.touches[0];
    if (
      !touch ||
      event.touches.length !== 1 ||
      touch.identifier !== drag?.identifier
    ) {
      drag = null;
      return;
    }
    if (!drag || drag.consumed) return;
    if (scroller.scrollTop > 0) {
      drag.startX = touch.clientX;
      drag.startY = touch.clientY;
      return;
    }
    // A finger moving down requests older content. Native scroll events may
    // stop entirely at the clamped edge, so detect intent from touch movement.
    const deltaY = touch.clientY - drag.startY;
    const deltaX = touch.clientX - drag.startX;
    if (deltaY < minimumHistoryPullPixels || deltaY <= Math.abs(deltaX)) return;
    drag.consumed = true;
    onTopScrollIntent();
  }

  function endDrag() {
    drag = null;
  }

  scroller.addEventListener("touchstart", handleTouchStart, { passive: true });
  scroller.addEventListener("touchmove", handleTouchMove, { passive: true });
  scroller.addEventListener("touchend", endDrag, { passive: true });
  scroller.addEventListener("touchcancel", endDrag, { passive: true });
  return () => {
    endDrag();
    scroller.removeEventListener("touchstart", handleTouchStart);
    scroller.removeEventListener("touchmove", handleTouchMove);
    scroller.removeEventListener("touchend", endDrag);
    scroller.removeEventListener("touchcancel", endDrag);
  };
}
