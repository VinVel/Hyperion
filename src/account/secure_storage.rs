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

use crate::native::host::AppHandle;
#[cfg(any(target_os = "android", target_os = "linux"))]
use std::collections::HashMap;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex, OnceLock},
};

// Matrix store encryption keys are grouped under this stable service name for
// compatibility with existing desktop credentials.
const SECRET_SERVICE_NAME: &str = "net.velcore.hyperion.matrix-store";

// Secret Service displays this label in KWallet Manager and GNOME Keyring.
// The lookup service stays stable so existing entries remain discoverable.
#[cfg(target_os = "linux")]
const LINUX_SECRET_LABEL: &str = "Hyperion";

// Android uses a named keyring-core store so Hyperion credentials stay isolated
// from any other Rust keyring users in the same application process.
#[cfg(target_os = "android")]
const ANDROID_STORE_NAME: &str = "hyperion-matrix-store";

// The native keyring store is shared process-wide, so initialize it once on
// first use. Android cannot create its store before the native host exposes
// the Activity context required by the keyring.
static DEFAULT_STORE_INITIALIZED: OnceLock<Mutex<bool>> = OnceLock::new();

pub fn unset_default_store() {
    drop(keyring_core::unset_default_store());
    if let Some(initialized) = DEFAULT_STORE_INITIALIZED.get() {
        match initialized.lock() {
            Ok(mut initialized) => {
                *initialized = false;
            }
            Err(poison_error) => {
                crate::utils::tracing::report_recoverable_error(
                    "account.secure_storage",
                    "shutdown",
                    "account.secure_storage_state_poisoned",
                    "storage",
                    &poison_error,
                );
            }
        }
    }
}

fn ensure_default_store(
    #[cfg(target_os = "android")] app: &AppHandle,
    #[cfg(not(target_os = "android"))] _app: &AppHandle,
) -> Result<(), String> {
    let initialized = DEFAULT_STORE_INITIALIZED.get_or_init(|| Mutex::new(false));
    let mut initialized = initialized.lock().map_err(|_poison_error| {
        String::from("Secure storage initialization state is not recoverable")
    })?;
    if *initialized {
        return Ok(());
    }

    #[cfg(target_os = "android")]
    initialize_android_context(app)?;

    let store_result = catch_unwind(AssertUnwindSafe(platform_default_store));
    let store = match store_result {
        Ok(Ok(store)) => store,
        Ok(Err(error)) => return Err(format!("Failed to initialize secure storage: {error}")),
        Err(_panic) => {
            return Err(String::from(
                "Failed to initialize secure storage because the native keyring panicked",
            ));
        }
    };
    keyring_core::set_default_store(store);
    *initialized = true;
    drop(initialized);
    Ok(())
}

#[cfg(target_os = "android")]
fn initialize_android_context(app: &AppHandle) -> Result<(), String> {
    let _ = app;
    unimplemented!("Qt Android host must expose the Activity context to secure storage")
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
    let store = zbus_secret_service_keyring_store::Store::new()?;
    Ok(store)
}

#[cfg(target_os = "android")]
fn platform_default_store() -> keyring_core::Result<Arc<keyring_core::CredentialStore>> {
    let mut configuration = HashMap::new();
    configuration.insert("name", ANDROID_STORE_NAME);

    let store = android_native_keyring_store::Store::new_with_configuration(&configuration)?;
    Ok(store)
}

pub fn get_secret(app: &AppHandle, key: &str) -> Result<Option<Vec<u8>>, String> {
    run_keyring_operation(|| get_secret_inner(app, key))
}

fn get_secret_inner(app: &AppHandle, key: &str) -> Result<Option<Vec<u8>>, String> {
    ensure_default_store(app).map_err(secure_storage_unavailable)?;
    let entry = open_secret_entry(key)?;

    match entry.get_secret() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(error) => Err(secure_storage_unavailable(format!(
            "Failed to read secure storage entry: {error}"
        ))),
    }
}

pub fn set_secret(app: &AppHandle, key: &str, value: &[u8]) -> Result<(), String> {
    run_keyring_operation(|| set_secret_inner(app, key, value))
}

fn set_secret_inner(app: &AppHandle, key: &str, value: &[u8]) -> Result<(), String> {
    ensure_default_store(app).map_err(secure_storage_unavailable)?;
    let entry = open_secret_entry(key)?;

    entry.set_secret(value).map_err(|error| {
        secure_storage_unavailable(format!("Failed to write secure storage entry: {error}"))
    })
}

pub fn delete_secret(app: &AppHandle, key: &str) -> Result<(), String> {
    run_keyring_operation(|| delete_secret_inner(app, key))
}

fn delete_secret_inner(app: &AppHandle, key: &str) -> Result<(), String> {
    ensure_default_store(app).map_err(secure_storage_unavailable)?;
    let entry = open_secret_entry(key)?;

    match entry.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(error) => Err(secure_storage_unavailable(format!(
            "Failed to delete secure storage entry: {error}"
        ))),
    }
}

fn run_keyring_operation<T>(operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    // zbus-secret-service-keyring-store exposes the keyring-core synchronous API
    // and internally creates a Tokio runtime for each Secret Service call. Exit
    // Hyperion's worker runtime first so its blocking API cannot nest runtimes.
    tokio::task::block_in_place(operation)
}

fn open_secret_entry(key: &str) -> Result<keyring_core::Entry, String> {
    #[cfg(target_os = "linux")]
    let entry_result = {
        let modifiers = HashMap::from([("label", LINUX_SECRET_LABEL)]);
        keyring_core::Entry::new_with_modifiers(SECRET_SERVICE_NAME, key, &modifiers)
    };

    #[cfg(not(target_os = "linux"))]
    let entry_result = keyring_core::Entry::new(SECRET_SERVICE_NAME, key);

    entry_result.map_err(|error| {
        secure_storage_unavailable(format!("Failed to open secure storage entry: {error}"))
    })
}

fn secure_storage_unavailable(detail: impl std::fmt::Display) -> String {
    // The stable prefix is safe to expose over IPC; detailed provider errors stay
    // in backend diagnostics rather than becoming UI-visible implementation data.
    crate::utils::tracing::report_recoverable_error(
        "account.secure_storage",
        "access_secret_service",
        "account.secure_storage_unavailable",
        "storage",
        &detail,
    );
    String::from("secure_storage_unavailable: encrypted account data cannot be unlocked")
}

#[cfg(test)]
mod tests {
    use super::run_keyring_operation;

    #[tokio::test(flavor = "multi_thread")]
    async fn keyring_operation_can_start_a_blocking_provider_runtime() {
        let result = run_keyring_operation(|| {
            let provider_runtime = tokio::runtime::Runtime::new()
                .map_err(|error| format!("failed to build provider runtime: {error}"))?;
            Ok::<u8, String>(provider_runtime.block_on(async { 7 }))
        });

        assert_eq!(result, Ok(7));
    }
}
