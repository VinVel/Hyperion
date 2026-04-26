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
#![warn(clippy::pedantic)]

mod account;
mod settings;
mod shell;

use account::{
    AccountManager, AccountSummary, HomeserverDirectory, LoginRequest, RegisterAccountRequest,
    RegistrationOutcome,
};
use settings::{
    get_theme_mode as load_theme_mode, get_theme_preset as load_theme_preset,
    set_theme_mode as save_theme_mode, set_theme_preset as save_theme_preset,
};
use shell::{
    GetRoomEventContextRequest, GetRoomSummaryRequest, GetRoomTimelineRequest, GlobalSearchRequest,
    GlobalSearchResponse, ListRoomThreadsRequest, ListSpacesRequest, RoomSummary,
    RoomThreadSummary, RoomTimeline, SendRoomMessageRequest, SendRoomMessageResponse, ShellManager,
    SpaceSummary,
};
use tauri::{AppHandle, State};

use tauri_plugin_android_secure_storage as android_secure_storage;
use tauri_plugin_mobile_webview_overlay as mobile_overlay_webview;

#[tauri::command]
async fn login_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: LoginRequest,
) -> Result<AccountSummary, String> {
    let account = manager.login(&app, request).await?;
    shell_manager
        .ensure_active_account_sync(&app, &manager)
        .await?;
    Ok(account)
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
    shell_manager: State<'_, ShellManager>,
    account_key: String,
) -> Result<(), String> {
    manager.switch_active_account(&app, &account_key).await?;
    shell_manager
        .ensure_active_account_sync(&app, &manager)
        .await
}

#[tauri::command]
async fn active_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
) -> Result<Option<AccountSummary>, String> {
    manager.active_account(&app).await
}

#[tauri::command]
async fn sign_out_active_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
) -> Result<Option<AccountSummary>, String> {
    let active_account = manager.active_account(&app).await?;
    if let Some(account) = active_account {
        shell_manager.stop_account(&account.account_key).await;
    }

    let next_account = manager.sign_out_active_account(&app).await?;
    if next_account.is_some() {
        shell_manager
            .ensure_active_account_sync(&app, &manager)
            .await?;
    } else {
        shell_manager.stop_all_accounts().await;
    }

    Ok(next_account)
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
async fn list_room_threads(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: Option<ListRoomThreadsRequest>,
) -> Result<Vec<RoomThreadSummary>, String> {
    shell_manager
        .list_room_threads(
            &app,
            &account_manager,
            request.unwrap_or(ListRoomThreadsRequest { search_query: None }),
        )
        .await
}

#[tauri::command]
async fn get_room_summary(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: GetRoomSummaryRequest,
) -> Result<RoomSummary, String> {
    shell_manager
        .get_room_summary(&app, &account_manager, request)
        .await
}

#[tauri::command]
async fn get_room_timeline(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: GetRoomTimelineRequest,
) -> Result<RoomTimeline, String> {
    shell_manager
        .get_room_timeline(&app, &account_manager, request)
        .await
}

#[tauri::command]
async fn get_room_event_context(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: GetRoomEventContextRequest,
) -> Result<RoomTimeline, String> {
    shell_manager
        .get_room_event_context(&app, &account_manager, request)
        .await
}

#[tauri::command]
async fn send_room_message(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: SendRoomMessageRequest,
) -> Result<SendRoomMessageResponse, String> {
    shell_manager
        .send_room_message(&app, &account_manager, request)
        .await
}

#[tauri::command]
async fn list_spaces(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: Option<ListSpacesRequest>,
) -> Result<Vec<SpaceSummary>, String> {
    shell_manager
        .list_spaces(
            &app,
            &account_manager,
            request.unwrap_or(ListSpacesRequest { search_query: None }),
        )
        .await
}

#[tauri::command]
async fn global_search(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: GlobalSearchRequest,
) -> Result<GlobalSearchResponse, String> {
    shell_manager
        .global_search(&app, &account_manager, request)
        .await
}

#[tauri::command]
async fn open_mobile_overlay_webview(
    app: AppHandle,
    url: String,
    title: Option<String>,
    user_agent: Option<String>,
) -> Result<(), String> {
    let resolved_user_agent = user_agent
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or(mobile_overlay_webview::default_desktop_user_agent());

    mobile_overlay_webview::open_url(&app, &url, title.as_deref(), Some(resolved_user_agent))
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn get_theme_preset(
    app: AppHandle,
    supported_presets: Vec<String>,
    default_preset: String,
) -> Result<String, String> {
    load_theme_preset(&app, &supported_presets, &default_preset)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn set_theme_preset(
    app: AppHandle,
    preset: String,
    supported_presets: Vec<String>,
    default_preset: String,
) -> Result<String, String> {
    save_theme_preset(&app, &preset, &supported_presets, &default_preset)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn get_theme_mode(app: AppHandle) -> Result<String, String> {
    load_theme_mode(&app)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn set_theme_mode(app: AppHandle, mode: String) -> Result<String, String> {
    save_theme_mode(&app, &mode)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Runs the Tauri application.
///
/// # Panics
///
/// Panics if Tauri fails to initialize or the application runtime exits with an
/// unrecoverable error.
pub fn run() {
    tauri::Builder::default()
        .manage(AccountManager::new())
        .manage(ShellManager::new())
        .plugin(android_secure_storage::init())
        .plugin(mobile_overlay_webview::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            login_account,
            list_accounts,
            switch_active_account,
            active_account,
            sign_out_active_account,
            validate_active_account,
            list_registration_homeservers,
            register_account,
            list_room_threads,
            get_room_summary,
            get_room_timeline,
            get_room_event_context,
            send_room_message,
            list_spaces,
            global_search,
            open_mobile_overlay_webview,
            get_theme_mode,
            get_theme_preset,
            set_theme_mode,
            set_theme_preset
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
