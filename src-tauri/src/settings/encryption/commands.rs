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
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, FilePath};
use tauri_plugin_fs::{FsExt, OpenOptions};

use crate::{
    account::{AccountClientSnapshot, AccountManager},
    settings::account as account_settings,
    shell::ShellManager,
};

pub use super::types::{
    CryptoIdentityResetOutcome, EncryptionOverview, GeneratedRecoveryKey, RecoveryKeyRequest,
    RoomKeyFileRequest, RoomKeyImportSummary,
};

// Matrix has no account-data DELETE endpoint, so disabling recovery is represented by an empty default-key event.
const SECRET_STORAGE_DEFAULT_KEY_EVENT_TYPE: &str = "m.secret_storage.default_key";
// Matrix Rust SDK uses this custom marker to prevent automatic backup re-creation after recovery deletion.
const BACKUP_DISABLED_EVENT_TYPE: &str = "m.org.matrix.custom.backup_disabled";
// Default export name shown by mobile document pickers when creating encrypted room-key files.
const ROOM_KEY_EXPORT_FILE_NAME: &str = "hyperion-room-keys.txt";
// App-private staging folder for Matrix SDK room-key import/export, because the SDK requires local paths.
const ROOM_KEY_TRANSFER_DIRECTORY_NAME: &str = "room-key-transfer";
pub const ENCRYPTION_OVERVIEW_UPDATED_EVENT: &str = "hyperion://encryption-overview-updated";

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
            last_refreshed_at_unix_ms: None,
        });
    };

    let cached_overview = account_settings::read_encryption_overview(&account.store_dir)?;
    schedule_encryption_overview_refresh(app, account.clone());
    if let Some(overview) = cached_overview {
        return Ok(overview);
    }

    let preferences =
        AccountManager::load_encryption_preferences_for_store(&account.client, &account.store_dir)
            .await?;

    Ok(EncryptionOverview {
        has_active_account: true,
        account_key: Some(account.account_key),
        user_id: account.client.user_id().map(ToString::to_string),
        device_id: account.client.device_id().map(ToString::to_string),
        ed25519_key: None,
        curve25519_key: None,
        recovery_state: None,
        backup_state: None,
        server_key_storage_opted_out: preferences.server_key_storage_opted_out,
        verified_devices_only: preferences.verified_devices_only,
        last_refreshed_at_unix_ms: None,
    })
}

async fn refreshed_encryption_overview(
    account: &AccountClientSnapshot,
) -> Result<EncryptionOverview, String> {
    let preferences =
        AccountManager::load_encryption_preferences_for_store(&account.client, &account.store_dir)
            .await?;
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
    let recovery_state = recovery_state_label(
        secret_storage_enabled,
        cross_signing_complete,
        backup_available,
    );

    Ok(EncryptionOverview {
        has_active_account: true,
        account_key: Some(account.account_key.clone()),
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
        last_refreshed_at_unix_ms: Some(now_unix_ms()),
    })
}

fn schedule_encryption_overview_refresh(app: AppHandle, account: AccountClientSnapshot) {
    tauri::async_runtime::spawn(async move {
        let overview = match refreshed_encryption_overview(&account).await {
            Ok(overview) => overview,
            Err(error) => {
                eprintln!("Failed to refresh encryption overview in background: {error}");
                return;
            }
        };

        if let Err(error) =
            account_settings::write_encryption_overview(&account.store_dir, &overview)
        {
            eprintln!("Failed to persist refreshed encryption overview: {error}");
        }

        if let Err(error) = app.emit(ENCRYPTION_OVERVIEW_UPDATED_EVENT, overview) {
            eprintln!("Failed to emit encryption overview update: {error}");
        }
    });
}

#[tauri::command]
pub async fn enable_server_key_storage(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
) -> Result<(), String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };

    let mut preferences =
        AccountManager::load_encryption_preferences_for_store(&account.client, &account.store_dir)
            .await?;
    let encryption = account.client.encryption();
    let backups = encryption.backups();
    backups
        .create()
        .await
        .map_err(|error| format!("Failed to enable server-side key backup: {error}"))?;
    preferences.server_key_storage_opted_out = false;
    AccountManager::persist_encryption_preferences_for_store(&account.store_dir, &preferences)
}

