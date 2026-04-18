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

use tauri::{AppHandle, Runtime};
#[cfg(target_os = "android")]
use tauri_plugin_android_secure_storage as android_secure_storage;

#[cfg(not(target_os = "android"))]
const SECRET_SERVICE_NAME: &str = "net.velcore.hyperion.matrix-store";

pub fn get_secret<R: Runtime>(app: &AppHandle<R>, key: &str) -> Result<Option<Vec<u8>>, String> {
    #[cfg(target_os = "android")]
    {
        return android_secure_storage::get_secret(app, key).map_err(|error| error.to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        let entry = keyring::Entry::new(SECRET_SERVICE_NAME, key)
            .map_err(|error| format!("Failed to open secure storage entry: {error}"))?;

        match entry.get_secret() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(format!("Failed to read secure storage entry: {error}")),
        }
    }
}

pub fn set_secret<R: Runtime>(app: &AppHandle<R>, key: &str, value: &[u8]) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        return android_secure_storage::set_secret(app, key, value)
            .map_err(|error| error.to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        keyring::Entry::new(SECRET_SERVICE_NAME, key)
            .map_err(|error| format!("Failed to open secure storage entry: {error}"))?
            .set_secret(value)
            .map_err(|error| format!("Failed to write secure storage entry: {error}"))
    }
}
