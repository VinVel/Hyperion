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

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::de::DeserializeOwned;
use tauri::{
    AppHandle, Runtime,
    plugin::{PluginApi, PluginHandle},
};

use crate::models::*;

const PLUGIN_IDENTIFIER: &str = "net.velcore.hyperion.androidsecurestorage";

// initializes the Kotlin or Swift plugin classes
pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<AndroidSecureStorage<R>> {
    let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "AndroidSecureStoragePlugin")?;
    Ok(AndroidSecureStorage(handle))
}

/// Access to the android-secure-storage APIs.
pub struct AndroidSecureStorage<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> AndroidSecureStorage<R> {
    pub fn get_secret(&self, key: &str) -> crate::Result<Option<Vec<u8>>> {
        let response = self.0.run_mobile_plugin::<GetSecretResponse>(
            "getSecret",
            SecretKeyRequest {
                key: key.to_owned(),
            },
        )?;

        response
            .value_base64
            .map(|value| {
                STANDARD
                    .decode(value)
                    .map_err(|error| crate::Error::Decode(error.to_string()))
            })
            .transpose()
    }

    pub fn set_secret(&self, key: &str, value: &[u8]) -> crate::Result<()> {
        self.0
            .run_mobile_plugin::<serde_json::Value>(
                "setSecret",
                SetSecretRequest {
                    key: key.to_owned(),
                    value_base64: STANDARD.encode(value),
                },
            )
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn delete_secret(&self, key: &str) -> crate::Result<()> {
        self.0
            .run_mobile_plugin::<serde_json::Value>(
                "deleteSecret",
                SecretKeyRequest {
                    key: key.to_owned(),
                },
            )
            .map(|_| ())
            .map_err(Into::into)
    }
}