#[tauri::command]
pub async fn disable_server_key_storage(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
) -> Result<(), String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };

    let mut preferences =
        AccountManager::load_encryption_preferences_for_store(&account.client, &account.store_dir)
            .await?;
    let encryption = account.client.encryption();
    let backups = encryption.backups();
    backups
        .disable_and_delete()
        .await
        .map_err(|error| format!("Failed to disable server-side key backup: {error}"))?;
    preferences.server_key_storage_opted_out = true;
    AccountManager::persist_encryption_preferences_for_store(&account.store_dir, &preferences)
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

    let encryption = account.client.encryption();
    let recovery = encryption.recovery();
    let recovery_key = recovery
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

    let encryption = account.client.encryption();
    let recovery = encryption.recovery();
    let recovery_result = recovery.disable().await;
    let backups = encryption.backups();
    backups
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
    let encryption = account.client.encryption();
    let secret_storage = encryption.secret_storage();
    let recovery_is_configured = secret_storage
        .is_enabled()
        .await
        .map_err(|error| format!("Failed to read recovery state: {error}"))?;
    if !recovery_is_configured {
        return Err(String::from(
            "Recovery is disabled for this account. Create a new recovery key before recovering secrets.",
        ));
    }

    let recovery = encryption.recovery();
    recovery
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
    let Some(destination) = room_key_export_destination(&app)? else {
        return Ok(None);
    };
    let export_path = room_key_export_path(&app, &destination)?;

    let encryption = account.client.encryption();
    encryption
        .export_room_keys(export_path.clone(), &passphrase, |_room_key| true)
        .await
        .map_err(|error| format!("Failed to export room keys: {error}"))?;
    if let RoomKeySelectedFile::DocumentUri(destination_uri) = &destination {
        copy_local_file_to_document_uri(&app, &export_path, destination_uri.clone())?;
        remove_transfer_file(&export_path)?;
    }

    Ok(Some(destination.to_display_string()))
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
    let Some(source) = room_key_import_source(&app)? else {
        return Ok(None);
    };
    let import_path = room_key_import_path(&app, &source)?;

    let encryption = account.client.encryption();
    let result = encryption
        .import_room_keys(import_path.clone(), &passphrase)
        .await
        .map_err(|error| format!("Failed to import room keys: {error}"))?;
    if matches!(source, RoomKeySelectedFile::DocumentUri(_source_uri)) {
        remove_transfer_file(&import_path)?;
    }

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

    let encryption = account.client.encryption();
    let recovery = encryption.recovery();
    let Some(handle) = recovery
        .reset_identity()
        .await
        .map_err(|error| format!("Failed to reset crypto identity: {error}"))?
    else {
        return Ok(CryptoIdentityResetOutcome::Completed);
    };

    match handle.auth_type() {
        CrossSigningResetAuthType::Uiaa(_uiaa_auth_info) => {
            Ok(CryptoIdentityResetOutcome::UiaaRequired)
        }
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
    let mut preferences =
        AccountManager::load_encryption_preferences_for_store(&account.client, &account.store_dir)
            .await?;
    if preferences.verified_devices_only == enabled {
        return Ok(());
    }

    preferences.verified_devices_only = enabled;
    AccountManager::persist_encryption_preferences_for_store(&account.store_dir, &preferences)?;
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

fn recovery_state_label(
    secret_storage_enabled: bool,
    cross_signing_complete: bool,
    backup_available: bool,
) -> String {
    if !secret_storage_enabled {
        return String::from("Disabled");
    }

    if cross_signing_complete && backup_available {
        return String::from("Enabled");
    }

    String::from("Incomplete")
}

enum RoomKeySelectedFile {
    LocalPath(PathBuf),
    DocumentUri(FilePath),
}

impl RoomKeySelectedFile {
    fn to_display_string(&self) -> String {
        match self {
            Self::LocalPath(path) => path.to_string_lossy().into_owned(),
            Self::DocumentUri(file_path) => file_path.to_string(),
        }
    }
}

fn room_key_export_destination(app: &AppHandle) -> Result<Option<RoomKeySelectedFile>, String> {
    let selected = app
        .dialog()
        .file()
        .add_filter("Encrypted Matrix room keys", &["txt", "keys"])
        .set_file_name(ROOM_KEY_EXPORT_FILE_NAME)
        .blocking_save_file();

    selected.map(classify_dialog_file).transpose()
}

fn room_key_import_source(app: &AppHandle) -> Result<Option<RoomKeySelectedFile>, String> {
    let selected = app
        .dialog()
        .file()
        .add_filter("Encrypted Matrix room keys", &["txt", "keys"])
        .blocking_pick_file();

    selected.map(classify_dialog_file).transpose()
}

fn classify_dialog_file(file_path: FilePath) -> Result<RoomKeySelectedFile, String> {
    match file_path {
        FilePath::Path(path) => Ok(RoomKeySelectedFile::LocalPath(path)),
        FilePath::Url(url) if url.scheme() == "file" => {
            let path = url
                .to_file_path()
                .map_err(|()| format!("Selected file URL is not a valid path: {url}"))?;
            Ok(RoomKeySelectedFile::LocalPath(path))
        }
        FilePath::Url(url) => Ok(RoomKeySelectedFile::DocumentUri(FilePath::Url(url))),
    }
}

fn room_key_export_path(
    app: &AppHandle,
    destination: &RoomKeySelectedFile,
) -> Result<PathBuf, String> {
    match destination {
        RoomKeySelectedFile::LocalPath(path) => Ok(path.clone()),
        RoomKeySelectedFile::DocumentUri(_destination_uri) => room_key_transfer_path(app, "export"),
    }
}

fn room_key_import_path(app: &AppHandle, source: &RoomKeySelectedFile) -> Result<PathBuf, String> {
    match source {
        RoomKeySelectedFile::LocalPath(path) => Ok(path.clone()),
        RoomKeySelectedFile::DocumentUri(source_uri) => {
            let import_path = room_key_transfer_path(app, "import")?;
            copy_document_uri_to_local_file(app, source_uri.clone(), &import_path)?;
            Ok(import_path)
        }
    }
}

fn room_key_transfer_path(app: &AppHandle, operation_name: &str) -> Result<PathBuf, String> {
    let transfer_directory = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("Failed to resolve app cache directory: {error}"))?
        .join(ROOM_KEY_TRANSFER_DIRECTORY_NAME);
    fs::create_dir_all(&transfer_directory)
        .map_err(|error| format!("Failed to prepare room-key transfer directory: {error}"))?;

    Ok(transfer_directory.join(format!("{operation_name}-{}.keys", rand::random::<u64>())))
}

