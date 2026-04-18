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

use tauri::{
    AppHandle, Manager, Runtime,
    plugin::{Builder, TauriPlugin},
};

pub use models::*;

#[cfg(not(target_os = "android"))]
mod desktop;
#[cfg(target_os = "android")]
mod mobile;

mod error;
mod models;

pub use error::{Error, Result};

#[cfg(not(target_os = "android"))]
use desktop::AndroidSecureStorage;
#[cfg(target_os = "android")]
use mobile::AndroidSecureStorage;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the android-secure-storage APIs.
pub trait AndroidSecureStorageExt<R: Runtime> {
    fn android_secure_storage(&self) -> &AndroidSecureStorage<R>;
}

impl<R: Runtime, T: Manager<R>> crate::AndroidSecureStorageExt<R> for T {
    fn android_secure_storage(&self) -> &AndroidSecureStorage<R> {
        self.state::<AndroidSecureStorage<R>>().inner()
    }
}

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("android-secure-storage")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            let android_secure_storage = mobile::init(app, api)?;
            #[cfg(not(target_os = "android"))]
            let android_secure_storage = desktop::init(app, api)?;
            app.manage(android_secure_storage);
            Ok(())
        })
        .build()
}

pub fn get_secret<R: Runtime>(app: &AppHandle<R>, key: &str) -> Result<Option<Vec<u8>>> {
    app.android_secure_storage().get_secret(key)
}

pub fn set_secret<R: Runtime>(app: &AppHandle<R>, key: &str, value: &[u8]) -> Result<()> {
    app.android_secure_storage().set_secret(key, value)
}

pub fn delete_secret<R: Runtime>(app: &AppHandle<R>, key: &str) -> Result<()> {
    app.android_secure_storage().delete_secret(key)
}
