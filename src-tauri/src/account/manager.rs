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

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use matrix_sdk::{Client, SqliteStoreConfig, search_index::SearchIndexStoreKind};
use matrix_sdk_crypto::CollectStrategy;
use tauri::AppHandle;
use tauri::async_runtime::Mutex as AsyncMutex;

use super::secure_storage;
use super::types::{
    AccountClientSnapshot, AccountSummary, ActiveAccount, EncryptionPreferences, LoginRequest,
};

#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub enum AccountAuthorizationMode {
    #[default]
    Online,
    ReauthenticationRequired,
}

mod registration;
mod storage;

const SESSION_CUSTOM_VALUE_KEY: &[u8] = b"hyperion.account.session.v1";
const ACCOUNT_METADATA_CUSTOM_VALUE_KEY: &[u8] = b"hyperion.account.metadata.v1";
// Account-scoped encryption UI preferences live beside the Matrix session in the encrypted store.
pub const ENCRYPTION_PREFERENCES_CUSTOM_VALUE_KEY: &[u8] =
    b"hyperion.account.encryption-preferences.v1";
const STORE_KEY_PREFIX: &str = "matrix-store-key";
const STORE_KEY_LENGTH: usize = 32;
// Fresh post-sign-out stores need a short random suffix when Windows still
// holds the old SQLite directory, preventing reuse with a newly generated key.
const REPLACEMENT_STORE_ID_RANDOM_BYTES: usize = 8;

pub(super) struct ManagedAccount {
    // Each logged-in account owns its own Matrix client instance.
    client: Client,
    user_id: String,
    homeserver_url: String,
    store_dir: PathBuf,
}

pub(super) struct AccountStorageLocation {
    pub(super) store_id: String,
    pub(super) store_dir: PathBuf,
    pub(super) cache_dir: PathBuf,
    pub(super) homeserver_url: String,
}

#[derive(Clone, Default)]
pub struct AccountManager {
    // The SDK does not manage multiple logged-in accounts for us, so we keep
    // one client per account and switch which one the UI treats as active.
    accounts: Arc<RwLock<HashMap<String, ManagedAccount>>>,
    active_account_key: Arc<RwLock<Option<String>>>,
    restore_lock: Arc<AsyncMutex<()>>,
    restore_completed: Arc<RwLock<bool>>,
    authorization_modes: Arc<RwLock<HashMap<String, AccountAuthorizationMode>>>,
}

