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

use serde::de::DeserializeOwned;
use tauri::{AppHandle, Runtime, plugin::PluginApi};

pub fn init<R: Runtime, C: DeserializeOwned>(
    app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> crate::Result<AndroidSecureStorage<R>> {
    Ok(AndroidSecureStorage(app.clone()))
}

/// Access to the android-secure-storage APIs.
pub struct AndroidSecureStorage<R: Runtime>(AppHandle<R>);

impl<R: Runtime> AndroidSecureStorage<R> {
    pub fn get_secret(&self, _key: &str) -> crate::Result<Option<Vec<u8>>> {
        Err(crate::Error::UnsupportedPlatform)
    }

    pub fn set_secret(&self, _key: &str, _value: &[u8]) -> crate::Result<()> {
        Err(crate::Error::UnsupportedPlatform)
    }

    pub fn delete_secret(&self, _key: &str) -> crate::Result<()> {
        Err(crate::Error::UnsupportedPlatform)
    }
}
