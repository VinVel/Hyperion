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
    path::PathBuf,
    sync::{Arc, RwLock},
};

use matrix_sdk::Client;
use tauri::{AppHandle, Manager};

use super::types::{AccountSummary, LoginRequest};

struct ManagedAccount {
    // Each logged-in account owns its own Matrix client instance.
    _client: Client,
    user_id: String,
    homeserver_url: String,
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
