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
use serde::Serialize;
use tauri::{
    AppHandle, Runtime,
    plugin::{Builder, TauriPlugin},
};

#[cfg(target_os = "android")]
use tauri::Manager;

#[cfg(target_os = "android")]
use tauri::plugin::PluginHandle;

#[cfg(target_os = "android")]
pub struct AndroidCustomTabs<R: Runtime>(PluginHandle<R>);

#[cfg(target_os = "android")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenCustomTabRequest {
    url: String,
}

#[cfg(target_os = "android")]
impl<R: Runtime> AndroidCustomTabs<R> {
    pub fn open_url(&self, url: &str) -> Result<(), String> {
        self.0
            .run_mobile_plugin::<serde_json::Value>(
                "openUrl",
                OpenCustomTabRequest {
                    url: url.to_owned(),
                },
            )
            .map(|_| ())
            .map_err(|error| format!("Failed to open Android Custom Tab: {error}"))
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("android-custom-tabs")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                let handle =
                    _api.register_android_plugin("net.velcore.hyperion", "CustomTabsPlugin")?;
                _app.manage(AndroidCustomTabs(handle));
            }

            Ok(())
        })
        .build()
}

pub fn open_url<R: Runtime>(app: &AppHandle<R>, url: &str) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let custom_tabs = app.state::<AndroidCustomTabs<R>>();
        return custom_tabs.open_url(url);
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, url);
        Err(String::from(
            "Android Custom Tabs are only available on Android",
        ))
    }
}
