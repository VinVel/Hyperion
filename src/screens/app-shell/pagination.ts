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

import type { RoomTimeline } from "./appShellAdapters";

export type PaginationState =
  | { status: "idle" }
  | { status: "loading"; requestId: string; startedAt: number }
  | { status: "cooldown"; retryAt: number; retryCount: number }
  | { status: "error"; message: string };

export type PaginationContext = {
  accountKey: string;
  roomId: string;
  timelineContext: string;
  instanceId: string;
};

export const idlePaginationState: PaginationState = { status: "idle" };

// Retry delays keep a cursor-advancing empty page from creating a tight request loop.
const paginationRetryBaseDelayMilliseconds = 500;
const paginationRetryMaximumDelayMilliseconds = 8_000;
// Saturation keeps 500ms exponential waits at the eight-second maximum.
export const paginationMaximumBackoffCount = 4;

export function paginationBackoffDelayMilliseconds(retryCount: number): number {
  const exponent = Math.min(
    paginationMaximumBackoffCount,
    Math.max(0, retryCount),
  );
  return Math.min(
    paginationRetryBaseDelayMilliseconds * 2 ** exponent,
    paginationRetryMaximumDelayMilliseconds,
  );
}

export function paginationCanLoadAtTimelineStart(
  isLoading: boolean,
  nextBefore: string | null,
  isAtTop: boolean,
  attemptedToScrollUpward: boolean,
): boolean {
  return (
    !isLoading && nextBefore !== null && isAtTop && attemptedToScrollUpward
  );
}

export function paginationStateKey(context: PaginationContext): string {
  return JSON.stringify([
    context.accountKey,
    context.roomId,
    context.timelineContext,
    context.instanceId,
  ]);
}

export function timelineContextKey(focusedEventId: string | null): string {
  return focusedEventId ? `focused:${focusedEventId}` : "live";
}

export function paginationIsLoading(state: PaginationState): boolean {
  return state.status === "loading" || state.status === "cooldown";
}
export function paginationContextForTimeline(
  accountKey: string,
  timeline: RoomTimeline | null,
): PaginationContext | null {
  if (!timeline) {
    return null;
  }

  return {
    accountKey,
    instanceId: timeline.timelineIdentity.instanceId,
    roomId: timeline.roomId,
    timelineContext: timelineContextKey(timeline.focusedEventId),
  };
}
export function createPaginationRequestId(): string {
  const randomPart =
    globalThis.crypto?.randomUUID?.() ?? Math.random().toString(36).slice(2);
  return `pagination-${Date.now()}-${randomPart}`;
}
