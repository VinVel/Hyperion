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
use base64::{Engine as _, engine::general_purpose::STANDARD};
#[cfg(target_os = "android")]
use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Runtime,
    plugin::{Builder, TauriPlugin},
};

#[cfg(target_os = "android")]
use tauri::Manager;

#[cfg(target_os = "android")]
use tauri::plugin::PluginHandle;

#[cfg(not(target_os = "android"))]
const SECRET_SERVICE_NAME: &str = "net.velcore.hyperion.matrix-store";

#[cfg(target_os = "android")]
pub struct AndroidSecureStore<R: Runtime>(PluginHandle<R>);

#[cfg(target_os = "android")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SecretKeyRequest {
    key: String,
}

#[cfg(target_os = "android")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetSecretRequest {
    key: String,
    value_base64: String,
}

#[cfg(target_os = "android")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetSecretResponse {
    value_base64: Option<String>,
}

#[cfg(target_os = "android")]
impl<R: Runtime> AndroidSecureStore<R> {
    pub fn get_secret(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        let response = self
            .0
            .run_mobile_plugin::<GetSecretResponse>(
                "getSecret",
                SecretKeyRequest {
                    key: key.to_owned(),
                },
            )
            .map_err(|error| format!("Failed to read Android secure storage: {error}"))?;

        response
            .value_base64
            .map(|value| {
                STANDARD
                    .decode(value)
                    .map_err(|error| format!("Failed to decode Android secure secret: {error}"))
            })
            .transpose()
    }

    pub fn set_secret(&self, key: &str, value: &[u8]) -> Result<(), String> {
        self.0
            .run_mobile_plugin::<serde_json::Value>(
                "setSecret",
                SetSecretRequest {
                    key: key.to_owned(),
                    value_base64: STANDARD.encode(value),
                },
            )
            .map(|_| ())
            .map_err(|error| format!("Failed to write Android secure storage: {error}"))
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("secure-store")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                let handle =
                    _api.register_android_plugin("net.velcore.hyperion", "SecureStorePlugin")?;
                _app.manage(AndroidSecureStore(handle));
            }

            Ok(())
        })
        .build()
}

pub fn get_secret<R: Runtime>(app: &AppHandle<R>, key: &str) -> Result<Option<Vec<u8>>, String> {
    #[cfg(target_os = "android")]
    {
        let secure_store = app.state::<AndroidSecureStore<R>>();
        return secure_store.get_secret(key);
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
        let secure_store = app.state::<AndroidSecureStore<R>>();
        return secure_store.set_secret(key, value);
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
