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

import {
  paginationBackoffDelayMilliseconds,
  paginationMaximumBackoffCount,
} from "./pagination";

// A single top-boundary request may traverse a few empty SDK pages, but never
// enough to turn one user gesture into an unbounded homeserver request burst.
// Count Hyperion calls to SDK pagination, including the gesture's first call.
const maximumCallsPerGesture = 3;
// 500ms * 2^4 is the maximum delay; saturating also avoids numeric overflow.

export type PaginationViewport = {
  items: string[];
  signal: AbortSignal;
  settle: (revision: number, signal: AbortSignal) => Promise<string[] | null>;
};
type PaginationBatchPorts = {
  request: () => Promise<{ reachedStart: boolean; revision: number }>;
  state: (loading: boolean) => void;
  error: (error: unknown) => void;
};

export class PaginationBatch {
  private active = false;
  private previousItems: string[] | null = null;
  private readonly lifecycle = new AbortController();
  backoffCount = 0;

  constructor(private readonly ports: PaginationBatchPorts) {}

  dispose() {
    this.lifecycle.abort();
  }

  async start(viewport: PaginationViewport): Promise<void> {
    if (this.active || this.lifecycle.signal.aborted || viewport.signal.aborted)
      return;
    if (
      this.previousItems &&
      hasOlderRenderedRows(this.previousItems, viewport.items)
    ) {
      this.backoffCount = 0;
    }
    this.previousItems = viewport.items;
    this.active = true;
    this.ports.state(true);
    const signal = AbortSignal.any([this.lifecycle.signal, viewport.signal]);
    try {
      for (let calls = 1; calls <= maximumCallsPerGesture; calls += 1) {
        const result = await this.ports.request();
        if (signal.aborted) return;
        if (result.reachedStart) return;
        const rendered = await viewport.settle(result.revision, signal);
        if (!rendered || signal.aborted) return;
        if (hasOlderRenderedRows(viewport.items, rendered)) {
          this.backoffCount = 0;
          return;
        }
        const delay = paginationBackoffDelayMilliseconds(this.backoffCount);
        this.backoffCount = Math.min(
          paginationMaximumBackoffCount,
          this.backoffCount + 1,
        );
        if (calls === maximumCallsPerGesture) return;
        if (!(await waitForContinuation(delay, signal))) return;
        // Arrivals during the delay may supply older content. Recheck rendering
        // before spending another SDK call, including asynchronous layout work.
        const latest = await viewport.settle(result.revision, signal);
        if (!latest || signal.aborted) return;
        if (hasOlderRenderedRows(viewport.items, latest)) {
          this.backoffCount = 0;
          return;
        }
      }
    } catch (error) {
      if (!this.lifecycle.signal.aborted) this.ports.error(error);
    } finally {
      this.active = false;
      if (!this.lifecycle.signal.aborted) this.ports.state(false);
    }
  }
}

export function hasOlderRenderedRows(
  before: string[],
  after: string[],
): boolean {
  if (!before.length) return after.length > 0;
  const previous = new Set(before);
  const firstSurvivor = after.findIndex((id) => previous.has(id));
  // If every prior row was removed, there is no reliable older boundary. Stop
  // conservatively on replacement content instead of spending extra calls.
  if (firstSurvivor < 0) return after.length > 0;
  return after.slice(0, firstSurvivor).some((id) => !previous.has(id));
}

function waitForContinuation(
  delay: number,
  signal: AbortSignal,
): Promise<boolean> {
  return new Promise((resolve) => {
    if (signal.aborted) {
      resolve(false);
      return;
    }
    const timer = setTimeout(() => finish(true), delay);
    function finish(elapsed: boolean) {
      clearTimeout(timer);
      signal.removeEventListener("abort", cancel);
      resolve(elapsed);
    }
    function cancel() {
      finish(false);
    }
    signal.addEventListener("abort", cancel, { once: true });
  });
}
