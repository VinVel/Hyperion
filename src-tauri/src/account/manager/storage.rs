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
    fs,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use matrix_sdk::{Client, authentication::matrix::MatrixSession};
use rand::{TryRng, rngs::SysRng};
use tauri::{AppHandle, Manager};

use super::{
    AccountManager, AccountStorageLocation, ManagedAccount, REPLACEMENT_STORE_ID_RANDOM_BYTES,
    STORE_KEY_LENGTH, STORE_KEY_PREFIX,
};
use crate::account::{
    secure_storage,
    types::{EncryptionPreferences, StoredAccountMetadata},
};
use crate::settings::account as account_settings;

struct AccountRestoreState {
    retry_needed: bool,
}

enum DiscoveredAccountRestore {
    Restored {
        account_key: String,
        account: ManagedAccount,
        is_active: bool,
    },
    SkippedMissingKey,
    RetryNeeded,
}

impl AccountManager {
    pub(super) fn account_storage(
        &self,
        app: &AppHandle,
        homeserver_url: &str,
        account_hint: &str,
    ) -> Result<AccountStorageLocation, String> {
        let storage_parent_dir = Self::accounts_root_dir(app)?;

        // Homeserver + account hint makes the on-disk store stable before we
        // know the final Matrix user id returned by the server. This keeps each
        // account in its own sqlite database, which is required for
        // multi-account support with the Matrix Rust SDK.
        let mut store_id = Self::account_store_id(homeserver_url, account_hint);
        let mut store_root_dir = storage_parent_dir.join(&store_id);

        if store_root_dir.exists() && !self.store_dir_is_managed(&store_root_dir.join("store")) {
            drop(secure_storage::delete_secret(
                app,
                &Self::store_key_entry_id(&store_id),
            ));
            store_id = Self::replacement_account_store_id(&store_id)?;
            store_root_dir = storage_parent_dir.join(&store_id);
        }

        let store_dir = store_root_dir.join("store");
        let cache_dir = store_root_dir.join("cache");

        fs::create_dir_all(&store_dir)
            .map_err(|err| format!("Failed to create account store directory: {err}"))?;
        fs::create_dir_all(&cache_dir)
            .map_err(|err| format!("Failed to create account cache directory: {err}"))?;

        Ok(AccountStorageLocation {
            store_id,
            store_dir,
            cache_dir,
            homeserver_url: homeserver_url.to_owned(),
        })
    }

    fn store_dir_is_managed(&self, store_dir: &Path) -> bool {
        let accounts = self
            .accounts
            .read()
            .expect("account manager accounts lock poisoned");

        accounts
            .values()
            .any(|account| account.store_dir == store_dir)
    }

    pub(super) fn remove_dir_with_retries(
        path: &Path,
        failure_context: &str,
    ) -> Result<(), String> {
        const RESET_RETRY_ATTEMPTS: usize = 50;
        const RESET_RETRY_DELAY: Duration = Duration::from_millis(100);

        for attempt in 0..RESET_RETRY_ATTEMPTS {
            if !path.exists() {
                return Ok(());
            }

            match fs::remove_dir_all(path) {
                Ok(()) => return Ok(()),
                Err(err)
                    if err.raw_os_error() == Some(32) && attempt + 1 < RESET_RETRY_ATTEMPTS =>
                {
                    thread::sleep(RESET_RETRY_DELAY);
                }
                Err(err) => return Err(format!("{failure_context}: {err}")),
            }
        }

        Err(format!("{failure_context} after repeated retries"))
    }

    pub(super) fn reset_store_dir(store_dir: &Path) -> Result<(), String> {
        Self::remove_dir_with_retries(store_dir, "Failed to reset stale account store directory")?;
        fs::create_dir_all(store_dir)
            .map_err(|err| format!("Failed to recreate account store directory: {err}"))
    }

