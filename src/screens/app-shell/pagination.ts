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
};

export const idlePaginationState: PaginationState = { status: "idle" };

// Retry delays keep a cursor-advancing empty page from creating a tight request loop.
const paginationRetryBaseDelayMilliseconds = 500;
const paginationRetryMaximumDelayMilliseconds = 8_000;
// A single top-boundary request may traverse a few empty SDK pages, but never
// enough to turn one user gesture into an unbounded homeserver request burst.
const paginationMaximumCursorAdvancesPerRequest = 3;

type PaginationContinuation = {
  hadNewItems: boolean;
  nextBefore: string | null;
  reachedStart: boolean;
  tokenChanged: boolean;
};

export function paginationBackoffDelayMilliseconds(retryCount: number): number {
  const exponent = Math.max(0, retryCount);
  return Math.min(
    paginationRetryBaseDelayMilliseconds * 2 ** exponent,
    paginationRetryMaximumDelayMilliseconds,
  );
}

export function paginationCanAutomaticallyContinue(
  response: PaginationContinuation,
  cursorAdvanceCount: number,
): boolean {
  return (
    !response.hadNewItems &&
    response.tokenChanged &&
    !response.reachedStart &&
    response.nextBefore !== null &&
    paginationCanAdvanceCursor(cursorAdvanceCount)
  );
}

export function paginationCanAdvanceCursor(
  cursorAdvanceCount: number,
): boolean {
  return cursorAdvanceCount < paginationMaximumCursorAdvancesPerRequest;
}

export function paginationErrorIsRateLimited(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  const normalizedMessage = message.toLocaleLowerCase();
  return (
    normalizedMessage.includes("m_limit_exceeded") ||
    normalizedMessage.includes("too many requests") ||
    normalizedMessage.includes("rate limit") ||
    normalizedMessage.includes("rate_limit") ||
    normalizedMessage.includes("429")
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
  return `${context.accountKey}::${context.roomId}::${context.timelineContext}`;
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
    roomId: timeline.roomId,
    timelineContext: timelineContextKey(timeline.focusedEventId),
  };
}
export function createPaginationRequestId(): string {
  const randomPart =
    globalThis.crypto?.randomUUID?.() ?? Math.random().toString(36).slice(2);
  return `pagination-${Date.now()}-${randomPart}`;
}
