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
    sync::{Arc, RwLock},
    thread,
    time::Duration,
};

use matrix_sdk::Client;
use tauri::{AppHandle, Manager};

use super::types::{AccountSummary, LoginRequest};

struct ManagedAccount {
    // Each logged-in account owns its own Matrix client instance.
    _client: Client,
    user_id: String,
    homeserver_url: String,
    store_dir: PathBuf,
}

#[derive(Clone, Default)]
pub struct AccountManager {
    // The SDK does not manage multiple logged-in accounts for us, so we keep
    // one client per account and switch which one the UI treats as active.
    accounts: Arc<RwLock<HashMap<String, ManagedAccount>>>,
    active_account_key: Arc<RwLock<Option<String>>>,
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
        // Multi-account support requires isolated stores. Every account gets
        // its own sqlite database directory under the app data folder.
        let store_dir = self.account_store_dir(app, &request.homeserver_url, &request.username)?;

        let client = self.login_client_with_recovery(&store_dir, &request).await?;

        let user_id = client
            .user_id()
            .ok_or_else(|| String::from("Login succeeded, but user id is not available"))?
            .to_string();

        let account_key = user_id.clone();
        let homeserver_url = request.homeserver_url;
        let mut accounts = self
            .accounts
            .write()
            .expect("account manager accounts lock poisoned");
        // Replacing an existing entry lets the same account log in again
        // without leaving a stale client instance behind.
        accounts.insert(
            account_key.clone(),
            ManagedAccount {
                _client: client,
                user_id: user_id.clone(),
                homeserver_url: homeserver_url.clone(),
                store_dir: store_dir.clone(),
            },
        );

        let mut active_account = self
            .active_account_key
            .write()
            .expect("account manager active account lock poisoned");
        if active_account.is_none() {
            *active_account = Some(account_key.clone());
        }

