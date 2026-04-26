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

use matrix_sdk::{
    Client,
    encryption::CrossSigningResetAuthType,
    ruma::{events::GlobalAccountDataEventType, serde::Raw},
};
use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, FilePath};

use crate::{account::AccountManager, shell::ShellManager};

pub use super::types::{
    CryptoIdentityResetOutcome, EncryptionOverview, GeneratedRecoveryKey, RecoveryKeyRequest,
    RoomKeyFileRequest, RoomKeyImportSummary,
};

// Matrix has no account-data DELETE endpoint, so disabling recovery is represented by an empty default-key event.
const SECRET_STORAGE_DEFAULT_KEY_EVENT_TYPE: &str = "m.secret_storage.default_key";
// Matrix Rust SDK uses this custom marker to prevent automatic backup re-creation after recovery deletion.
const BACKUP_DISABLED_EVENT_TYPE: &str = "m.org.matrix.custom.backup_disabled";

#[tauri::command]
pub async fn get_encryption_overview(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
) -> Result<EncryptionOverview, String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Ok(EncryptionOverview {
            has_active_account: false,
            account_key: None,
            user_id: None,
            device_id: None,
            ed25519_key: None,
            curve25519_key: None,
            recovery_state: None,
            backup_state: None,
            server_key_storage_opted_out: false,
            verified_devices_only: false,
        });
    };

    let preferences = AccountManager::load_encryption_preferences(&account.client).await?;
    let encryption = account.client.encryption();
    let own_device = encryption
        .get_own_device()
        .await
        .map_err(|error| format!("Failed to read this device's encryption keys: {error}"))?;
    let backups = encryption.backups();
    let server_backup_exists = backups.fetch_exists_on_server().await.ok();
    let server_backup_enabled = backups.are_enabled().await;
    let secret_storage_enabled = encryption
        .secret_storage()
        .is_enabled()
        .await
        .map_err(|error| format!("Failed to read secret storage state: {error}"))?;
    let cross_signing_complete = encryption
        .cross_signing_status()
        .await
        .is_some_and(|status| status.is_complete());
    let backup_available = server_backup_enabled
        || server_backup_exists.unwrap_or_default()
        || preferences.server_key_storage_opted_out;
    let recovery_state = if !secret_storage_enabled {
        String::from("Disabled")
    } else if cross_signing_complete && backup_available {
        String::from("Enabled")
    } else {
        String::from("Incomplete")
    };

    Ok(EncryptionOverview {
        has_active_account: true,
        account_key: Some(account.account_key),
        user_id: account.client.user_id().map(ToString::to_string),
        device_id: account.client.device_id().map(ToString::to_string),
        ed25519_key: own_device
            .as_ref()
            .and_then(|device| device.ed25519_key())
            .map(|key| key.to_base64()),
        curve25519_key: own_device
            .as_ref()
            .and_then(|device| device.curve25519_key())
            .map(|key| key.to_base64()),
        recovery_state: Some(recovery_state),
        backup_state: Some(backup_state_label(
            server_backup_enabled,
            server_backup_exists,
        )),
        server_key_storage_opted_out: preferences.server_key_storage_opted_out,
        verified_devices_only: preferences.verified_devices_only,
    })
}

#[tauri::command]
pub async fn enable_server_key_storage(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
) -> Result<(), String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };

    let mut preferences = AccountManager::load_encryption_preferences(&account.client).await?;
    account
        .client
        .encryption()
        .backups()
        .create()
        .await
        .map_err(|error| format!("Failed to enable server-side key backup: {error}"))?;
    preferences.server_key_storage_opted_out = false;
    AccountManager::persist_encryption_preferences(&account.client, &preferences).await
}

#[tauri::command]
pub async fn disable_server_key_storage(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
) -> Result<(), String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };

    let mut preferences = AccountManager::load_encryption_preferences(&account.client).await?;
    account
        .client
        .encryption()
        .backups()
        .disable_and_delete()
        .await
        .map_err(|error| format!("Failed to disable server-side key backup: {error}"))?;
    preferences.server_key_storage_opted_out = true;
    AccountManager::persist_encryption_preferences(&account.client, &preferences).await
}

