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

import { afterEach, expect, test, vi } from "vitest";
import type { RoomTimeline } from "../appShellAdapters";
import { waitForRenderedTimeline } from "./paginationRendering";

function fixture() {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (callback: () => void) =>
    setTimeout(callback, 16),
  );
  vi.stubGlobal("cancelAnimationFrame", clearTimeout);
  let current = {
    timelineIdentity: { instanceId: "instance" },
    revision: 1,
    items: [{ id: "oldest" }],
  } as RoomTimeline;
  const controller = new AbortController();
  return {
    read: () => current,
    render: (revision: number) => {
      current = { ...current, revision };
    },
    controller,
  };
}
afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

test("completion waits for an acknowledged revision to be rendered, not elapsed request time", async () => {
  const f = fixture();
  let finished = false;
  const wait = waitForRenderedTimeline(
    f.read,
    "instance",
    2,
    f.controller.signal,
  ).then((items) => {
    finished = true;
    return items;
  });
  await vi.advanceTimersByTimeAsync(4000);
  expect(finished).toBe(false);
  f.render(2);
  await vi.advanceTimersByTimeAsync(32);
  expect(await wait).toEqual(["oldest"]);
  expect(vi.getTimerCount()).toBe(0);
});

test("a snapshot rendered before completion settles without waiting for another update", async () => {
  const f = fixture();
  f.render(3);
  const wait = waitForRenderedTimeline(
    f.read,
    "instance",
    2,
    f.controller.signal,
  );
  await vi.advanceTimersByTimeAsync(32);
  expect(await wait).toEqual(["oldest"]);
});

test("a newer render restarts settling and invalidation cancels pending frames", async () => {
  const f = fixture();
  f.render(2);
  let finished = false;
  const wait = waitForRenderedTimeline(
    f.read,
    "instance",
    2,
    f.controller.signal,
  ).then((items) => {
    finished = true;
    return items;
  });
  await vi.advanceTimersByTimeAsync(16);
  f.render(3);
  await vi.advanceTimersByTimeAsync(16);
  expect(finished).toBe(false);
  f.controller.abort();
  expect(await wait).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
});

test("a recreated timeline cannot satisfy the prior instance's completion", async () => {
  const f = fixture();
  const wait = waitForRenderedTimeline(
    f.read,
    "obsolete",
    1,
    f.controller.signal,
  );
  await vi.advanceTimersByTimeAsync(16);
  expect(await wait).toBeNull();
});
