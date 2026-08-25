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

import { invoke } from "@tauri-apps/api/core";
import type { ThemePreferences } from "../../components/context/themePreferences";
import type { ThemeMode } from "../../components/themes/colorpalette";

export function createTauriThemePreferences(): ThemePreferences {
  return {
    getThemeMode: () => invoke<string>("get_theme_mode"),
    setThemeMode: (mode: ThemeMode) =>
      invoke<string>("set_theme_mode", { mode }),
    getThemePreset: (supportedPresets, defaultPreset) =>
      invoke<string>("get_theme_preset", {
        supportedPresets,
        defaultPreset,
      }),
    setThemePreset: (preset, supportedPresets, defaultPreset) =>
      invoke<string>("set_theme_preset", {
        preset,
        supportedPresets,
        defaultPreset,
      }),
  };
}