        Ok(AccountSummary {
            account_key: account_key.clone(),
            user_id,
            homeserver_url,
            is_active: active_account.as_deref() == Some(account_key.as_str()),
        })
    }

    pub async fn list_accounts(&self) -> Vec<AccountSummary> {
        let accounts = self
            .accounts
            .read()
            .expect("account manager accounts lock poisoned");
        let active_account_key = self
            .active_account_key
            .read()
            .expect("account manager active account lock poisoned");

        accounts
            .iter()
            .map(|(account_key, account)| AccountSummary {
                account_key: account_key.clone(),
                user_id: account.user_id.clone(),
                homeserver_url: account.homeserver_url.clone(),
                is_active: active_account_key.as_deref() == Some(account_key.as_str()),
            })
            .collect()
    }

    pub async fn switch_active_account(&self, account_key: &str) -> Result<(), String> {
        let accounts = self
            .accounts
            .read()
            .expect("account manager accounts lock poisoned");
        if !accounts.contains_key(account_key) {
            return Err(format!("Unknown account key: {account_key}"));
        }
        drop(accounts);

        let mut active_account_key = self
            .active_account_key
            .write()
            .expect("account manager active account lock poisoned");
        *active_account_key = Some(account_key.to_owned());
        Ok(())
    }

    pub async fn active_account(&self) -> Option<AccountSummary> {
        let accounts = self
            .accounts
            .read()
            .expect("account manager accounts lock poisoned");
        let active_account_key = self
            .active_account_key
            .read()
            .expect("account manager active account lock poisoned");
        let key = active_account_key.clone()?;
        let account = accounts.get(&key)?;

        Some(AccountSummary {
            account_key: key.clone(),
            user_id: account.user_id.clone(),
            homeserver_url: account.homeserver_url.clone(),
            is_active: true,
        })
    }

    pub async fn validate_active_account(&self) -> Result<Option<AccountSummary>, String> {
        let Some((account_summary, client, store_dir)) = self.active_account_snapshot() else {
            return Ok(None);
        };

        match client.whoami().await {
            Ok(_) => Ok(Some(account_summary)),
            Err(error) if Self::is_invalid_session_error(&error.to_string()) => {
                drop(client);
                self.release_accounts_for_store_dir(&store_dir);

                if let Err(cleanup_error) = Self::reset_store_dir(&store_dir) {
                    eprintln!(
                        "Failed to clean up the local store for a deauthorized account: \
                         {cleanup_error}"
                    );
                }

                Ok(None)
            }
            Err(error) => Err(format!("Failed to validate active account session: {error}")),
        }
    }

    fn account_store_dir(
        &self,
        app: &AppHandle,
        homeserver_url: &str,
        username: &str,
    ) -> Result<PathBuf, String> {
        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|err| format!("Failed to resolve app data directory: {err}"))?;

        // Homeserver + username makes the on-disk store stable before we know
        // the final Matrix user id returned by the server.
        let account_folder = format!(
            "{}_{}",
            Self::sanitize_for_path(homeserver_url),
            Self::sanitize_for_path(username)
        );

        let store_dir = app_data_dir
            .join("matrix-accounts")
            .join(account_folder)
            .join("store");

        fs::create_dir_all(&store_dir)
            .map_err(|err| format!("Failed to create account store directory: {err}"))?;

        Ok(store_dir)
    }

    async fn login_client_with_recovery(
        &self,
        store_dir: &Path,
        request: &LoginRequest,
    ) -> Result<Client, String> {
        match Self::build_and_login_client(store_dir, request).await {
            Ok(client) => Ok(client),
            Err(error_message) if Self::is_stale_crypto_store_error(&error_message) => {
                // If the server issued this account a new device ID, the old crypto
                // store can no longer be reused. Reset just this account's local
                // store and retry the login once with a clean database.
                self.release_accounts_for_store_dir(store_dir);
                Self::reset_store_dir(store_dir)?;

                Self::build_and_login_client(store_dir, request)
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
            .filter_map(|(account_key, account)| {
                (account.store_dir == store_dir).then(|| account_key.clone())
            })
            .collect();

        if matching_account_keys.is_empty() {
            return;
        }

        let mut active_account_key = self
            .active_account_key
            .write()
            .expect("account manager active account lock poisoned");

        let removed_active_account = active_account_key.as_ref().is_some_and(|active_key| {
            matching_account_keys
                .iter()
                .any(|account_key| account_key == active_key)
        });

        for account_key in &matching_account_keys {
            accounts.remove(account_key);
        }

        if removed_active_account {
            *active_account_key = accounts.keys().next().cloned();
        }
    }

    fn active_account_snapshot(&self) -> Option<(AccountSummary, Client, PathBuf)> {
        let accounts = self
            .accounts
            .read()
            .expect("account manager accounts lock poisoned");
        let active_account_key = self
            .active_account_key
            .read()
            .expect("account manager active account lock poisoned");
        let key = active_account_key.clone()?;
        let account = accounts.get(&key)?;

        Some((
            AccountSummary {
                account_key: key,
                user_id: account.user_id.clone(),
                homeserver_url: account.homeserver_url.clone(),
                is_active: true,
            },
            account._client.clone(),
            account.store_dir.clone(),
        ))
    }

    async fn build_and_login_client(
        store_dir: &Path,
        request: &LoginRequest,
    ) -> Result<Client, String> {
        let client = Client::builder()
            .homeserver_url(request.homeserver_url.clone())
            .sqlite_store(store_dir, None)
            .build()
            .await
            .map_err(|err| format!("Failed to build Matrix client: {err}"))?;

        let mut login_builder = client
            .matrix_auth()
            .login_username(&request.username, &request.password);

        if let Some(device_name) = &request.device_display_name {
            login_builder = login_builder.initial_device_display_name(device_name);
        }

        login_builder
            .send()
            .await
            .map_err(|err| format!("Login failed: {err}"))?;

        Ok(client)
    }

    fn is_stale_crypto_store_error(error_message: &str) -> bool {
        error_message.contains("account in the store doesn't match the account in the constructor")
    }

    fn is_invalid_session_error(error_message: &str) -> bool {
        error_message.contains("M_UNKNOWN_TOKEN")
            || error_message.contains("UnknownToken")
            || error_message.contains("unknown token")
    }

    fn reset_store_dir(store_dir: &Path) -> Result<(), String> {
        const RESET_RETRY_ATTEMPTS: usize = 20;
        const RESET_RETRY_DELAY: Duration = Duration::from_millis(100);

        for attempt in 0..RESET_RETRY_ATTEMPTS {
            if store_dir.exists() {
                match fs::remove_dir_all(store_dir) {
                    Ok(()) => {}
                    Err(err)
                        if err.raw_os_error() == Some(32)
                            && attempt + 1 < RESET_RETRY_ATTEMPTS =>
                    {
                        thread::sleep(RESET_RETRY_DELAY);
                        continue;
                    }
                    Err(err) => {
                        return Err(format!(
                            "Failed to reset stale account store directory: {err}"
                        ));
                    }
                }
            }

            fs::create_dir_all(store_dir)
                .map_err(|err| format!("Failed to recreate account store directory: {err}"))?;

            return Ok(());
        }

        Err(String::from(
            "Failed to reset stale account store directory after repeated retries",
        ))
    }

    fn sanitize_for_path(input: &str) -> String {
        input
            .chars()
            .map(|c| match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' => c,
                _ => '_',
            })
            .collect()
    }
}
