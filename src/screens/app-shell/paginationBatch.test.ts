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
import { PaginationBatch, hasOlderRenderedRows } from "./paginationBatch";

function fixture() {
  const request = vi.fn(async () => ({ reachedStart: false, revision: 2 }));
  const settle = vi.fn(async () => ["oldest", "latest"]);
  const state = vi.fn();
  const error = vi.fn();
  const controller = new AbortController();
  const batch = new PaginationBatch({ request, state, error });
  const viewport = {
    signal: controller.signal,
    settle,
    items: ["oldest", "latest"],
  };
  return { batch, request, settle, state, error, controller, viewport };
}
afterEach(() => vi.useRealTimers());

test("one gesture allows exactly three calls, including unchanged/nonvisible results", async () => {
  vi.useFakeTimers();
  const f = fixture();
  const run = f.batch.start(f.viewport);
  await vi.advanceTimersByTimeAsync(499);
  expect(f.request).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(1);
  expect(f.request).toHaveBeenCalledTimes(2);
  await vi.advanceTimersByTimeAsync(1000);
  await run;
  expect(f.request).toHaveBeenCalledTimes(3);
  expect(f.state).toHaveBeenLastCalledWith(false);
});

test("slow work remains exclusive beyond three seconds and waits for rendered projection", async () => {
  vi.useFakeTimers();
  const f = fixture();
  let complete!: (value: { reachedStart: boolean; revision: number }) => void;
  f.request.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        complete = resolve;
      }),
  );
  const run = f.batch.start(f.viewport);
  await vi.advanceTimersByTimeAsync(4000);
  await f.batch.start(f.viewport);
  expect(f.request).toHaveBeenCalledTimes(1);
  expect(f.state).toHaveBeenLastCalledWith(true);
  let rendered!: (items: string[]) => void;
  f.settle.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        rendered = resolve;
      }),
  );
  complete({ reachedStart: false, revision: 2 });
  await vi.advanceTimersByTimeAsync(10000);
  expect(f.request).toHaveBeenCalledTimes(1);
  rendered(["older", "oldest", "latest"]);
  await run;
  expect(f.state).toHaveBeenLastCalledWith(false);
});

test("fresh gestures replenish call budget but preserve saturated no-progress backoff", async () => {
  vi.useFakeTimers();
  const f = fixture();
  const first = f.batch.start(f.viewport);
  await vi.runAllTimersAsync();
  await first;
  const second = f.batch.start(f.viewport);
  await vi.advanceTimersByTimeAsync(3999);
  expect(f.request).toHaveBeenCalledTimes(4);
  await vi.advanceTimersByTimeAsync(1);
  expect(f.request).toHaveBeenCalledTimes(5);
  await vi.runAllTimersAsync();
  await second;
  expect(f.batch.backoffCount).toBe(4);
  f.settle.mockResolvedValueOnce(["older", "oldest", "latest"]);
  await f.batch.start(f.viewport);
  expect(f.batch.backoffCount).toBe(0);
});

test("terminal and failed requests stop without automatic retry", async () => {
  const f = fixture();
  f.request.mockResolvedValueOnce({ reachedStart: true, revision: 2 });
  await f.batch.start(f.viewport);
  expect(f.request).toHaveBeenCalledTimes(1);
  f.request.mockRejectedValueOnce("network");
  await f.batch.start(f.viewport);
  expect(f.error).toHaveBeenCalledWith("network");
  expect(f.request).toHaveBeenCalledTimes(2);
});

test("leaving the edge during a request or continuation wait stops the batch", async () => {
  vi.useFakeTimers();
  const f = fixture();
  const run = f.batch.start(f.viewport);
  await vi.advanceTimersByTimeAsync(100);
  f.controller.abort();
  await vi.runAllTimersAsync();
  await run;
  expect(f.request).toHaveBeenCalledTimes(1);
});

test("lifecycle invalidation detaches late failures and cancels timers", async () => {
  vi.useFakeTimers();
  const f = fixture();
  let reject!: (error: unknown) => void;
  f.request.mockImplementationOnce(
    () =>
      new Promise((_resolve, fail) => {
        reject = fail;
      }),
  );
  const run = f.batch.start(f.viewport);
  f.batch.dispose();
  reject("late failure from room A");
  await run;
  expect(f.error).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(0);
});

test("appends, edits, duplicate-only projections and removals are not older progress", () => {
  expect(hasOlderRenderedRows(["a", "b"], ["a", "b", "arrival"])).toBe(false);
  expect(hasOlderRenderedRows(["a", "b"], ["b"])).toBe(false);
  expect(hasOlderRenderedRows(["a", "b"], ["a", "b"])).toBe(false);
  expect(hasOlderRenderedRows(["a", "b"], ["older", "b", "arrival"])).toBe(
    true,
  );
});

test("late arrivals during cooldown cancel continuation and reset backoff", async () => {
  vi.useFakeTimers();
  const f = fixture();
  f.settle.mockResolvedValueOnce(["oldest", "latest"]);
  f.settle.mockResolvedValueOnce(["older", "oldest", "latest"]);
  const run = f.batch.start(f.viewport);
  await vi.runAllTimersAsync();
  await run;
  expect(f.request).toHaveBeenCalledTimes(1);
  expect(f.batch.backoffCount).toBe(0);
});

test("leaving the edge never releases an outstanding operation early", async () => {
  const f = fixture();
  let complete!: (value: { reachedStart: boolean; revision: number }) => void;
  f.request.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        complete = resolve;
      }),
  );
  const run = f.batch.start(f.viewport);
  f.controller.abort();
  await f.batch.start({ ...f.viewport, signal: new AbortController().signal });
  expect(f.request).toHaveBeenCalledTimes(1);
  expect(f.state).toHaveBeenLastCalledWith(true);
  complete({ reachedStart: false, revision: 2 });
  await run;
  expect(f.settle).not.toHaveBeenCalled();
});

test("older rows arriving between gestures reset the accumulated backoff", async () => {
  vi.useFakeTimers();
  const f = fixture();
  const first = f.batch.start(f.viewport);
  await vi.runAllTimersAsync();
  await first;
  expect(f.batch.backoffCount).toBe(3);
  const items = ["older", "oldest", "latest"];
  f.settle.mockResolvedValue(items);
  const next = f.batch.start({ ...f.viewport, items });
  await vi.advanceTimersByTimeAsync(500);
  expect(f.request).toHaveBeenCalledTimes(5);
  await vi.runAllTimersAsync();
  await next;
});
