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
 */

import { getCurrentWindow } from "@tauri-apps/api/window";
import { platform } from "@tauri-apps/plugin-os";
import type { WindowController } from "../../components/ui/windowController";

const desktopPlatforms = new Set(["linux", "macos", "windows"]);

export function createTauriWindowController(): WindowController {
  return {
    isDesktopPlatform() {
      try {
        return desktopPlatforms.has(platform());
      } catch {
        return false;
      }
    },
    startDragging: () => getCurrentWindow().startDragging(),
    toggleMaximize: () => getCurrentWindow().toggleMaximize(),
    minimize: () => getCurrentWindow().minimize(),
    close: () => getCurrentWindow().close(),
  };
}
