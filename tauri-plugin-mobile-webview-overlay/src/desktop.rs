use serde::de::DeserializeOwned;
use tauri::{AppHandle, Runtime, plugin::PluginApi};

use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
    app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> crate::Result<MobileWebviewOverlay<R>> {
    Ok(MobileWebviewOverlay(app.clone()))
}

/// Access to the mobile-webview-overlay APIs.
pub struct MobileWebviewOverlay<R: Runtime>(AppHandle<R>);

impl<R: Runtime> MobileWebviewOverlay<R> {
    pub fn open(&self, _payload: OpenOverlayWebviewRequest) -> crate::Result<()> {
        Err(crate::Error::UnsupportedPlatform)
    }
}
