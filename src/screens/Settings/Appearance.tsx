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

import { Palette } from 'lucide-react';
import { useState } from 'react';
import { Button, Card, Typography } from '../../components/ui';
import { useTheme } from '../../context/ThemeContext';
import {
  themePresetDetails,
  themePalettes,
  type ThemeMode,
  type ThemePresetName,
} from '../../themes/colorpalette';

const themePresetNames = Object.keys(themePresetDetails) as ThemePresetName[];
const themeModes: ThemeMode[] = ['system', 'light', 'dark'];

function getErrorMessage(error: unknown): string {
  if (typeof error === 'string' && error.trim().length > 0) {
    return error;
  }

  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message;
  }

  return 'The selected theme preset could not be saved.';
}

export default function Appearance() {
  const { theme, themePreset, setTheme, setThemePreset } = useTheme();
  const [isSavingThemeMode, setIsSavingThemeMode] = useState(false);
  const [isSavingPreset, setIsSavingPreset] = useState(false);
  const [themeModeError, setThemeModeError] = useState<string | null>(null);
  const [presetError, setPresetError] = useState<string | null>(null);

  async function handleThemeModeChange(nextTheme: ThemeMode) {
    if (theme === nextTheme || isSavingThemeMode) {
      return;
    }

    setThemeModeError(null);
    setIsSavingThemeMode(true);

    try {
      await setTheme(nextTheme);
    } catch (error) {
      setThemeModeError(getErrorMessage(error));
    } finally {
      setIsSavingThemeMode(false);
    }
  }

  async function handleThemePresetChange(nextPreset: ThemePresetName) {
    if (themePreset === nextPreset || isSavingPreset) {
      return;
    }

    setPresetError(null);
    setIsSavingPreset(true);

    try {
      await setThemePreset(nextPreset);
    } catch (error) {
      setPresetError(getErrorMessage(error));
    } finally {
      setIsSavingPreset(false);
    }
  }

  return (
    <div className="settings-view-section-body">
      <Card className="settings-view-card">
        <div className="settings-view-card-copy">
          <Typography variant="h3">Theme mode</Typography>
          <Typography muted variant="bodySmall">
            System is the default. You can override it at any time with a manual light or dark
            preference.
          </Typography>
        </div>

        <div
          className="settings-view-toggle-group"
          role="group"
          aria-label="Theme mode selection"
        >
          {themeModes.map((modeName) => {
            const isActive = theme === modeName;

            return (
              <Button
                key={modeName}
                aria-pressed={isActive}
                className={isActive ? 'settings-view-toggle--active' : undefined}
                disabled={isSavingThemeMode}
                onClick={() => void handleThemeModeChange(modeName)}
                variant={isActive ? 'primary' : 'secondary'}
              >
                {modeName[0].toUpperCase()}
                {modeName.slice(1)}
              </Button>
            );
          })}
        </div>

        {themeModeError ? (
          <Typography className="settings-view-error" variant="bodySmall">
            {themeModeError}
          </Typography>
        ) : null}
      </Card>

      <Card className="settings-view-card">
        <div className="settings-view-card-head">
          <Palette aria-hidden="true" />
          <div className="settings-view-card-copy">
            <Typography variant="h3">Palette preset</Typography>
            <Typography muted variant="bodySmall">
              The selected preset is saved natively through Tauri settings.
            </Typography>
          </div>
        </div>

        <div className="settings-view-preset-grid" role="list" aria-label="Theme presets">
          {themePresetNames.map((presetName) => {
            const preset = themePresetDetails[presetName];
            const isActive = themePreset === presetName;
            const previewColors = themePalettes[presetName].light;

            return (
              <button
                key={presetName}
                aria-pressed={isActive}
                className={`settings-view-theme-preset${
                  isActive ? ' settings-view-theme-preset--active' : ''
                }`}
                disabled={isSavingPreset}
                onClick={() => void handleThemePresetChange(presetName)}
                type="button"
              >
                <div className="settings-view-theme-preset-swatches" aria-hidden="true">
                  <span
                    className="settings-view-theme-preset-swatch"
                    style={{ backgroundColor: previewColors.primary }}
                  />
                  <span
                    className="settings-view-theme-preset-swatch"
                    style={{ backgroundColor: previewColors.secondary }}
                  />
                  <span
                    className="settings-view-theme-preset-swatch"
                    style={{ backgroundColor: previewColors.tertiary }}
                  />
                </div>

                <div className="settings-view-theme-preset-copy">
                  <Typography variant="label">{preset.label}</Typography>
                </div>
              </button>
            );
          })}
        </div>

        <Typography muted variant="meta">
          Current theme mode: {theme}
        </Typography>
        {presetError ? (
          <Typography className="settings-view-error" variant="bodySmall">
            {presetError}
          </Typography>
        ) : null}
      </Card>
    </div>
  );
}