fn copy_local_file_to_document_uri(
    app: &AppHandle,
    source_path: &Path,
    destination_uri: FilePath,
) -> Result<(), String> {
    let mut source = fs::File::open(source_path)
        .map_err(|error| format!("Failed to open exported room keys: {error}"))?;
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    let mut destination = app
        .fs()
        .open(destination_uri, options)
        .map_err(|error| format!("Failed to open selected export destination: {error}"))?;
    std::io::copy(&mut source, &mut destination)
        .map_err(|error| format!("Failed to copy room keys to selected destination: {error}"))?;
    destination
        .flush()
        .map_err(|error| format!("Failed to flush exported room keys: {error}"))
}

fn copy_document_uri_to_local_file(
    app: &AppHandle,
    source_uri: FilePath,
    destination_path: &Path,
) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.read(true);
    let mut source = app
        .fs()
        .open(source_uri, options)
        .map_err(|error| format!("Failed to open selected import file: {error}"))?;
    let mut destination = fs::File::create(destination_path)
        .map_err(|error| format!("Failed to prepare room-key import file: {error}"))?;
    std::io::copy(&mut source, &mut destination)
        .map_err(|error| format!("Failed to copy selected import file: {error}"))?;
    destination
        .flush()
        .map_err(|error| format!("Failed to flush room-key import file: {error}"))
}

fn remove_transfer_file(path: &Path) -> Result<(), String> {
    fs::remove_file(path)
        .map_err(|error| format!("Failed to remove temporary room-key transfer file: {error}"))
}

async fn enable_recovery_with_clean_backup(client: &Client) -> Result<String, String> {
    let encryption = client.encryption();
    let recovery = encryption.recovery();
    let enable_recovery = recovery.enable().wait_for_backups_to_upload();

    match enable_recovery.await {
        Ok(recovery_key) => Ok(recovery_key),
        Err(error) if is_backup_already_exists_error(&error.to_string()) => {
            let backups = encryption.backups();
            backups.disable_and_delete().await.map_err(|delete_error| {
                format!("Failed to remove the existing server key backup: {delete_error}")
            })?;
            let recovery = encryption.recovery();
            let enable_recovery = recovery.enable().wait_for_backups_to_upload();
            enable_recovery
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
    let account = client.account();
    let default_key_event_type =
        GlobalAccountDataEventType::from(SECRET_STORAGE_DEFAULT_KEY_EVENT_TYPE);
    let default_key_content = Raw::new(&json!({}))
        .map_err(|error| format!("Failed to serialize disabled recovery state: {error}"))?
        .cast_unchecked();
    account
        .set_account_data_raw(default_key_event_type, default_key_content)
        .await
        .map_err(|error| format!("Failed to mark recovery as disabled: {error}"))?;

    let backup_disabled_event_type = GlobalAccountDataEventType::from(BACKUP_DISABLED_EVENT_TYPE);
    let backup_disabled_content = Raw::new(&json!({ "disabled": true }))
        .map_err(|error| format!("Failed to serialize disabled backup marker: {error}"))?
        .cast_unchecked();
    account
        .set_account_data_raw(backup_disabled_event_type, backup_disabled_content)
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

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}
