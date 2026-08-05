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

import { beforeEach, describe, expect, it, vi } from "vitest";

const { emit, enabledLevels } = vi.hoisted(() => ({
  emit: vi.fn(),
  enabledLevels: vi.fn(),
}));

vi.mock("@fltsci/tauri-plugin-tracing", async (importOriginal) => {
  const original =
    await importOriginal<typeof import("@fltsci/tauri-plugin-tracing")>();
  return { ...original, emit, enabledLevels };
});

import { debug, initializeTracing, trace, tracingTesting } from "./tracing";

describe("frontend tracing helper", () => {
  beforeEach(() => {
    emit.mockReset();
    enabledLevels.mockReset();
    tracingTesting.reset();
  });

  it("initializes enabled levels only once", async () => {
    enabledLevels.mockResolvedValue({
      trace: false,
      debug: true,
      info: true,
      warn: true,
      error: true,
    });

    await Promise.all([initializeTracing(), initializeTracing()]);

    expect(enabledLevels).toHaveBeenCalledOnce();
  });

  it("disables every level when initialization fails", async () => {
    enabledLevels.mockRejectedValue(new Error("not installed"));
    await initializeTracing();

    debug("test.event", { component: "test" });

    expect(emit).not.toHaveBeenCalled();
  });

  it("does not evaluate lazy context for a disabled level", async () => {
    enabledLevels.mockResolvedValue({
      trace: false,
      debug: false,
      info: true,
      warn: true,
      error: true,
    });
    await initializeTracing();
    const contextFactory = vi.fn(() => ({ component: "timeline" }));

    trace("timeline.snapshot", contextFactory);

    expect(contextFactory).not.toHaveBeenCalled();
    expect(emit).not.toHaveBeenCalled();
  });

  it("evaluates an enabled lazy context once and consumes transport errors", async () => {
    enabledLevels.mockResolvedValue({
      trace: true,
      debug: true,
      info: true,
      warn: true,
      error: true,
    });
    emit.mockRejectedValue(new Error("transport failed"));
    await initializeTracing();
    const contextFactory = vi.fn(() => ({
      component: "timeline",
      diagnosticDetails: { items: [1, 2] },
    }));

    trace("timeline.snapshot", contextFactory);
    await Promise.resolve();

    expect(contextFactory).toHaveBeenCalledOnce();
    expect(emit).toHaveBeenCalledOnce();
  });

  it("suppresses an unchanged diagnostic event and emits its changed state", async () => {
    enabledLevels.mockResolvedValue({
      trace: true,
      debug: true,
      info: true,
      warn: true,
      error: true,
    });
    emit.mockResolvedValue(undefined);
    await initializeTracing();

    trace("timeline.snapshot", {
      component: "timeline",
      roomId: "!room:example.org",
      diagnosticDetails: { itemCount: 4 },
    });
    trace("timeline.snapshot", {
      component: "timeline",
      roomId: "!room:example.org",
      diagnosticDetails: { itemCount: 4 },
    });
    trace("timeline.snapshot", {
      component: "timeline",
      roomId: "!room:example.org",
      diagnosticDetails: { itemCount: 5 },
    });

    expect(emit).toHaveBeenCalledTimes(2);
  });
});
