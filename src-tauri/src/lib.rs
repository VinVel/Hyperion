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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AccountManager::new())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            login_account,
            list_accounts,
            switch_active_account,
            active_account
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
