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

type NavigationKey = Pick<
  KeyboardEvent,
  "key" | "repeat" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey"
>;

export function keyboardPaginationIntent(
  event: NavigationKey,
  atOldestEdge: boolean,
  interactiveTarget: boolean,
): boolean {
  if (
    !atOldestEdge ||
    interactiveTarget ||
    event.repeat ||
    event.altKey ||
    event.metaKey
  )
    return false;
  return (
    event.key === "ArrowUp" ||
    event.key === "PageUp" ||
    event.key === "Home" ||
    (event.key === " " && event.shiftKey)
  );
}
