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

import { expect, test, vi } from "vitest";
import { attachTimelineTouchPagination } from "./touchPagination";

function fixture() {
  const target = Object.assign(new EventTarget(), { scrollTop: 0 });
  const trigger = vi.fn();
  const detach = attachTimelineTouchPagination(target as HTMLElement, trigger);
  function touch(type: string, y = 0, x = 0, count = 1) {
    const event = new Event(type, { cancelable: true });
    Object.defineProperty(event, "touches", {
      value: Array.from({ length: count }, (_, identifier) => ({
        identifier,
        clientX: x,
        clientY: y,
      })),
    });
    target.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
  }
  return { target, trigger, detach, touch };
}

test("a pull at the oldest edge triggers once per drag, then permits a fresh drag", () => {
  const { touch, trigger } = fixture();
  touch("touchstart", 100);
  touch("touchmove", 120);
  touch("touchmove", 160);
  expect(trigger).toHaveBeenCalledTimes(1);
  touch("touchend", 160, 0, 0);
  touch("touchstart", 100);
  touch("touchmove", 120);
  expect(trigger).toHaveBeenCalledTimes(2);
});

test("reaching the top during a drag permits a further pull, including elastic overscroll", () => {
  const { touch, target, trigger } = fixture();
  target.scrollTop = 50;
  touch("touchstart", 100);
  touch("touchmove", 150);
  expect(trigger).not.toHaveBeenCalled();
  target.scrollTop = -2;
  touch("touchmove", 175);
  expect(trigger).toHaveBeenCalledOnce();
});

test("taps, jitter, horizontal drags, and movement toward newer messages do not paginate", () => {
  const { touch, trigger } = fixture();
  touch("touchstart", 100);
  touch("touchmove", 102);
  touch("touchmove", 80);
  touch("touchmove", 110, 100);
  expect(trigger).not.toHaveBeenCalled();
});

test("cancelled and multi-touch gestures cannot trigger until a fresh single touch", () => {
  const { touch, trigger } = fixture();
  touch("touchstart", 100);
  touch("touchcancel", 100, 0, 0);
  touch("touchmove", 150);
  touch("touchstart", 100);
  touch("touchmove", 120, 0, 2);
  touch("touchmove", 160);
  expect(trigger).not.toHaveBeenCalled();
  touch("touchstart", 100);
  touch("touchmove", 120);
  expect(trigger).toHaveBeenCalledOnce();
});

test("programmatic scroll and detached listeners cannot trigger pagination", () => {
  const { target, touch, trigger, detach } = fixture();
  target.dispatchEvent(new Event("scroll"));
  detach();
  touch("touchstart", 100);
  touch("touchmove", 150);
  expect(trigger).not.toHaveBeenCalled();
});
