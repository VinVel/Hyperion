use tauri::{
    AppHandle, Manager, Runtime,
    plugin::{Builder, TauriPlugin},
};

pub use models::*;

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

mod error;
mod models;

pub use error::{Error, Result};

#[cfg(desktop)]
use desktop::MobileWebviewOverlay;
#[cfg(mobile)]
use mobile::MobileWebviewOverlay;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the mobile-webview-overlay APIs.
pub trait MobileWebviewOverlayExt<R: Runtime> {
    fn mobile_webview_overlay(&self) -> &MobileWebviewOverlay<R>;
}

impl<R: Runtime, T: Manager<R>> crate::MobileWebviewOverlayExt<R> for T {
    fn mobile_webview_overlay(&self) -> &MobileWebviewOverlay<R> {
        self.state::<MobileWebviewOverlay<R>>().inner()
    }
}

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("mobile-webview-overlay")
        .setup(|app, api| {
            #[cfg(mobile)]
            let mobile_webview_overlay = mobile::init(app, api)?;
            #[cfg(desktop)]
            let mobile_webview_overlay = desktop::init(app, api)?;
            app.manage(mobile_webview_overlay);
            Ok(())
        })
        .build()
}

pub fn open_url<R: Runtime>(
    app: &AppHandle<R>,
    url: &str,
    title: Option<&str>,
    user_agent: Option<&str>,
) -> Result<()> {
    app.mobile_webview_overlay()
        .open(OpenOverlayWebviewRequest {
            url: url.to_owned(),
            title: title.map(str::to_owned),
            user_agent: user_agent.map(str::to_owned),
        })
}
