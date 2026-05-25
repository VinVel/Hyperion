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
use std::{collections::HashMap, sync::mpsc};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex, OnceLock},
};
#[cfg(target_os = "android")]
use tauri::Manager;
use tauri::{AppHandle, Runtime};

// Matrix store encryption keys are grouped under this stable service name for
// compatibility with existing desktop credentials.
const SECRET_SERVICE_NAME: &str = "net.velcore.hyperion.matrix-store";

// Android uses a named keyring-core store so Hyperion credentials stay isolated
// from any other Rust keyring users in the same application process.
#[cfg(target_os = "android")]
const ANDROID_STORE_NAME: &str = "hyperion-matrix-store";

// The native keyring store is shared process-wide, so initialize it once on
// first use. Android cannot create its store during Tauri setup because the
// native keyring needs the Activity context that Tauri exposes after startup.
static DEFAULT_STORE_INITIALIZED: OnceLock<Mutex<bool>> = OnceLock::new();

pub fn unset_default_store() {
    drop(keyring_core::unset_default_store());
    if let Some(initialized) = DEFAULT_STORE_INITIALIZED.get() {
        match initialized.lock() {
            Ok(mut initialized) => {
                *initialized = false;
            }
            Err(_poison_error) => {
                eprintln!("Secure storage initialization state was poisoned during shutdown");
            }
        }
    }
}

fn ensure_default_store<R: Runtime>(
    #[cfg(target_os = "android")] app: &AppHandle<R>,
    #[cfg(not(target_os = "android"))] _app: &AppHandle<R>,
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
fn initialize_android_context<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let Some(webview) = app.webviews().into_values().next() else {
        return Err(String::from(
            "Secure storage is not available until the Android webview is ready",
        ));
    };
    let (sender, receiver) = mpsc::channel();

    webview
        .with_webview(move |platform_webview| {
            platform_webview
                .jni_handle()
                .exec(move |env, activity, _webview| {
                    let result = env
                        .get_java_vm()
                        .map_err(|error| format!("Failed to read the Android Java VM: {error}"))
                        .map(|java_vm| {
                            let vm = java_vm.get_java_vm_pointer().cast();
                            let context = activity.as_raw().cast();
                            // android-native-keyring-store reads this process-global context
                            // when opening its SharedPreferences/KeyStore vault.
                            unsafe {
                                ndk_context::initialize_android_context(vm, context);
                            }
                        });

                    drop(sender.send(result));
                });
        })
        .map_err(|error| format!("Failed to access the Android webview: {error}"))?;

    receiver
        .recv()
        .map_err(|error| format!("Failed to receive Android secure-storage context: {error}"))?
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

pub fn get_secret<R: Runtime>(app: &AppHandle<R>, key: &str) -> Result<Option<Vec<u8>>, String> {
    ensure_default_store(app)?;
    let entry = keyring_core::Entry::new(SECRET_SERVICE_NAME, key)
        .map_err(|error| format!("Failed to open secure storage entry: {error}"))?;

    match entry.get_secret() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("Failed to read secure storage entry: {error}")),
    }
}

pub fn set_secret<R: Runtime>(app: &AppHandle<R>, key: &str, value: &[u8]) -> Result<(), String> {
    ensure_default_store(app)?;
    let entry = keyring_core::Entry::new(SECRET_SERVICE_NAME, key)
        .map_err(|error| format!("Failed to open secure storage entry: {error}"))?;

    entry
        .set_secret(value)
        .map_err(|error| format!("Failed to write secure storage entry: {error}"))
}

pub fn delete_secret<R: Runtime>(app: &AppHandle<R>, key: &str) -> Result<(), String> {
    ensure_default_store(app)?;
    let entry = keyring_core::Entry::new(SECRET_SERVICE_NAME, key)
        .map_err(|error| format!("Failed to open secure storage entry: {error}"))?;

    match entry.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(error) => Err(format!("Failed to delete secure storage entry: {error}")),
    }
}