#[tauri::command]
pub async fn create_recovery_key(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
) -> Result<GeneratedRecoveryKey, String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };

    let recovery_key = enable_recovery_with_clean_backup(&account.client).await?;

    Ok(GeneratedRecoveryKey {
        recovery_key: recovery_key.clone(),
    })
}

#[tauri::command]
pub async fn rotate_recovery_key(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
) -> Result<GeneratedRecoveryKey, String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };

    let recovery_key = account
        .client
        .encryption()
        .recovery()
        .reset_key()
        .await
        .map_err(|error| format!("Failed to rotate recovery key: {error}"))?;

    Ok(GeneratedRecoveryKey {
        recovery_key: recovery_key.clone(),
    })
}

#[tauri::command]
pub async fn delete_recovery(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
) -> Result<(), String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };

    let recovery_result = account.client.encryption().recovery().disable().await;
    account
        .client
        .encryption()
        .backups()
        .disable_and_delete()
        .await
        .map_err(|error| format!("Failed to delete the server key backup: {error}"))?;

    if let Err(error) = recovery_result {
        mark_recovery_account_data_disabled(&account.client).await?;
        if !is_backup_not_enabled_error(&error.to_string()) {
            return Err(format!("Failed to delete recovery: {error}"));
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn recover_with_recovery_key(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
    request: RecoveryKeyRequest,
) -> Result<(), String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };
    let recovery_key = request.recovery_key.trim();
    if recovery_key.is_empty() {
        return Err(String::from("Recovery key must not be empty"));
    }
    let recovery_is_configured = account
        .client
        .encryption()
        .secret_storage()
        .is_enabled()
        .await
        .map_err(|error| format!("Failed to read recovery state: {error}"))?;
    if !recovery_is_configured {
        return Err(String::from(
            "Recovery is disabled for this account. Create a new recovery key before recovering secrets.",
        ));
    }

    account
        .client
        .encryption()
        .recovery()
        .recover(recovery_key)
        .await
        .map_err(recover_error_message)
}

#[tauri::command]
pub async fn export_room_keys(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
    request: RoomKeyFileRequest,
) -> Result<Option<String>, String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };
    let passphrase = normalized_passphrase(&request.passphrase)?;
    let Some(path) = room_key_export_path(&app)? else {
        return Ok(None);
    };

    account
        .client
        .encryption()
        .export_room_keys(path.clone(), &passphrase, |_| true)
        .await
        .map_err(|error| format!("Failed to export room keys: {error}"))?;

    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn import_room_keys(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
    request: RoomKeyFileRequest,
) -> Result<Option<RoomKeyImportSummary>, String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };
    let passphrase = normalized_passphrase(&request.passphrase)?;
    let Some(path) = room_key_import_path(&app)? else {
        return Ok(None);
    };

    let result = account
        .client
        .encryption()
        .import_room_keys(path, &passphrase)
        .await
        .map_err(|error| format!("Failed to import room keys: {error}"))?;

    Ok(Some(RoomKeyImportSummary {
        imported_count: result.imported_count,
        total_count: result.total_count,
    }))
}

#[tauri::command]
pub async fn reset_crypto_identity(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
) -> Result<CryptoIdentityResetOutcome, String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };

    let Some(handle) = account
        .client
        .encryption()
        .recovery()
        .reset_identity()
        .await
        .map_err(|error| format!("Failed to reset crypto identity: {error}"))?
    else {
        return Ok(CryptoIdentityResetOutcome::Completed);
    };

    match handle.auth_type() {
        CrossSigningResetAuthType::Uiaa(_) => Ok(CryptoIdentityResetOutcome::UiaaRequired),
        CrossSigningResetAuthType::OAuth(info) => Ok(CryptoIdentityResetOutcome::OAuthRequired {
            approval_url: info.approval_url.to_string(),
        }),
    }
}

