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

use account::{AccountManager, AccountSummary, LoginRequest};
use tauri::{AppHandle, State};

#[tauri::command]
async fn login_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
    request: LoginRequest,
) -> Result<AccountSummary, String> {
    manager.login(&app, request).await
}

#[tauri::command]
async fn list_accounts(manager: State<'_, AccountManager>) -> Result<Vec<AccountSummary>, String> {
    Ok(manager.list_accounts().await)
}

#[tauri::command]
async fn switch_active_account(
    manager: State<'_, AccountManager>,
    account_key: String,
) -> Result<(), String> {
    manager.switch_active_account(&account_key).await
}

#[tauri::command]
async fn active_account(
    manager: State<'_, AccountManager>,
) -> Result<Option<AccountSummary>, String> {
    Ok(manager.active_account().await)
}

#[tauri::command]
async fn validate_active_account(
    manager: State<'_, AccountManager>,
) -> Result<Option<AccountSummary>, String> {
    manager.validate_active_account().await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AccountManager::new())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            login_account,
            list_accounts,
            switch_active_account,
            active_account,
            validate_active_account
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
