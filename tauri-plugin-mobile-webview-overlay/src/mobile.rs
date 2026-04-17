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
use tauri::{
    AppHandle, Runtime,
    plugin::{PluginApi, PluginHandle},
};

use crate::models::*;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_mobile_webview_overlay);
#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "net.velcore.hyperion.mobilewebviewoverlay";

// initializes the Kotlin or Swift plugin classes
pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<MobileWebviewOverlay<R>> {
    #[cfg(target_os = "android")]
    let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "MobileWebviewOverlayPlugin")?;
    #[cfg(target_os = "ios")]
    let handle = api.register_ios_plugin(init_plugin_mobile_webview_overlay)?;
    Ok(MobileWebviewOverlay(handle))
}

/// Access to the mobile-webview-overlay APIs.
pub struct MobileWebviewOverlay<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> MobileWebviewOverlay<R> {
    pub fn open(&self, payload: OpenOverlayWebviewRequest) -> crate::Result<()> {
        self.0
            .run_mobile_plugin::<serde_json::Value>("open", payload)
            .map(|_| ())
            .map_err(Into::into)
    }
}