#[tauri::command]
pub async fn set_verified_devices_only(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
    shell_manager: tauri::State<'_, ShellManager>,
    enabled: bool,
) -> Result<(), String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };
    let mut preferences = AccountManager::load_encryption_preferences(&account.client).await?;
    if preferences.verified_devices_only == enabled {
        return Ok(());
    }

    preferences.verified_devices_only = enabled;
    AccountManager::persist_encryption_preferences(&account.client, &preferences).await?;
    shell_manager.stop_account(&account.account_key).await;
    account_manager.rebuild_active_client(&app).await?;
    shell_manager
        .ensure_active_account_sync(&app, &account_manager)
        .await
}

fn normalized_passphrase(passphrase: &str) -> Result<String, String> {
    let passphrase = passphrase.trim();
    if passphrase.is_empty() {
        return Err(String::from("Passphrase must not be empty"));
    }

    Ok(passphrase.to_owned())
}

fn room_key_export_path(app: &AppHandle) -> Result<Option<std::path::PathBuf>, String> {
    let selected = app
        .dialog()
        .file()
        .add_filter("Encrypted Matrix room keys", &["txt", "keys"])
        .blocking_save_file();

    selected.map(file_path_into_path).transpose()
}

fn room_key_import_path(app: &AppHandle) -> Result<Option<std::path::PathBuf>, String> {
    let selected = app
        .dialog()
        .file()
        .add_filter("Encrypted Matrix room keys", &["txt", "keys"])
        .blocking_pick_file();

    selected.map(file_path_into_path).transpose()
}

fn file_path_into_path(file_path: FilePath) -> Result<std::path::PathBuf, String> {
    file_path
        .into_path()
        .map_err(|path| format!("Selected file is not a local filesystem path: {path}"))
}

async fn enable_recovery_with_clean_backup(client: &Client) -> Result<String, String> {
    match client
        .encryption()
        .recovery()
        .enable()
        .wait_for_backups_to_upload()
        .await
    {
        Ok(recovery_key) => Ok(recovery_key),
        Err(error) if is_backup_already_exists_error(&error.to_string()) => {
            client
                .encryption()
                .backups()
                .disable_and_delete()
                .await
                .map_err(|delete_error| {
                    format!("Failed to remove the existing server key backup: {delete_error}")
                })?;
            client
                .encryption()
                .recovery()
                .enable()
                .wait_for_backups_to_upload()
                .await
                .map_err(|enable_error| format!("Failed to create recovery: {enable_error}"))
        }
        Err(error) => Err(format!("Failed to create recovery: {error}")),
    }
}

fn is_backup_already_exists_error(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("backup") && message.contains("exists")
}

fn is_backup_not_enabled_error(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("backup") && message.contains("not enabled")
}

async fn mark_recovery_account_data_disabled(client: &Client) -> Result<(), String> {
    client
        .account()
        .set_account_data_raw(
            GlobalAccountDataEventType::from(SECRET_STORAGE_DEFAULT_KEY_EVENT_TYPE),
            Raw::new(&json!({}))
                .map_err(|error| format!("Failed to serialize disabled recovery state: {error}"))?
                .cast_unchecked(),
        )
        .await
        .map_err(|error| format!("Failed to mark recovery as disabled: {error}"))?;
    client
        .account()
        .set_account_data_raw(
            GlobalAccountDataEventType::from(BACKUP_DISABLED_EVENT_TYPE),
            Raw::new(&json!({ "disabled": true }))
                .map_err(|error| format!("Failed to serialize disabled backup marker: {error}"))?
                .cast_unchecked(),
        )
        .await
        .map_err(|error| format!("Failed to mark server key backup as disabled: {error}"))?;

    Ok(())
}

fn recover_error_message(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    if message.contains("missing field `key`") {
        String::from(
            "Recovery is not configured correctly on this account. Create a new recovery key before recovering secrets.",
        )
    } else {
        format!("Failed to recover encryption secrets: {message}")
    }
}

fn backup_state_label(server_backup_enabled: bool, server_backup_exists: Option<bool>) -> String {
    if server_backup_enabled || server_backup_exists.unwrap_or_default() {
        String::from("Enabled")
    } else {
        String::from("Unknown")
    }
}
