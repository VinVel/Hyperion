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

export type ViewportAnchor = { eventId: string; offsetPixels: number };
type Row = { id: string };

export function classifyTimelineChange(
  previous: readonly Row[],
  next: readonly Row[],
) {
  const added = next.length - previous.length;
  if (added === 0 && previous.every((row, i) => row.id === next[i]?.id))
    return "same";
  if (added > 0 && previous.length > 0) {
    if (previous.every((row, i) => row.id === next[i + added]?.id))
      return "prepend";
    if (previous.every((row, i) => row.id === next[i]?.id)) return "append";
  }
  return "structural";
}

export function survivingAnchor(
  visible: readonly ViewportAnchor[],
  next: readonly Row[],
): ViewportAnchor | null {
  const ids = new Set(next.map((row) => row.id));
  return visible.find((anchor) => ids.has(anchor.eventId)) ?? null;
}

export function captureVisibleAnchors(scroller: HTMLElement): ViewportAnchor[] {
  const viewport = scroller.getBoundingClientRect();
  return Array.from(scroller.querySelectorAll<HTMLElement>("[data-event-id]"))
    .filter((row) => {
      const rect = row.getBoundingClientRect();
      return rect.bottom > viewport.top && rect.top < viewport.bottom;
    })
    .map((row) => ({
      eventId: row.dataset.eventId!,
      offsetPixels: row.getBoundingClientRect().top - viewport.top,
    }));
}

export function measureAnchor(
  scroller: HTMLElement,
  anchor: ViewportAnchor,
): number | null {
  const row = Array.from(
    scroller.querySelectorAll<HTMLElement>("[data-event-id]"),
  ).find((row) => row.dataset.eventId === anchor.eventId);
  return row
    ? row.getBoundingClientRect().top -
        scroller.getBoundingClientRect().top -
        anchor.offsetPixels
    : null;
}

export function initialViewportLocation(
  timeline: { items: readonly Row[]; focusedEventId: string | null },
  bookmark: (ViewportAnchor & { wasAtBottom: boolean }) | null,
): import("react-virtuoso").IndexLocationWithAlign {
  if (bookmark && (!bookmark.wasAtBottom || timeline.focusedEventId)) {
    return {
      index: Math.max(
        0,
        timeline.items.findIndex((item) => item.id === bookmark.eventId),
      ),
      align: "start",
      offset: -bookmark.offsetPixels,
    };
  }
  if (timeline.focusedEventId) {
    return {
      index: Math.max(
        0,
        timeline.items.findIndex((item) => item.id === timeline.focusedEventId),
      ),
      align: "center",
    };
  }
  return { index: Math.max(0, timeline.items.length - 1), align: "end" };
}
