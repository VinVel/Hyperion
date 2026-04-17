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

#[cfg(mobile)]
use serde::Serialize;
use tauri::{
    AppHandle, Runtime,
    plugin::{Builder, TauriPlugin},
};

#[cfg(target_os = "android")]
use tauri::Manager;

#[cfg(mobile)]
use tauri::plugin::PluginHandle;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_mobile_overlay_webview);

#[cfg(mobile)]
pub struct MobileOverlayWebview<R: Runtime>(PluginHandle<R>);

#[cfg(mobile)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenOverlayWebviewRequest {
    url: String,
    title: Option<String>,
    user_agent: Option<String>,
}

#[cfg(mobile)]
impl<R: Runtime> MobileOverlayWebview<R> {
    pub fn open_url(
        &self,
        url: &str,
        title: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<(), String> {
        self.0
            .run_mobile_plugin::<serde_json::Value>(
                "open",
                OpenOverlayWebviewRequest {
                    url: url.to_owned(),
                    title: title.map(str::to_owned),
                    user_agent: user_agent.map(str::to_owned),
                },
            )
            .map(|_| ())
            .map_err(|error| format!("Failed to open mobile overlay webview: {error}"))
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("mobile-overlay-webview")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            let handle =
                _api.register_android_plugin("net.velcore.hyperion", "MobileOverlayWebViewPlugin")?;

            #[cfg(target_os = "ios")]
            let handle = _api.register_ios_plugin(init_plugin_mobile_overlay_webview)?;

            #[cfg(mobile)]
            _app.manage(MobileOverlayWebview(handle));

            Ok(())
        })
        .build()
}

pub fn open_url<R: Runtime>(
    app: &AppHandle<R>,
    url: &str,
    title: Option<&str>,
    user_agent: Option<&str>,
) -> Result<(), String> {
    #[cfg(mobile)]
    {
        let overlay_webview = app.state::<MobileOverlayWebview<R>>();
        return overlay_webview.open_url(url, title, user_agent);
    }

    #[cfg(not(mobile))]
    {
        let _ = (app, url, title, user_agent);
        Err(String::from(
            "Mobile overlay webview is only available on Android and iOS",
        ))
    }
}
