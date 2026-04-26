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

use crate::models::OpenOverlayWebviewRequest;

pub fn init<R: Runtime, C: DeserializeOwned>(
    app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> MobileWebviewOverlay<R> {
    MobileWebviewOverlay(app.clone())
}

/// Access to the mobile-webview-overlay APIs.
pub struct MobileWebviewOverlay<R: Runtime>(AppHandle<R>);

impl<R: Runtime> MobileWebviewOverlay<R> {
    pub fn open(&self, _payload: OpenOverlayWebviewRequest) -> crate::Result<()> {
        let _ = &self.0;
        Err(crate::Error::UnsupportedPlatform)
    }
}