    pub(crate) async fn ensure_loaded(&self, app: &AppHandle) -> Result<(), String> {
        if self.restore_is_completed() {
            return Ok(());
        }

        let _guard = self.restore_lock.lock().await;
        if self.restore_is_completed() {
            return Ok(());
        }

        let restore_state = self.restore_accounts_state(app).await?;
        {
            let mut restore_completed = self
                .restore_completed
                .write()
                .expect("account manager restore flag lock poisoned");
            *restore_completed = !restore_state.retry_needed;
        }

        Ok(())
    }

    fn restore_is_completed(&self) -> bool {
        let restore_completed = self
            .restore_completed
            .read()
            .expect("account manager restore flag lock poisoned");
        *restore_completed
    }

    async fn restore_accounts_state(&self, app: &AppHandle) -> Result<AccountRestoreState, String> {
        let discovered_stores = Self::discover_account_stores(app)?;
        let mut restored_accounts = HashMap::new();
        let mut active_account_key = None;
        let mut retry_needed = false;

        for storage in discovered_stores {
            match Self::restore_discovered_account_store(app, storage).await? {
                DiscoveredAccountRestore::Restored {
                    account_key,
                    account,
                    is_active,
                } => {
                    if is_active && active_account_key.is_none() {
                        active_account_key = Some(account_key.clone());
                    }

                    restored_accounts.insert(account_key, account);
                }
                DiscoveredAccountRestore::SkippedMissingKey => {}
                DiscoveredAccountRestore::RetryNeeded => {
                    retry_needed = true;
                }
            }
        }

        if active_account_key.is_none() {
            active_account_key = restored_accounts.keys().next().cloned();
        }

        let has_restored_accounts = !restored_accounts.is_empty();

        {
            let mut accounts = self
                .accounts
                .write()
                .expect("account manager accounts lock poisoned");
            *accounts = restored_accounts;
        }

        {
            let mut active_account = self
                .active_account_key
                .write()
                .expect("account manager active account lock poisoned");
            *active_account = active_account_key;
        }

        if has_restored_accounts {
            self.persist_account_store_metadata().await?;
        }

        Ok(AccountRestoreState { retry_needed })
    }

    async fn restore_discovered_account_store(
        app: &AppHandle,
        storage: AccountStorageLocation,
    ) -> Result<DiscoveredAccountRestore, String> {
        let store_key = match Self::load_store_key(app, &storage.store_id) {
            Ok(Some(store_key)) => store_key,
            Ok(None) => {
                eprintln!(
                    "Skipping persisted account store {} because its secure encryption key is missing",
                    storage.store_id
                );
                return Ok(DiscoveredAccountRestore::SkippedMissingKey);
            }
            Err(error) => {
                eprintln!(
                    "Deferring persisted account store {} because its secure encryption key could not be read: {error}",
                    storage.store_id
                );
                return Ok(DiscoveredAccountRestore::RetryNeeded);
            }
        };

        let Some((client, metadata, session)) =
            Self::restore_account_client_and_metadata(app, &storage, &store_key).await?
        else {
            return Ok(DiscoveredAccountRestore::RetryNeeded);
        };

        let preferences =
            Self::load_encryption_preferences_for_store(&client, &storage.store_dir).await?;
        let client = Self::rebuild_restored_client_if_needed(
            client,
            &storage,
            &store_key,
            session,
            &preferences,
        )
        .await?;

        let account_key = metadata.user_id.clone();
        let is_active = metadata.is_active;
        let account = ManagedAccount {
            client,
            user_id: metadata.user_id,
            homeserver_url: metadata.homeserver_url,
            store_dir: storage.store_dir,
        };

        Ok(DiscoveredAccountRestore::Restored {
            account_key,
            account,
            is_active,
        })
    }

