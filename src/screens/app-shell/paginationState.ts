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

export type PaginationState =
  | { status: "idle" }
  | { status: "loading"; requestId: string; startedAt: number }
  | { status: "error"; message: string };

export type PaginationContext = {
  accountKey: string;
  roomId: string;
  timelineContext: string;
};

export const idlePaginationState: PaginationState = { status: "idle" };

export function paginationStateKey(context: PaginationContext): string {
  return `${context.accountKey}::${context.roomId}::${context.timelineContext}`;
}

export function timelineContextKey(focusedEventId: string | null): string {
  return focusedEventId ? `focused:${focusedEventId}` : "live";
}

export function paginationIsLoading(state: PaginationState): boolean {
  return state.status === "loading";
}
