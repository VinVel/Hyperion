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

mod account;
mod mobile_custom_tabs;
mod secure_storage;

use account::{
    AccountManager, AccountSummary, HomeserverDirectory, LoginRequest, RegisterAccountRequest,
    RegistrationOutcome,
};
use tauri::{AppHandle, State};
use tauri_plugin_mobile_webview_overlay as mobile_overlay_webview;

#[tauri::command]
async fn login_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
    request: LoginRequest,
) -> Result<AccountSummary, String> {
    manager.login(&app, request).await
}

#[tauri::command]
async fn list_accounts(
    app: AppHandle,
    manager: State<'_, AccountManager>,
) -> Result<Vec<AccountSummary>, String> {
    manager.list_accounts(&app).await
}

#[tauri::command]
async fn switch_active_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
    account_key: String,
) -> Result<(), String> {
    manager.switch_active_account(&app, &account_key).await
}

#[tauri::command]
async fn active_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
) -> Result<Option<AccountSummary>, String> {
    manager.active_account(&app).await
}

#[tauri::command]
async fn validate_active_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
) -> Result<Option<AccountSummary>, String> {
    manager.validate_active_account(&app).await
}

#[tauri::command]
async fn list_registration_homeservers(
    manager: State<'_, AccountManager>,
) -> Result<HomeserverDirectory, String> {
    manager.list_registration_homeservers().await
}

#[tauri::command]
async fn register_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
    request: RegisterAccountRequest,
) -> Result<RegistrationOutcome, String> {
    manager.register_account(&app, request).await
}

#[tauri::command]
async fn open_android_custom_tab(app: AppHandle, url: String) -> Result<(), String> {
    mobile_custom_tabs::open_url(&app, &url)
}

#[tauri::command]
async fn open_mobile_overlay_webview(
    app: AppHandle,
    url: String,
    title: Option<String>,
    user_agent: Option<String>,
) -> Result<(), String> {
    mobile_overlay_webview::open_url(&app, &url, title.as_deref(), user_agent.as_deref())
        .map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AccountManager::new())
        .plugin(mobile_custom_tabs::init())
        .plugin(mobile_overlay_webview::init())
        .plugin(secure_storage::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            login_account,
            list_accounts,
            switch_active_account,
            active_account,
            validate_active_account,
            list_registration_homeservers,
            register_account,
            open_android_custom_tab,
            open_mobile_overlay_webview
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
