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

use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use tauri::{AppHandle, Manager};

pub const DEFAULT_THEME_MODE: &str = "system";

// Validate theme modes in the backend because that registry is frontend-independent.
const SUPPORTED_THEME_MODES: [&str; 3] = ["system", "light", "dark"];

// Store theme settings in one small JSON file so more settings can join the same module later.
const THEME_SETTINGS_FILE_NAME: &str = "theme-settings.json";

#[derive(Debug, Default, Deserialize, Serialize)]
struct ThemeSettingsFile {
    theme_preset: Option<String>,
    theme_mode: Option<String>,
}

pub fn get_theme_preset(
    app: &AppHandle,
    supported_presets: &[String],
    default_preset: &str,
) -> Result<String, String> {
    let settings = read_theme_settings(app)?;
    Ok(normalize_theme_preset(
        settings.theme_preset.as_deref(),
        supported_presets,
        default_preset,
    ))
}

pub fn set_theme_preset(
    app: &AppHandle,
    preset: &str,
    supported_presets: &[String],
    default_preset: &str,
) -> Result<String, String> {
    let normalized_preset =
        normalize_theme_preset(Some(preset), supported_presets, default_preset);
    let settings = ThemeSettingsFile {
        theme_preset: Some(normalized_preset.clone()),
        theme_mode: read_theme_settings(app)?.theme_mode,
    };

    write_theme_settings(app, &settings)?;

    Ok(normalized_preset)
}

pub fn get_theme_mode(app: &AppHandle) -> Result<String, String> {
    let settings = read_theme_settings(app)?;
    Ok(normalize_theme_mode(settings.theme_mode.as_deref()).to_owned())
}

pub fn set_theme_mode(app: &AppHandle, mode: &str) -> Result<String, String> {
    let mut settings = read_theme_settings(app)?;
    let normalized_mode = normalize_theme_mode(Some(mode)).to_owned();
    settings.theme_mode = Some(normalized_mode.clone());

    write_theme_settings(app, &settings)?;

    Ok(normalized_mode)
}

fn normalize_theme_preset(
    candidate: Option<&str>,
    supported_presets: &[String],
    default_preset: &str,
) -> String {
    let resolved_default_preset = resolve_default_theme_preset(supported_presets, default_preset);

    let Some(candidate) = candidate.map(str::trim).filter(|value| !value.is_empty()) else {
        return resolved_default_preset;
    };

    supported_presets
        .into_iter()
        .map(String::as_str)
        .find(|preset| *preset == candidate)
        .unwrap_or(resolved_default_preset.as_str())
        .to_owned()
}

fn resolve_default_theme_preset(supported_presets: &[String], default_preset: &str) -> String {
    let normalized_default_preset = default_preset.trim();
    if supported_presets
        .iter()
        .any(|preset| preset == normalized_default_preset)
    {
        return normalized_default_preset.to_owned();
    }

    supported_presets
        .iter()
        .map(String::as_str)
        .find(|preset| !preset.trim().is_empty())
        .unwrap_or(normalized_default_preset)
        .to_owned()
}

fn normalize_theme_mode(candidate: Option<&str>) -> &'static str {
    let Some(candidate) = candidate.map(str::trim).filter(|value| !value.is_empty()) else {
        return DEFAULT_THEME_MODE;
    };

    SUPPORTED_THEME_MODES
        .into_iter()
        .find(|mode| *mode == candidate)
        .unwrap_or(DEFAULT_THEME_MODE)
}

fn read_theme_settings(app: &AppHandle) -> Result<ThemeSettingsFile, String> {
    let settings_path = theme_settings_path(app)?;

    match fs::read(&settings_path) {
        Ok(contents) => serde_json::from_slice(&contents)
            .map_err(|error| format!("Failed to parse theme settings file: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(ThemeSettingsFile::default())
        }
        Err(error) => Err(format!("Failed to read theme settings file: {error}")),
    }
}

fn write_theme_settings(app: &AppHandle, settings: &ThemeSettingsFile) -> Result<(), String> {
    let settings_path = theme_settings_path(app)?;
    let settings_dir = settings_path
        .parent()
        .ok_or_else(|| String::from("Theme settings path has no parent directory"))?;

    fs::create_dir_all(settings_dir)
        .map_err(|error| format!("Failed to create settings directory: {error}"))?;

    let contents = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("Failed to serialize theme settings: {error}"))?;

    fs::write(settings_path, contents)
        .map_err(|error| format!("Failed to write theme settings file: {error}"))
}

fn theme_settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Failed to resolve app data directory: {error}"))?
        .join("settings")
        .join(THEME_SETTINGS_FILE_NAME))
}