impl AccountManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn login(
        &self,
        app: &AppHandle,
        request: LoginRequest,
    ) -> Result<AccountSummary, String> {
        self.ensure_loaded(app).await?;

        // Multi-account support requires isolated stores. Every account gets
        // its own encrypted sqlite database directory under the app data folder.
        let storage = self.account_storage(app, &request.homeserver_url, &request.username)?;
        let store_key = Self::load_or_create_store_key(app, &storage.store_id)?;
        // A soft logout retains the encrypted store and its device identity.
        // Reusing that device ID is required by the Matrix soft-logout flow.
        let retained_device_id = self.device_id_for_store_dir(&storage.store_dir);
        let client = self
            .login_client_with_recovery(
                &storage,
                &store_key,
                &request,
                retained_device_id.as_deref(),
            )
            .await?;
        let matrix_auth = client.matrix_auth();
        let session = matrix_auth
            .session()
            .ok_or_else(|| String::from("Login succeeded, but session data is not available"))?;
        let user_id = session.meta.user_id.to_string();

        Self::persist_session(&client, &session).await?;
        let account = self.store_logged_in_account(
            user_id.clone(),
            user_id,
            Self::client_homeserver_url(&client),
            storage.store_dir,
            client,
        );
        self.set_authorization_mode(&account.account_key, AccountAuthorizationMode::Online);
        self.persist_account_store_metadata().await?;

        Ok(account)
    }

    pub async fn list_accounts(&self, app: &AppHandle) -> Result<Vec<AccountSummary>, String> {
        self.ensure_loaded(app).await?;

        let accounts = self
            .accounts
            .read()
            .expect("account manager accounts lock poisoned");
        let active_account_key = self
            .active_account_key
            .read()
            .expect("account manager active account lock poisoned");

        Ok(accounts
            .iter()
            .map(|(account_key, account)| AccountSummary {
                account_key: account_key.clone(),
                user_id: account.user_id.clone(),
                homeserver_url: account.homeserver_url.clone(),
                is_active: active_account_key.as_deref() == Some(account_key.as_str()),
            })
            .collect())
    }

    pub async fn switch_active_account(
        &self,
        app: &AppHandle,
        account_key: &str,
    ) -> Result<(), String> {
        self.ensure_loaded(app).await?;

        {
            let accounts = self
                .accounts
                .read()
                .expect("account manager accounts lock poisoned");
            if !accounts.contains_key(account_key) {
                return Err(format!("Unknown account key: {account_key}"));
            }
        }

        {
            let mut active_account_key = self
                .active_account_key
                .write()
                .expect("account manager active account lock poisoned");
            *active_account_key = Some(account_key.to_owned());
        }

        self.persist_account_store_metadata().await?;
        Ok(())
    }

    pub async fn active_account(&self, app: &AppHandle) -> Result<Option<AccountSummary>, String> {
        self.ensure_loaded(app).await?;

        let active_account_key = self
            .active_account_key
            .read()
            .expect("account manager active account lock poisoned");
        let Some(key) = active_account_key.clone() else {
            return Ok(None);
        };
        drop(active_account_key);

        let accounts = self
            .accounts
            .read()
            .expect("account manager accounts lock poisoned");
        let Some(account) = accounts.get(&key) else {
            return Ok(None);
        };
        let summary = AccountSummary {
            account_key: key,
            user_id: account.user_id.clone(),
            homeserver_url: account.homeserver_url.clone(),
            is_active: true,
        };
        drop(accounts);

        Ok(Some(summary))
    }

    pub async fn sign_out_active_account(
        &self,
        app: &AppHandle,
    ) -> Result<Option<AccountSummary>, String> {
        self.ensure_loaded(app).await?;

        let Some(active_account_snapshot) = self.active_account_snapshot() else {
            return Ok(None);
        };
        let store_dir = active_account_snapshot.2;

        let store_root_dir = store_dir
            .parent()
            .ok_or_else(|| String::from("Active account store path has no parent directory"))?;
        let store_dir_name = store_root_dir
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| String::from("Active account store directory has no valid store id"))?;
        let store_id = store_dir_name.to_owned();

        secure_storage::delete_secret(app, &Self::store_key_entry_id(&store_id))?;
        self.release_accounts_for_store_dir(&store_dir);
        self.persist_account_store_metadata().await?;

        drop(Self::remove_dir_with_retries(
            store_root_dir,
            "Failed to remove the signed-out account store directory",
        ));

        self.active_account(app).await
    }

    pub async fn validate_active_account(
        &self,
        app: &AppHandle,
    ) -> Result<Option<AccountSummary>, String> {
        self.ensure_loaded(app).await?;
        self.active_account(app).await
    }

    pub async fn deauthorize_account_store(
        &self,
        app: &AppHandle,
        store_dir: &Path,
    ) -> Result<Option<AccountSummary>, String> {
        self.ensure_loaded(app).await?;
        self.release_deauthorized_account_store(app, store_dir);
        self.persist_account_store_metadata().await?;
        self.active_account(app).await
    }

    fn release_deauthorized_account_store(&self, app: &AppHandle, store_dir: &Path) {
        let Some(store_root_dir) = store_dir.parent().map(Path::to_owned) else {
            #[cfg(debug_assertions)]
            tracing::warn!(
                target: "hyperion",
                event_name = "account.cleanup_skipped",
                component = "account.storage",
                operation = "cleanup_deauthorized_account",
                error_code = "account.storage_path_invalid",
                error_category = "storage",
                store_path = %store_dir.display(),
                "Failed to clean up a deauthorized account: store path has no parent ({})",
                store_dir.display()
            );
            #[cfg(not(debug_assertions))]
            tracing::warn!(
                target: "hyperion",
                event_name = "account.cleanup_skipped",
                component = "account.storage",
                operation = "cleanup_deauthorized_account",
                error_code = "account.storage_path_invalid",
                error_category = "storage",
                "Deauthorized account cleanup was skipped"
            );
            return;
        };
        let Some(store_id) = store_root_dir
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
        else {
            #[cfg(debug_assertions)]
            tracing::warn!(
                target: "hyperion",
                event_name = "account.cleanup_skipped",
                component = "account.storage",
                operation = "cleanup_deauthorized_account",
                error_code = "account.storage_id_missing",
                error_category = "storage",
                store_path = %store_root_dir.display(),
                "Failed to clean up a deauthorized account: account root has no store id ({})",
                store_root_dir.display()
            );
            #[cfg(not(debug_assertions))]
            tracing::warn!(
                target: "hyperion",
                event_name = "account.cleanup_skipped",
                component = "account.storage",
                operation = "cleanup_deauthorized_account",
                error_code = "account.storage_id_missing",
                error_category = "storage",
                "Deauthorized account cleanup was skipped"
            );
            return;
        };

        self.release_accounts_for_store_dir(store_dir);
        drop(secure_storage::delete_secret(
            app,
            &Self::store_key_entry_id(&store_id),
        ));

        if let Err(cleanup_error) = Self::remove_dir_with_retries(
            &store_root_dir,
            "Failed to remove deauthorized account store directory",
        ) {
            crate::utils::tracing::report_recoverable_error(
                "account.storage",
                "cleanup_deauthorized_account",
                "account.storage_cleanup_failed",
                "storage",
                &cleanup_error,
            );
        }
    }

    pub async fn optional_active_account(
        &self,
        app: &AppHandle,
    ) -> Result<Option<ActiveAccount>, String> {
        self.ensure_loaded(app).await?;
        Ok(self.active_account_client_loaded().map(ActiveAccount::new))
    }

    pub async fn require_active_account(&self, app: &AppHandle) -> Result<ActiveAccount, String> {
        self.optional_active_account(app)
            .await?
            .ok_or_else(|| String::from("No active account is available"))
    }

    pub fn network_actions_allowed(&self, account_key: &str) -> bool {
        self.authorization_modes
            .read()
            .expect("account authorization modes lock poisoned")
            .get(account_key)
            .copied()
            .unwrap_or_default()
            == AccountAuthorizationMode::Online
    }

    pub fn mark_reauthentication_required(&self, account_key: &str) {
        self.set_authorization_mode(
            account_key,
            AccountAuthorizationMode::ReauthenticationRequired,
        );
    }

    pub async fn persist_refreshed_session(
        &self,
        account: &AccountClientSnapshot,
    ) -> Result<(), String> {
        let session = account.client.matrix_auth().session().ok_or_else(|| {
            String::from("session_refresh_persistence_failed: refreshed session is unavailable")
        })?;
        Self::persist_session(&account.client, &session)
            .await
            .map_err(|error| format!("session_refresh_persistence_failed: {error}"))
    }

    fn set_authorization_mode(&self, account_key: &str, mode: AccountAuthorizationMode) {
        self.authorization_modes
            .write()
            .expect("account authorization modes lock poisoned")
            .insert(account_key.to_owned(), mode);
    }

    pub(crate) fn active_account_client_loaded(&self) -> Option<AccountClientSnapshot> {
        let active_account_key = self
            .active_account_key
            .read()
            .expect("account manager active account lock poisoned");
        let account_key = active_account_key.clone()?;
        drop(active_account_key);

        let accounts = self
            .accounts
            .read()
            .expect("account manager accounts lock poisoned");
        let account = accounts.get(&account_key)?;
        let snapshot = AccountClientSnapshot {
            account_key,
            homeserver_url: account.homeserver_url.clone(),
            client: account.client.clone(),
            store_dir: account.store_dir.clone(),
        };
        drop(accounts);

        Some(snapshot)
    }

    pub async fn rebuild_active_client(&self, app: &AppHandle) -> Result<bool, String> {
        self.ensure_loaded(app).await?;

        let Some((account_summary, current_client, store_dir)) = self.active_account_snapshot()
        else {
            return Ok(false);
        };

        let preferences =
            Self::load_encryption_preferences_for_store(&current_client, &store_dir).await?;
        let Some(session) = Self::load_session(&current_client).await? else {
            return Err(String::from("The active account session is not available"));
        };
        drop(current_client);

        let store_root_dir = store_dir
            .parent()
            .ok_or_else(|| String::from("Active account store path has no parent directory"))?;
        let store_dir_name = store_root_dir
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| String::from("Active account store directory has no valid store id"))?;
        let store_id = store_dir_name.to_owned();
        let store_key = Self::load_store_key(app, &store_id)?
            .ok_or_else(|| String::from("The active account store key is not available"))?;
        let cache_dir = store_root_dir.join("cache");
        let replacement_client = Self::build_client_with_preferences(
            &account_summary.homeserver_url,
            &store_dir,
            &cache_dir,
            &store_key,
            &preferences,
        )
        .await?;

        replacement_client
            .restore_session(session)
            .await
            .map_err(|error| format!("Failed to restore rebuilt Matrix client session: {error}"))?;

        {
            let mut accounts = self
                .accounts
                .write()
                .expect("account manager accounts lock poisoned");
            let account = accounts
                .get_mut(&account_summary.account_key)
                .ok_or_else(|| {
                    String::from(
                        "The active account disappeared while rebuilding its Matrix client",
                    )
                })?;
            account.client = replacement_client.clone();
            drop(accounts);
        }

        Ok(true)
    }

    async fn login_client_with_recovery(
        &self,
        storage: &AccountStorageLocation,
        store_key: &[u8; STORE_KEY_LENGTH],
        request: &LoginRequest,
        retained_device_id: Option<&str>,
    ) -> Result<Client, String> {
        match Self::build_and_login_client(
            &storage.store_dir,
            &storage.cache_dir,
            store_key,
            request,
            retained_device_id,
        )
        .await
        {
            Ok(client) => Ok(client),
            Err(error_message)
                if retained_device_id.is_none()
                    && Self::is_stale_crypto_store_error(&error_message) =>
            {
                // If the server issued this account a new device ID, the old crypto
                // store can no longer be reused. Reset just this account's local
                // store and retry the login once with a clean database.
                self.release_accounts_for_store_dir(&storage.store_dir);
                Self::reset_store_dir(&storage.store_dir)?;

                Self::build_and_login_client(
                    &storage.store_dir,
                    &storage.cache_dir,
                    store_key,
                    request,
                    None,
                )
                .await
                .map_err(|retry_error| {
                    format!(
                        "Login failed after resetting the stale local crypto store: \
                             {retry_error}"
                    )
                })
            }
            Err(error_message) => Err(error_message),
        }
    }

    fn release_accounts_for_store_dir(&self, store_dir: &Path) {
        let mut accounts = self
            .accounts
            .write()
            .expect("account manager accounts lock poisoned");

        let matching_account_keys: Vec<String> = accounts
            .iter()
            .filter(|account_entry| account_entry.1.store_dir == store_dir)
            .map(|account_entry| account_entry.0.clone())
            .collect();

        if matching_account_keys.is_empty() {
            return;
        }

        let mut active_account_key = self
            .active_account_key
            .write()
            .expect("account manager active account lock poisoned");

        let removed_active_account =
            active_account_was_removed(active_account_key.as_deref(), &matching_account_keys);

        for account_key in &matching_account_keys {
            accounts.remove(account_key);
        }

        if removed_active_account {
            *active_account_key = accounts.keys().next().cloned();
        }
    }

    fn device_id_for_store_dir(&self, store_dir: &Path) -> Option<String> {
        let accounts = self
            .accounts
            .read()
            .expect("account manager accounts lock poisoned");
        accounts
            .values()
            .find(|account| account.store_dir == store_dir)
            .and_then(|account| account.client.matrix_auth().session())
            .map(|session| session.meta.device_id.to_string())
    }

    fn active_account_snapshot(&self) -> Option<(AccountSummary, Client, PathBuf)> {
        let active_account_key = self
            .active_account_key
            .read()
            .expect("account manager active account lock poisoned");
        let key = active_account_key.clone()?;
        drop(active_account_key);

        let accounts = self
            .accounts
            .read()
            .expect("account manager accounts lock poisoned");
        let account = accounts.get(&key)?;
        let snapshot = (
            AccountSummary {
                account_key: key,
                user_id: account.user_id.clone(),
                homeserver_url: account.homeserver_url.clone(),
                is_active: true,
            },
            account.client.clone(),
            account.store_dir.clone(),
        );
        drop(accounts);

        Some(snapshot)
    }

    async fn build_and_login_client(
        store_dir: &Path,
        cache_dir: &Path,
        store_key: &[u8; STORE_KEY_LENGTH],
        request: &LoginRequest,
        retained_device_id: Option<&str>,
    ) -> Result<Client, String> {
        let client =
            Self::build_client(&request.homeserver_url, store_dir, cache_dir, store_key).await?;
        let mut login_builder = client
            .matrix_auth()
            .login_username(&request.username, &request.password)
            .request_refresh_token();

        if let Some(device_name) = &request.device_display_name {
            login_builder = login_builder.initial_device_display_name(device_name);
        }
        if let Some(device_id) = retained_device_id {
            login_builder = login_builder.device_id(device_id);
        }

        login_builder
            .send()
            .await
            .map_err(|err| format!("Login failed: {err}"))?;

        Ok(client)
    }

    pub(super) async fn build_client(
        homeserver_target: &str,
        store_dir: &Path,
        cache_dir: &Path,
        store_key: &[u8; STORE_KEY_LENGTH],
    ) -> Result<Client, String> {
        Self::build_client_with_preferences(
            homeserver_target,
            store_dir,
            cache_dir,
            store_key,
            &EncryptionPreferences::default(),
        )
        .await
    }

    async fn build_client_with_preferences(
        homeserver_target: &str,
        store_dir: &Path,
        cache_dir: &Path,
        store_key: &[u8; STORE_KEY_LENGTH],
        preferences: &EncryptionPreferences,
    ) -> Result<Client, String> {
        let store_config = SqliteStoreConfig::new(store_dir).key(Some(store_key));
        let room_key_recipient_strategy = room_key_recipient_strategy(preferences);

        let client_builder = Client::builder()
            .server_name_or_homeserver_url(homeserver_target)
            .sqlite_store_with_config_and_cache_path(store_config, Some(cache_dir))
            .with_room_key_recipient_strategy(room_key_recipient_strategy)
            .with_enable_share_history_on_invite(preferences.share_encrypted_history_on_invite)
            .handle_refresh_tokens()
            // Hyperion owns durable search indexing. Keeping the SDK's
            // experimental index in memory avoids path issues from raw room IDs
            // on Windows while the app-level index handles searchable metadata.
            .search_index_store(SearchIndexStoreKind::InMemory);

        let client = client_builder
            .build()
            .await
            .map_err(|err| format!("Failed to build Matrix client: {err}"))?;
        client
            .event_cache()
            .subscribe()
            .map_err(|err| format!("Failed to subscribe the Matrix event cache: {err}"))?;
        Ok(client)
    }

    pub(super) fn store_logged_in_account(
        &self,
        account_key: String,
        user_id: String,
        homeserver_url: String,
        store_dir: PathBuf,
        client: Client,
    ) -> AccountSummary {
        let mut accounts = self
            .accounts
            .write()
            .expect("account manager accounts lock poisoned");

        // Replacing an existing entry lets the same account log in or register
        // again without leaving a stale client instance behind.
        accounts.insert(
            account_key.clone(),
            ManagedAccount {
                client,
                user_id: user_id.clone(),
                homeserver_url: homeserver_url.clone(),
                store_dir,
            },
        );
        drop(accounts);

        let mut active_account = self
            .active_account_key
            .write()
            .expect("account manager active account lock poisoned");
        if active_account.is_none() {
            *active_account = Some(account_key.clone());
        }
        let is_active = active_account.as_deref() == Some(account_key.as_str());
        drop(active_account);

        AccountSummary {
            account_key,
            user_id,
            homeserver_url,
            is_active,
        }
    }

    fn is_stale_crypto_store_error(error_message: &str) -> bool {
        error_message.contains("account in the store doesn't match the account in the constructor")
    }

    pub(super) fn client_homeserver_url(client: &Client) -> String {
        let homeserver = client.homeserver().to_string();
        let homeserver = homeserver.trim_end_matches('/');

        homeserver.to_owned()
    }
}

fn room_key_recipient_strategy(preferences: &EncryptionPreferences) -> CollectStrategy {
    if preferences.verified_devices_only {
        return CollectStrategy::OnlyTrustedDevices;
    }

    CollectStrategy::AllDevices
}

fn active_account_was_removed(
    active_account_key: Option<&str>,
    removed_account_keys: &[String],
) -> bool {
    let Some(active_account_key) = active_account_key else {
        return false;
    };

    removed_account_keys
        .iter()
        .any(|account_key| account_key == active_account_key)
}
