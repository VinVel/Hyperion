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

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    account::types::EncryptionPreferences,
    settings::{encryption::EncryptionOverview, sessions::SessionOverview},
};

// Account settings live beside the Matrix store so each account keeps its own
// offline preferences and last-known server-derived state.
const ACCOUNT_SETTINGS_DIR_NAME: &str = "settings";
const ENCRYPTION_SETTINGS_FILE_NAME: &str = "encryption-settings.json";
const SESSION_SETTINGS_FILE_NAME: &str = "session-settings.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountEncryptionSettings {
    #[serde(default)]
    pub preferences: EncryptionPreferences,
    #[serde(default)]
    pub overview: Option<EncryptionOverview>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountSessionSettings {
    #[serde(default)]
    pub overview: Option<SessionOverview>,
}

pub fn read_encryption_settings(store_dir: &Path) -> Result<AccountEncryptionSettings, String> {
    let settings_path = encryption_settings_path(store_dir)?;
    match fs::read(&settings_path) {
        Ok(contents) => serde_json::from_slice(&contents)
            .map_err(|error| format!("Failed to parse encryption settings file: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(AccountEncryptionSettings::default())
        }
        Err(error) => Err(format!("Failed to read encryption settings file: {error}")),
    }
}

pub fn write_encryption_settings(
    store_dir: &Path,
    settings: &AccountEncryptionSettings,
) -> Result<(), String> {
    let settings_path = encryption_settings_path(store_dir)?;
    let settings_dir = settings_path
        .parent()
        .ok_or_else(|| String::from("Encryption settings path has no parent directory"))?;
    fs::create_dir_all(settings_dir)
        .map_err(|error| format!("Failed to create encryption settings directory: {error}"))?;

    let contents = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("Failed to serialize encryption settings: {error}"))?;
    fs::write(settings_path, contents)
        .map_err(|error| format!("Failed to write encryption settings file: {error}"))
}

pub fn read_session_settings(store_dir: &Path) -> Result<AccountSessionSettings, String> {
    let settings_path = session_settings_path(store_dir)?;
    match fs::read(&settings_path) {
        Ok(contents) => serde_json::from_slice(&contents)
            .map_err(|error| format!("Failed to parse session settings file: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(AccountSessionSettings::default())
        }
        Err(error) => Err(format!("Failed to read session settings file: {error}")),
    }
}

pub fn write_session_settings(
    store_dir: &Path,
    settings: &AccountSessionSettings,
) -> Result<(), String> {
    let settings_path = session_settings_path(store_dir)?;
    let settings_dir = settings_path
        .parent()
        .ok_or_else(|| String::from("Session settings path has no parent directory"))?;
    fs::create_dir_all(settings_dir)
        .map_err(|error| format!("Failed to create session settings directory: {error}"))?;

    let contents = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("Failed to serialize session settings: {error}"))?;
    fs::write(settings_path, contents)
        .map_err(|error| format!("Failed to write session settings file: {error}"))
}

pub fn read_encryption_preferences(store_dir: &Path) -> Result<EncryptionPreferences, String> {
    read_encryption_settings(store_dir).map(|settings| settings.preferences)
}

pub fn write_encryption_preferences(
    store_dir: &Path,
    preferences: &EncryptionPreferences,
) -> Result<(), String> {
    let mut settings = read_encryption_settings(store_dir)?;
    settings.preferences = preferences.clone();
    if let Some(overview) = settings.overview.as_mut() {
        overview.server_key_storage_opted_out = preferences.server_key_storage_opted_out;
        overview.verified_devices_only = preferences.verified_devices_only;
    }
    write_encryption_settings(store_dir, &settings)
}

pub fn read_encryption_overview(store_dir: &Path) -> Result<Option<EncryptionOverview>, String> {
    read_encryption_settings(store_dir).map(|settings| settings.overview)
}

pub fn write_encryption_overview(
    store_dir: &Path,
    overview: &EncryptionOverview,
) -> Result<(), String> {
    let mut settings = read_encryption_settings(store_dir)?;
    settings.preferences.server_key_storage_opted_out = overview.server_key_storage_opted_out;
    settings.preferences.verified_devices_only = overview.verified_devices_only;
    settings.overview = Some(overview.clone());
    write_encryption_settings(store_dir, &settings)
}

pub fn read_session_overview(store_dir: &Path) -> Result<Option<SessionOverview>, String> {
    read_session_settings(store_dir).map(|settings| settings.overview)
}

pub fn write_session_overview(store_dir: &Path, overview: &SessionOverview) -> Result<(), String> {
    let mut settings = read_session_settings(store_dir)?;
    settings.overview = Some(overview.clone());
    write_session_settings(store_dir, &settings)
}

fn encryption_settings_path(store_dir: &Path) -> Result<PathBuf, String> {
    Ok(account_settings_dir(store_dir)?.join(ENCRYPTION_SETTINGS_FILE_NAME))
}

fn session_settings_path(store_dir: &Path) -> Result<PathBuf, String> {
    Ok(account_settings_dir(store_dir)?.join(SESSION_SETTINGS_FILE_NAME))
}

fn account_settings_dir(store_dir: &Path) -> Result<PathBuf, String> {
    let account_root = store_dir
        .parent()
        .ok_or_else(|| String::from("Account store directory has no parent"))?;

    Ok(account_root.join(ACCOUNT_SETTINGS_DIR_NAME))
}