    async fn restore_account_client_and_metadata(
        app: &AppHandle,
        storage: &AccountStorageLocation,
        store_key: &[u8; STORE_KEY_LENGTH],
    ) -> Result<Option<(Client, StoredAccountMetadata, MatrixSession)>, String> {
        let client = match Self::build_client(
            &storage.homeserver_url,
            &storage.store_dir,
            &storage.cache_dir,
            store_key,
        )
        .await
        {
            Ok(client) => client,
            Err(error) => {
                eprintln!(
                    "Deferring persisted account store {} because the Matrix client could not be rebuilt yet: {error}",
                    storage.store_id
                );
                return Ok(None);
            }
        };

        let Some(metadata) = Self::load_account_metadata(&client).await? else {
            drop(client);
            Self::prune_incomplete_store(app, storage, "its metadata is missing");
            return Ok(None);
        };

        let Some(session) = Self::load_session(&client).await? else {
            drop(client);
            Self::prune_incomplete_store(app, storage, "its session is missing");
            return Ok(None);
        };

        if let Err(error) = client.restore_session(session.clone()).await {
            eprintln!(
                "Deferring persisted account {} because the Matrix session could not be restored yet: {error}",
                metadata.user_id
            );
            return Ok(None);
        }

        Ok(Some((client, metadata, session)))
    }

    async fn rebuild_restored_client_if_needed(
        client: Client,
        storage: &AccountStorageLocation,
        store_key: &[u8; STORE_KEY_LENGTH],
        session: MatrixSession,
        preferences: &EncryptionPreferences,
    ) -> Result<Client, String> {
        let client_uses_default_preferences =
            !preferences.verified_devices_only && !preferences.share_encrypted_history_on_invite;
        if client_uses_default_preferences {
            return Ok(client);
        }

        drop(client);
        let rebuilt_client = Self::build_client_with_preferences(
            &storage.homeserver_url,
            &storage.store_dir,
            &storage.cache_dir,
            store_key,
            preferences,
        )
        .await?;
        rebuilt_client
            .restore_session(session)
            .await
            .map_err(|error| {
                format!("Failed to restore trusted-device-only Matrix client: {error}")
            })?;

        Ok(rebuilt_client)
    }

    fn prune_incomplete_store(app: &AppHandle, storage: &AccountStorageLocation, reason: &str) {
        // A discovered store with no persisted metadata or no session cannot be
        // restored into a valid account. Removing it avoids repeated startup
        // warnings from abandoned login/registration attempts.
        eprintln!(
            "Removing incomplete persisted account store {} because {reason}",
            storage.store_id
        );

        if let Err(error) = Self::remove_dir_with_retries(
            &storage.store_dir,
            "Failed to remove incomplete account store directory",
        ) {
            eprintln!("{error}");
            return;
        }

        let store_root_dir = storage
            .store_dir
            .parent()
            .ok_or_else(|| String::from("Incomplete account store path has no parent directory"));

        let store_root_dir = match store_root_dir {
            Ok(store_root_dir) => store_root_dir,
            Err(error) => {
                eprintln!("{error}");
                return;
            }
        };

        if let Err(error) = Self::remove_dir_with_retries(
            store_root_dir,
            "Failed to remove incomplete account root directory",
        ) {
            eprintln!("{error}");
        }

        drop(secure_storage::delete_secret(
            app,
            &Self::store_key_entry_id(&storage.store_id),
        ));
    }

    pub(super) async fn persist_account_store_metadata(&self) -> Result<(), String> {
        let active_account_key = self
            .active_account_key
            .read()
            .expect("account manager active account lock poisoned")
            .clone();

        let snapshots: Vec<(Client, StoredAccountMetadata)> = {
            let accounts = self
                .accounts
                .read()
                .expect("account manager accounts lock poisoned");

            accounts
                .iter()
                .map(|(account_key, account)| {
                    (
                        account.client.clone(),
                        StoredAccountMetadata {
                            user_id: account.user_id.clone(),
                            homeserver_url: account.homeserver_url.clone(),
                            is_active: active_account_key.as_deref() == Some(account_key.as_str()),
                        },
                    )
                })
                .collect()
        };

        for (client, metadata) in snapshots {
            Self::persist_account_metadata(&client, &metadata).await?;
        }

        Ok(())
    }

