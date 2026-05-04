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

#[cfg(target_os = "android")]
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Runtime};

// Matrix store encryption keys are grouped under this stable service name for
// compatibility with existing desktop credentials.
const SECRET_SERVICE_NAME: &str = "net.velcore.hyperion.matrix-store";

// Android uses a named keyring-core store so Hyperion credentials stay isolated
// from any other Rust keyring users in the same application process.
#[cfg(target_os = "android")]
const ANDROID_STORE_NAME: &str = "hyperion-matrix-store";

pub(crate) fn initialize_default_store() -> keyring_core::Result<()> {
    keyring_core::set_default_store(platform_default_store()?);
    Ok(())
}

pub(crate) fn unset_default_store() {
    drop(keyring_core::unset_default_store());
}

#[cfg(target_os = "windows")]
fn platform_default_store() -> keyring_core::Result<Arc<keyring_core::CredentialStore>> {
    let store = windows_native_keyring_store::Store::new()?;
    Ok(store)
}

#[cfg(target_os = "macos")]
fn platform_default_store() -> keyring_core::Result<Arc<keyring_core::CredentialStore>> {
    let store = apple_native_keyring_store::keychain::Store::new()?;
    Ok(store)
}

#[cfg(target_os = "ios")]
fn platform_default_store() -> keyring_core::Result<Arc<keyring_core::CredentialStore>> {
    let store = apple_native_keyring_store::protected::Store::new()?;
    Ok(store)
}

#[cfg(target_os = "linux")]
fn platform_default_store() -> keyring_core::Result<Arc<keyring_core::CredentialStore>> {
    let store = linux_keyutils_keyring_store::Store::new()?;
    Ok(store)
}

#[cfg(target_os = "android")]
fn platform_default_store() -> keyring_core::Result<Arc<keyring_core::CredentialStore>> {
    let mut configuration = HashMap::new();
    configuration.insert("name", ANDROID_STORE_NAME);

    let store = android_native_keyring_store::Store::new_with_configuration(&configuration)?;
    Ok(store)
}

pub fn get_secret<R: Runtime>(_app: &AppHandle<R>, key: &str) -> Result<Option<Vec<u8>>, String> {
    let entry = keyring_core::Entry::new(SECRET_SERVICE_NAME, key)
        .map_err(|error| format!("Failed to open secure storage entry: {error}"))?;

    match entry.get_secret() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("Failed to read secure storage entry: {error}")),
    }
}

pub fn set_secret<R: Runtime>(_app: &AppHandle<R>, key: &str, value: &[u8]) -> Result<(), String> {
    let entry = keyring_core::Entry::new(SECRET_SERVICE_NAME, key)
        .map_err(|error| format!("Failed to open secure storage entry: {error}"))?;

    entry
        .set_secret(value)
        .map_err(|error| format!("Failed to write secure storage entry: {error}"))
}

pub fn delete_secret<R: Runtime>(_app: &AppHandle<R>, key: &str) -> Result<(), String> {
    let entry = keyring_core::Entry::new(SECRET_SERVICE_NAME, key)
        .map_err(|error| format!("Failed to open secure storage entry: {error}"))?;

    match entry.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(error) => Err(format!("Failed to delete secure storage entry: {error}")),
    }
}
