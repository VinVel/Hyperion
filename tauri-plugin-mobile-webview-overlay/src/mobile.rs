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
