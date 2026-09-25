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
#![recursion_limit = "256"]

mod account;
pub mod cxxqt_object;
mod native;
mod settings;
mod shell;
mod utils;

use cxx_qt::casting::Upcast;
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQmlEngine, QString, QUrl};
use std::pin::Pin;

// Keep Qt's app-specific data locations aligned with the previous Tauri bundle identifier.
const APPLICATION_IDENTIFIER: &str = "net.velcore.hyperion";

fn main() {
    configure_linux_wayland_portal_theme();

    // Create the application and engine
    let mut app = QGuiApplication::new();
    if let Some(app) = app.as_mut() {
        app.set_application_name(&QString::from(APPLICATION_IDENTIFIER));
    }

    let mut engine = QQmlApplicationEngine::new();

    // Load the QML path into the engine
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/net/velcore/hyperion/qml/main.qml"));
    }

    if let Some(engine) = engine.as_mut() {
        let engine: Pin<&mut QQmlEngine> = engine.upcast_pin();
        // Listen to a signal from the QML Engine
        engine
            .on_quit(|_| {
                println!("QML Quit!");
            })
            .release();
    }

    // Start the app
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}

#[cfg(target_os = "linux")]
fn configure_linux_wayland_portal_theme() {
    use std::env;

    // Qt's Wayland platform theme uses the desktop portal for native file dialogs.
    const XDG_PORTAL_PLATFORM_THEME: &str = "xdgdesktopportal";

    if env::var_os("QT_QPA_PLATFORMTHEME").is_some() {
        return;
    }

    let is_wayland_session = env::var("XDG_SESSION_TYPE")
        .is_ok_and(|session_type| session_type.eq_ignore_ascii_case("wayland"));
    let has_wayland_display = env::var_os("WAYLAND_DISPLAY").is_some();
    if !(is_wayland_session || has_wayland_display) {
        return;
    }

    // This runs before QGuiApplication creation and before the process starts worker threads.
    unsafe {
        env::set_var("QT_QPA_PLATFORMTHEME", XDG_PORTAL_PLATFORM_THEME);
    }
}

#[cfg(not(target_os = "linux"))]
fn configure_linux_wayland_portal_theme() {}