    pub(super) async fn persist_session(
        client: &Client,
        session: &MatrixSession,
    ) -> Result<(), String> {
        let value = serde_json::to_vec(session)
            .map_err(|error| format!("Failed to serialize persisted Matrix session: {error}"))?;

        let state_store = client.state_store();
        state_store
            .set_custom_value_no_read(super::SESSION_CUSTOM_VALUE_KEY, value)
            .await
            .map_err(|error| {
                format!("Failed to persist Matrix session in the encrypted store: {error}")
            })
    }

    pub(super) async fn load_session(client: &Client) -> Result<Option<MatrixSession>, String> {
        let state_store = client.state_store();
        let Some(value) = state_store
            .get_custom_value(super::SESSION_CUSTOM_VALUE_KEY)
            .await
            .map_err(|error| format!("Failed to load persisted Matrix session: {error}"))?
        else {
            return Ok(None);
        };

        serde_json::from_slice(&value)
            .map(Some)
            .map_err(|error| format!("Failed to parse persisted Matrix session: {error}"))
    }

    async fn persist_account_metadata(
        client: &Client,
        metadata: &StoredAccountMetadata,
    ) -> Result<(), String> {
        let value = serde_json::to_vec(metadata)
            .map_err(|error| format!("Failed to serialize persisted account metadata: {error}"))?;

        let state_store = client.state_store();
        state_store
            .set_custom_value_no_read(super::ACCOUNT_METADATA_CUSTOM_VALUE_KEY, value)
            .await
            .map_err(|error| {
                format!("Failed to persist account metadata in the encrypted store: {error}")
            })
    }

    async fn load_account_metadata(
        client: &Client,
    ) -> Result<Option<StoredAccountMetadata>, String> {
        let state_store = client.state_store();
        let Some(value) = state_store
            .get_custom_value(super::ACCOUNT_METADATA_CUSTOM_VALUE_KEY)
            .await
            .map_err(|error| format!("Failed to load persisted account metadata: {error}"))?
        else {
            return Ok(None);
        };

        serde_json::from_slice(&value)
            .map(Some)
            .map_err(|error| format!("Failed to parse persisted account metadata: {error}"))
    }

    pub async fn load_encryption_preferences_for_store(
        client: &Client,
        store_dir: &Path,
    ) -> Result<EncryptionPreferences, String> {
        let local_preferences = account_settings::read_encryption_preferences(store_dir)?;
        if local_preferences != EncryptionPreferences::default() {
            return Ok(local_preferences);
        }

        let legacy_preferences = Self::load_legacy_encryption_preferences(client).await?;
        if legacy_preferences != EncryptionPreferences::default() {
            account_settings::write_encryption_preferences(store_dir, &legacy_preferences)?;
        }

        Ok(legacy_preferences)
    }

    pub fn persist_encryption_preferences_for_store(
        store_dir: &Path,
        preferences: &EncryptionPreferences,
    ) -> Result<(), String> {
        account_settings::write_encryption_preferences(store_dir, preferences)
    }

    async fn load_legacy_encryption_preferences(
        client: &Client,
    ) -> Result<EncryptionPreferences, String> {
        let state_store = client.state_store();
        let Some(value) = state_store
            .get_custom_value(super::ENCRYPTION_PREFERENCES_CUSTOM_VALUE_KEY)
            .await
            .map_err(|error| format!("Failed to load encryption preferences: {error}"))?
        else {
            return Ok(EncryptionPreferences::default());
        };

        serde_json::from_slice(&value)
            .map_err(|error| format!("Failed to parse encryption preferences: {error}"))
    }

