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

import type { RoomTimeline } from "../appShellAdapters";

// Two animation frames let React and Virtuoso commit the acknowledged SDK
// projection. A newer render restarts the check; operation completion alone
// never proves that its rows have reached the list.
export function waitForRenderedTimeline(
  read: () => RoomTimeline | null,
  instanceId: string | undefined,
  revision: number,
  signal: AbortSignal,
): Promise<string[] | null> {
  return new Promise((resolve) => {
    let frame = 0;
    let previous: RoomTimeline | null = null;
    function finish(value: string[] | null) {
      cancelAnimationFrame(frame);
      signal.removeEventListener("abort", cancel);
      resolve(value);
    }
    function cancel() {
      finish(null);
    }
    function measure() {
      const rendered = read();
      if (!rendered || rendered.timelineIdentity.instanceId !== instanceId) {
        finish(null);
        return;
      }
      if (rendered.revision >= revision && previous === rendered) {
        finish(rendered.items.map((item) => item.id));
        return;
      }
      previous = rendered.revision >= revision ? rendered : null;
      frame = requestAnimationFrame(measure);
    }
    if (signal.aborted) {
      finish(null);
      return;
    }
    signal.addEventListener("abort", cancel, { once: true });
    frame = requestAnimationFrame(measure);
  });
}
