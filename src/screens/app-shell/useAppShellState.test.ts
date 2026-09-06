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

import { describe, expect, test } from "vitest";
import {
  idlePaginationState,
  paginationBackoffDelayMilliseconds,
  paginationCanLoadAtTimelineStart,
  paginationIsLoading,
  paginationStateKey,
  timelineContextKey,
  type PaginationState,
} from "./pagination";

describe("pagination state helpers", () => {
  test("pagination state is keyed by account, room, and timeline context", () => {
    const roomAKey = paginationStateKey({
      accountKey: "account-a",
      roomId: "!room-a:example.org",
      timelineContext: "live",
      instanceId: "instance",
    });
    const roomBKey = paginationStateKey({
      accountKey: "account-a",
      roomId: "!room-b:example.org",
      timelineContext: "live",
      instanceId: "instance",
    });

    expect(roomAKey).not.toBe(roomBKey);
  });

  test("loading state is explicit and retry states remain clickable", () => {
    const loadingState: PaginationState = {
      status: "loading",
      requestId: "request-1",
      startedAt: 1,
    };
    const errorState: PaginationState = {
      status: "error",
      message: "network",
    };

    expect(paginationIsLoading(loadingState)).toBe(true);
    expect(paginationIsLoading(errorState)).toBe(false);
    expect(paginationIsLoading(idlePaginationState)).toBe(false);
  });

  test("focused timelines use their own pagination context", () => {
    expect(timelineContextKey(null)).toBe("live");
    expect(timelineContextKey("$event")).toBe("focused:$event");
  });

  test("backoff is exponential and capped", () => {
    expect(paginationBackoffDelayMilliseconds(0)).toBe(500);
    expect(paginationBackoffDelayMilliseconds(1)).toBe(1000);
    expect(paginationBackoffDelayMilliseconds(Number.MAX_VALUE)).toBe(8000);
  });

  test("top-boundary pagination requires an upward attempt at the clamped start", () => {
    expect(
      paginationCanLoadAtTimelineStart(false, "timeline-ui-page:2", true, true),
    ).toBe(true);
    expect(
      paginationCanLoadAtTimelineStart(
        false,
        "timeline-ui-page:2",
        true,
        false,
      ),
    ).toBe(false);
    expect(
      paginationCanLoadAtTimelineStart(
        false,
        "timeline-ui-page:2",
        false,
        true,
      ),
    ).toBe(false);
    expect(
      paginationCanLoadAtTimelineStart(true, "timeline-ui-page:2", true, true),
    ).toBe(false);
    expect(paginationCanLoadAtTimelineStart(false, null, true, true)).toBe(
      false,
    );
  });
});
