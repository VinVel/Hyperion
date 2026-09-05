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

import { afterEach, describe, expect, test, vi } from "vitest";
import { removeLegacyTimelineSnapshots, writeCachedJson } from "./cache";

describe("cached JSON writes", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  test("does not throw when WebView local storage is full", () => {
    vi.stubGlobal("window", {
      localStorage: {
        setItem: () => {
          throw new Error("quota exceeded");
        },
      },
    });

    expect(() => writeCachedJson("timeline", { items: [] })).not.toThrow();
    expect(writeCachedJson("timeline", { items: [] })).toBe(false);
  });
});

test("legacy timeline cleanup preserves drafts, preferences, and collection caches", () => {
  const values = new Map([
    ["hyperion.appShell.roomSnapshots.account.!room", "messages"],
    ["hyperion.appShell.roomSnapshots.other.!room", "messages"],
    ["hyperion.appShell.roomThreads.account", "threads"],
    ["hyperion.appShell.spaces.account", "spaces"],
    ["hyperion.appShell.drafts.account", "draft"],
    ["theme", "dark"],
    ["hyperion.appShell.roomSnapshotsUnrelated", "keep"],
  ]);
  const storage = {
    get length() {
      return values.size;
    },
    key: (index: number) => [...values.keys()][index] ?? null,
    removeItem: (key: string) => values.delete(key),
  };
  removeLegacyTimelineSnapshots(storage);
  expect([...values.values()]).toEqual([
    "threads",
    "spaces",
    "draft",
    "dark",
    "keep",
  ]);
});

test("legacy cleanup tolerates a WebView that blocks storage access", () => {
  vi.stubGlobal("window", {
    get localStorage() {
      throw new Error("storage is unavailable");
    },
  });
  try {
    expect(() => removeLegacyTimelineSnapshots()).not.toThrow();
  } finally {
    vi.unstubAllGlobals();
  }
});