    fn discover_account_stores(app: &AppHandle) -> Result<Vec<AccountStorageLocation>, String> {
        let accounts_root = Self::accounts_root_dir(app)?;
        if !accounts_root.exists() {
            return Ok(Vec::new());
        }

        let mut stores = Vec::new();
        for entry in fs::read_dir(&accounts_root)
            .map_err(|error| format!("Failed to read the account storage root: {error}"))?
        {
            let entry = entry
                .map_err(|error| format!("Failed to inspect an account storage entry: {error}"))?;
            let file_type = entry.file_type().map_err(|error| {
                format!("Failed to inspect an account storage entry type: {error}")
            })?;

            if !file_type.is_dir() {
                continue;
            }

            let store_id = entry.file_name().to_string_lossy().into_owned();
            let Some(homeserver_url) = Self::decode_homeserver_url_from_store_id(&store_id) else {
                continue;
            };

            let store_dir = entry.path().join("store");
            if !store_dir.is_dir() {
                continue;
            }

            stores.push(AccountStorageLocation {
                store_id,
                store_dir,
                cache_dir: entry.path().join("cache"),
                homeserver_url,
            });
        }

        stores.sort_by(|left, right| left.store_id.cmp(&right.store_id));
        Ok(stores)
    }

    pub(super) fn load_or_create_store_key(
        app: &AppHandle,
        store_id: &str,
    ) -> Result<[u8; STORE_KEY_LENGTH], String> {
        if let Some(key) = Self::load_store_key(app, store_id)? {
            return Ok(key);
        }

        let mut key = [0_u8; STORE_KEY_LENGTH];
        fill_random_bytes(&mut key, "Failed to generate an account store key")?;
        secure_storage::set_secret(app, &Self::store_key_entry_id(store_id), &key)?;
        Ok(key)
    }

    pub(super) fn load_store_key(
        app: &AppHandle,
        store_id: &str,
    ) -> Result<Option<[u8; STORE_KEY_LENGTH]>, String> {
        let Some(secret) = secure_storage::get_secret(app, &Self::store_key_entry_id(store_id))?
        else {
            return Ok(None);
        };

        let secret_len = secret.len();
        let key_bytes: [u8; STORE_KEY_LENGTH] = secret.try_into().map_err(|_length_error| {
            format!(
                "Secure storage returned an invalid store key length for {store_id}: expected {STORE_KEY_LENGTH}, got {secret_len}",
            )
        })?;

        Ok(Some(key_bytes))
    }

    pub(super) fn store_key_entry_id(store_id: &str) -> String {
        format!("{STORE_KEY_PREFIX}::{store_id}")
    }

    fn account_store_id(homeserver_url: &str, account_hint: &str) -> String {
        let homeserver = URL_SAFE_NO_PAD.encode(homeserver_url.as_bytes());
        let account = URL_SAFE_NO_PAD.encode(account_hint.as_bytes());
        format!("v1__hs_{homeserver}__acct_{account}")
    }

    fn replacement_account_store_id(base_store_id: &str) -> Result<String, String> {
        let mut random_bytes = [0_u8; REPLACEMENT_STORE_ID_RANDOM_BYTES];
        fill_random_bytes(
            &mut random_bytes,
            "Failed to generate a replacement account store id",
        )?;
        let suffix = URL_SAFE_NO_PAD.encode(random_bytes);
        Ok(format!("{base_store_id}__fresh_{suffix}"))
    }

    fn decode_homeserver_url_from_store_id(store_id: &str) -> Option<String> {
        let encoded_homeserver = store_id.strip_prefix("v1__hs_")?.split_once("__acct_")?.0;

        let decoded = URL_SAFE_NO_PAD.decode(encoded_homeserver).ok()?;
        String::from_utf8(decoded).ok()
    }

    fn accounts_root_dir(app: &AppHandle) -> Result<PathBuf, String> {
        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|err| format!("Failed to resolve app data directory: {err}"))?;

        Ok(app_data_dir.join("matrix-accounts"))
    }
}

fn fill_random_bytes(destination: &mut [u8], failure_context: &str) -> Result<(), String> {
    let mut rng = SysRng;
    rng.try_fill_bytes(destination)
        .map_err(|error| format!("{failure_context}: {error}"))
}
