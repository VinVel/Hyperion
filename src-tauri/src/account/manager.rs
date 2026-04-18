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

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use matrix_sdk::{
    Client, Error as MatrixError, SqliteStoreConfig,
    authentication::matrix::MatrixSession,
    ruma::api::client::account::register::v3::{
        Request as MatrixRegistrationRequest, Response as MatrixRegistrationResponse,
    },
};
use rand::{RngCore, rngs::OsRng};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use tauri::async_runtime::Mutex as AsyncMutex;
use tauri::{AppHandle, Manager};

use super::secure_storage;
use super::types::{
    AccountSummary, HomeserverDirectory, HomeserverDirectoryEntry, LoginRequest,
    RegisterAccountRequest, RegistrationFlow, RegistrationOutcome,
};

const HOMESERVER_DIRECTORY_URL: &str = "https://servers.joinmatrix.org/servers.json";
const SESSION_CUSTOM_VALUE_KEY: &[u8] = b"hyperion.account.session.v1";
const ACCOUNT_METADATA_CUSTOM_VALUE_KEY: &[u8] = b"hyperion.account.metadata.v1";
const STORE_KEY_PREFIX: &str = "matrix-store-key";
const STORE_KEY_LENGTH: usize = 32;

struct ManagedAccount {
    // Each logged-in account owns its own Matrix client instance.
    _client: Client,
    user_id: String,
    homeserver_url: String,
    store_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredAccountMetadata {
    user_id: String,
    homeserver_url: String,
    is_active: bool,
}

struct AccountStorageLocation {
    store_id: String,
    store_dir: PathBuf,
    homeserver_url: String,
}

#[derive(Clone, Default)]
pub struct AccountManager {
    // The SDK does not manage multiple logged-in accounts for us, so we keep
    // one client per account and switch which one the UI treats as active.
    accounts: Arc<RwLock<HashMap<String, ManagedAccount>>>,
    active_account_key: Arc<RwLock<Option<String>>>,
    restore_lock: Arc<AsyncMutex<()>>,
    restore_completed: Arc<RwLock<bool>>,
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
        let client = self
            .login_client_with_recovery(app, &storage, &store_key, &request)
            .await?;
        let session = client
            .matrix_auth()
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

        let accounts = self
            .accounts
            .read()
            .expect("account manager accounts lock poisoned");
        let active_account_key = self
            .active_account_key
            .read()
            .expect("account manager active account lock poisoned");
        let Some(key) = active_account_key.clone() else {
            return Ok(None);
        };
        let Some(account) = accounts.get(&key) else {
            return Ok(None);
        };

        Ok(Some(AccountSummary {
            account_key: key,
            user_id: account.user_id.clone(),
            homeserver_url: account.homeserver_url.clone(),
            is_active: true,
        }))
    }

    pub async fn validate_active_account(
        &self,
        app: &AppHandle,
    ) -> Result<Option<AccountSummary>, String> {
        self.ensure_loaded(app).await?;

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

                self.persist_account_store_metadata().await?;
                Ok(None)
            }
            Err(error) => Err(format!(
                "Failed to validate active account session: {error}"
            )),
        }
    }

    pub async fn list_registration_homeservers(&self) -> Result<HomeserverDirectory, String> {
        // Fetch the directory on demand so the UI always sees the latest
        // registration metadata published by joinmatrix.org.
        let mut directory = HttpClient::new()
            .get(HOMESERVER_DIRECTORY_URL)
            .send()
            .await
            .map_err(|err| format!("Failed to fetch the homeserver directory: {err}"))?
            .error_for_status()
            .map_err(|err| format!("Failed to fetch the homeserver directory: {err}"))?
            .json::<HomeserverDirectory>()
            .await
            .map_err(|err| format!("Failed to parse the homeserver directory: {err}"))?;

        for homeserver in &mut directory.public_servers {
            Self::enrich_homeserver_entry(homeserver);
        }

        Ok(directory)
    }

    pub async fn register_account(
        &self,
        app: &AppHandle,
        request: RegisterAccountRequest,
    ) -> Result<RegistrationOutcome, String> {
        self.ensure_loaded(app).await?;

        // Resolve the server against a fresh directory response each time the
        // register button is pressed, as requested by the flow design.
        let directory = self.list_registration_homeservers().await?;
        let homeserver = directory
            .public_servers
            .into_iter()
            .find(|homeserver| homeserver.server_id == request.server_id)
            .ok_or_else(|| format!("Unknown homeserver id: {}", request.server_id))?;

        match homeserver.registration_flow {
            RegistrationFlow::MatrixSdk => {
                self.register_with_matrix_sdk(app, homeserver, request)
                    .await
            }
            RegistrationFlow::ExternalLink => self.open_external_registration(homeserver),
            RegistrationFlow::InfoOnly => Ok(RegistrationOutcome::InformationOnly {
                homeserver,
                message: String::from(
                    "This homeserver uses a registration flow that Hyperion does not implement yet. Present its metadata in the UI for manual guidance.",
                ),
            }),
        }
    }

    fn account_storage(
        &self,
        app: &AppHandle,
        homeserver_url: &str,
        account_hint: &str,
    ) -> Result<AccountStorageLocation, String> {
        let accounts_root = self.accounts_root_dir(app)?;

        // Homeserver + account hint makes the on-disk store stable before we
        // know the final Matrix user id returned by the server. This keeps each
        // account in its own sqlite database, which is required for
        // multi-account support with the Matrix Rust SDK.
        let store_id = Self::account_store_id(homeserver_url, account_hint);
        let store_dir = accounts_root.join(&store_id).join("store");

        fs::create_dir_all(&store_dir)
            .map_err(|err| format!("Failed to create account store directory: {err}"))?;

        Ok(AccountStorageLocation {
            store_id,
            store_dir,
            homeserver_url: homeserver_url.to_owned(),
        })
    }

    async fn login_client_with_recovery(
        &self,
        _app: &AppHandle,
        storage: &AccountStorageLocation,
        store_key: &[u8; STORE_KEY_LENGTH],
        request: &LoginRequest,
    ) -> Result<Client, String> {
        match Self::build_and_login_client(&storage.store_dir, store_key, request).await {
            Ok(client) => Ok(client),
            Err(error_message) if Self::is_stale_crypto_store_error(&error_message) => {
                // If the server issued this account a new device ID, the old crypto
                // store can no longer be reused. Reset just this account's local
                // store and retry the login once with a clean database.
                self.release_accounts_for_store_dir(&storage.store_dir);
                Self::reset_store_dir(&storage.store_dir)?;

                Self::build_and_login_client(&storage.store_dir, store_key, request)
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
            .filter(|(_, account)| account.store_dir == store_dir)
            .map(|(account_key, _)| account_key.clone())
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

    async fn register_with_matrix_sdk(
        &self,
        app: &AppHandle,
        homeserver: HomeserverDirectoryEntry,
        request: RegisterAccountRequest,
    ) -> Result<RegistrationOutcome, String> {
        let homeserver_url = homeserver
            .homeserver_url
            .clone()
            .ok_or_else(|| String::from("The selected homeserver does not expose a client URL"))?;
        let homeserver_target = Self::registration_homeserver_target(&homeserver);
        let storage = self.account_storage(app, &homeserver_url, &request.username)?;
        let store_key = Self::load_or_create_store_key(app, &storage.store_id)?;
        let client = Self::build_client(&homeserver_target, &storage.store_dir, &store_key).await?;

        match Self::perform_registration(&client, &request).await {
            Ok(_) => {}
            Err(error) if error.as_uiaa_response().is_some() => {
                return self.handle_uiaa_registration_requirement(homeserver, error);
            }
            Err(error) => {
                return Err(format!("Registration failed: {error}"));
            }
        }

        let mut notes = Vec::new();

        // Matrix registration creates the account first; optional profile data
        // such as the display name is set once the SDK has established a session.
        if let Some(display_name) = request.display_name.as_deref()
            && let Err(error) = client.account().set_display_name(Some(display_name)).await
        {
            notes.push(format!(
                "The account was created, but setting the display name failed: {error}"
            ));
        }

        if request.email.is_some() {
            notes.push(String::from(
                "Email was collected in the request, but Matrix email verification flows are not implemented in Hyperion yet.",
            ));
        }

        let session = client.matrix_auth().session().ok_or_else(|| {
            String::from("Registration succeeded, but session data is not available")
        })?;
        let user_id = session.meta.user_id.to_string();

        Self::persist_session(&client, &session).await?;
        let account = self.store_logged_in_account(
            user_id.clone(),
            user_id,
            Self::client_homeserver_url(&client),
            storage.store_dir,
            client,
        );
        self.persist_account_store_metadata().await?;

        Ok(RegistrationOutcome::Registered {
            account,
            homeserver,
            email_submitted: request.email.is_some(),
            email_applied: false,
            note: (!notes.is_empty()).then(|| notes.join(" ")),
        })
    }

    fn open_external_registration(
        &self,
        homeserver: HomeserverDirectoryEntry,
    ) -> Result<RegistrationOutcome, String> {
        let reg_link = homeserver.reg_link.clone().ok_or_else(|| {
            String::from("The selected homeserver does not provide a registration link")
        })?;

        Ok(RegistrationOutcome::ExternalRegistrationOpened {
            homeserver,
            reg_link,
        })
    }

    fn handle_uiaa_registration_requirement(
        &self,
        homeserver: HomeserverDirectoryEntry,
        error: MatrixError,
    ) -> Result<RegistrationOutcome, String> {
        if homeserver.reg_link.is_some() {
            return self.open_external_registration(homeserver);
        }

        Ok(RegistrationOutcome::InformationOnly {
            homeserver,
            message: format!(
                "This homeserver requires interactive registration steps that Hyperion does not \
                 support yet. Complete the server's registration flow first, then sign in here. \
                 Details: {error}"
            ),
        })
    }

    async fn build_and_login_client(
        store_dir: &Path,
        store_key: &[u8; STORE_KEY_LENGTH],
        request: &LoginRequest,
    ) -> Result<Client, String> {
        let client = Self::build_client(&request.homeserver_url, store_dir, store_key).await?;

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

    async fn build_client(
        homeserver_target: &str,
        store_dir: &Path,
        store_key: &[u8; STORE_KEY_LENGTH],
    ) -> Result<Client, String> {
        let store_config = SqliteStoreConfig::new(store_dir).key(Some(store_key));

        Client::builder()
            .server_name_or_homeserver_url(homeserver_target)
            .sqlite_store_with_config_and_cache_path(store_config, Option::<&Path>::None)
            .build()
            .await
            .map_err(|err| format!("Failed to build Matrix client: {err}"))
    }

    async fn perform_registration(
        client: &Client,
        request: &RegisterAccountRequest,
    ) -> Result<MatrixRegistrationResponse, MatrixError> {
        let mut registration_request = MatrixRegistrationRequest::new();
        registration_request.username = Some(request.username.clone());
        registration_request.password = Some(request.password.clone());
        registration_request.initial_device_display_name = request.device_display_name.clone();

        client.matrix_auth().register(registration_request).await
    }

    fn store_logged_in_account(
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
                _client: client,
                user_id: user_id.clone(),
                homeserver_url: homeserver_url.clone(),
                store_dir,
            },
        );

        let mut active_account = self
            .active_account_key
            .write()
            .expect("account manager active account lock poisoned");
        if active_account.is_none() {
            *active_account = Some(account_key.clone());
        }

        AccountSummary {
            account_key: account_key.clone(),
            user_id,
            homeserver_url,
            is_active: active_account.as_deref() == Some(account_key.as_str()),
        }
    }

    fn enrich_homeserver_entry(homeserver: &mut HomeserverDirectoryEntry) {
        homeserver.server_id = Self::derive_server_id(homeserver);
        homeserver.homeserver_url = Self::derive_homeserver_url(homeserver);
        homeserver.registration_flow = Self::derive_registration_flow(homeserver);
        homeserver.supports_display_name =
            matches!(homeserver.registration_flow, RegistrationFlow::MatrixSdk);
    }

    fn derive_server_id(homeserver: &HomeserverDirectoryEntry) -> String {
        homeserver
            .client_domain
            .clone()
            .or_else(|| homeserver.server_domain.clone())
            .unwrap_or_else(|| homeserver.name.clone())
    }

    fn derive_homeserver_url(homeserver: &HomeserverDirectoryEntry) -> Option<String> {
        homeserver
            .client_domain
            .as_deref()
            .or(homeserver.server_domain.as_deref())
            .map(Self::ensure_https_url)
    }

    fn registration_homeserver_target(homeserver: &HomeserverDirectoryEntry) -> String {
        homeserver
            .server_domain
            .clone()
            .or_else(|| homeserver.client_domain.clone())
            .or_else(|| homeserver.homeserver_url.clone())
            .unwrap_or_else(|| homeserver.server_id.clone())
    }

    fn derive_registration_flow(homeserver: &HomeserverDirectoryEntry) -> RegistrationFlow {
        if homeserver.using_vanilla_reg == Some(true) {
            RegistrationFlow::MatrixSdk
        } else if homeserver.using_vanilla_reg == Some(false)
            && homeserver.reg_link.is_some()
            && matches!(
                homeserver.reg_method.as_deref(),
                Some("SSO") | Some("In-house Element") | Some("Application Form")
            )
        {
            RegistrationFlow::ExternalLink
        } else {
            RegistrationFlow::InfoOnly
        }
    }

    fn ensure_https_url(value: &str) -> String {
        if value.contains("://") {
            value.to_owned()
        } else {
            format!("https://{value}")
        }
    }

    fn is_stale_crypto_store_error(error_message: &str) -> bool {
        error_message.contains("account in the store doesn't match the account in the constructor")
    }

    fn is_invalid_session_error(error_message: &str) -> bool {
        error_message.contains("M_UNKNOWN_TOKEN")
            || error_message.contains("UnknownToken")
            || error_message.contains("unknown token")
    }

    fn remove_dir_with_retries(path: &Path, failure_context: &str) -> Result<(), String> {
        const RESET_RETRY_ATTEMPTS: usize = 20;
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

    fn reset_store_dir(store_dir: &Path) -> Result<(), String> {
        Self::remove_dir_with_retries(store_dir, "Failed to reset stale account store directory")?;
        fs::create_dir_all(store_dir)
            .map_err(|err| format!("Failed to recreate account store directory: {err}"))
    }

    async fn ensure_loaded(&self, app: &AppHandle) -> Result<(), String> {
        if *self
            .restore_completed
            .read()
            .expect("account manager restore flag lock poisoned")
        {
            return Ok(());
        }

        let _guard = self.restore_lock.lock().await;
        if *self
            .restore_completed
            .read()
            .expect("account manager restore flag lock poisoned")
        {
            return Ok(());
        }

        self.restore_accounts_state(app).await?;
        *self
            .restore_completed
            .write()
            .expect("account manager restore flag lock poisoned") = true;

        Ok(())
    }

    async fn restore_accounts_state(&self, app: &AppHandle) -> Result<(), String> {
        let discovered_stores = self.discover_account_stores(app)?;
        let mut restored_accounts = HashMap::new();
        let mut active_account_key = None;

        for storage in discovered_stores {
            let Some(store_key) = Self::load_store_key(app, &storage.store_id)? else {
                eprintln!(
                    "Skipping persisted account store {} because its secure encryption key is missing",
                    storage.store_id
                );
                continue;
            };

            let client = match Self::build_client(
                &storage.homeserver_url,
                &storage.store_dir,
                &store_key,
            )
            .await
            {
                Ok(client) => client,
                Err(error) => {
                    eprintln!(
                        "Skipping persisted account store {} because the Matrix client could not be rebuilt: {error}",
                        storage.store_id
                    );
                    continue;
                }
            };

            let metadata = match Self::load_account_metadata(&client).await? {
                Some(metadata) => metadata,
                None => {
                    drop(client);
                    Self::prune_incomplete_store(app, &storage, "its metadata is missing");
                    continue;
                }
            };

            let Some(session) = Self::load_session(&client).await? else {
                drop(client);
                Self::prune_incomplete_store(app, &storage, "its session is missing");
                continue;
            };

            if let Err(error) = client.restore_session(session).await {
                eprintln!(
                    "Skipping persisted account {} because the Matrix session could not be restored: {error}",
                    metadata.user_id
                );
                continue;
            }

            if metadata.is_active && active_account_key.is_none() {
                active_account_key = Some(metadata.user_id.clone());
            }

            restored_accounts.insert(
                metadata.user_id.clone(),
                ManagedAccount {
                    _client: client,
                    user_id: metadata.user_id,
                    homeserver_url: metadata.homeserver_url,
                    store_dir: storage.store_dir,
                },
            );
        }

        if active_account_key.is_none() {
            active_account_key = restored_accounts.keys().next().cloned();
        }

        *self
            .accounts
            .write()
            .expect("account manager accounts lock poisoned") = restored_accounts;
        *self
            .active_account_key
            .write()
            .expect("account manager active account lock poisoned") = active_account_key;

        self.persist_account_store_metadata().await?;
        Ok(())
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
            "Failed to remove incomplete account store root directory",
        ) {
            eprintln!("{error}");
            return;
        }

        if let Err(error) =
            secure_storage::delete_secret(app, &Self::store_key_entry_id(&storage.store_id))
        {
            eprintln!("Failed to remove orphaned account store key: {error}");
        }
    }

    async fn persist_account_store_metadata(&self) -> Result<(), String> {
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
                        account._client.clone(),
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

    async fn persist_session(client: &Client, session: &MatrixSession) -> Result<(), String> {
        let value = serde_json::to_vec(session)
            .map_err(|error| format!("Failed to serialize persisted Matrix session: {error}"))?;

        client
            .state_store()
            .set_custom_value_no_read(SESSION_CUSTOM_VALUE_KEY, value)
            .await
            .map_err(|error| {
                format!("Failed to persist Matrix session in the encrypted store: {error}")
            })
    }

    async fn load_session(client: &Client) -> Result<Option<MatrixSession>, String> {
        let Some(value) = client
            .state_store()
            .get_custom_value(SESSION_CUSTOM_VALUE_KEY)
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

        client
            .state_store()
            .set_custom_value_no_read(ACCOUNT_METADATA_CUSTOM_VALUE_KEY, value)
            .await
            .map_err(|error| {
                format!("Failed to persist account metadata in the encrypted store: {error}")
            })
    }

    async fn load_account_metadata(
        client: &Client,
    ) -> Result<Option<StoredAccountMetadata>, String> {
        let Some(value) = client
            .state_store()
            .get_custom_value(ACCOUNT_METADATA_CUSTOM_VALUE_KEY)
            .await
            .map_err(|error| format!("Failed to load persisted account metadata: {error}"))?
        else {
            return Ok(None);
        };

        serde_json::from_slice(&value)
            .map(Some)
            .map_err(|error| format!("Failed to parse persisted account metadata: {error}"))
    }

    fn discover_account_stores(
        &self,
        app: &AppHandle,
    ) -> Result<Vec<AccountStorageLocation>, String> {
        let accounts_root = self.accounts_root_dir(app)?;
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
                homeserver_url,
            });
        }

        stores.sort_by(|left, right| left.store_id.cmp(&right.store_id));
        Ok(stores)
    }

    fn load_or_create_store_key(
        app: &AppHandle,
        store_id: &str,
    ) -> Result<[u8; STORE_KEY_LENGTH], String> {
        if let Some(key) = Self::load_store_key(app, store_id)? {
            return Ok(key);
        }

        let mut key = [0_u8; STORE_KEY_LENGTH];
        OsRng.fill_bytes(&mut key);
        secure_storage::set_secret(app, &Self::store_key_entry_id(store_id), &key)?;
        Ok(key)
    }

    fn load_store_key(
        app: &AppHandle,
        store_id: &str,
    ) -> Result<Option<[u8; STORE_KEY_LENGTH]>, String> {
        let Some(secret) = secure_storage::get_secret(app, &Self::store_key_entry_id(store_id))?
        else {
            return Ok(None);
        };

        let secret_len = secret.len();
        let key_bytes: [u8; STORE_KEY_LENGTH] = secret.try_into().map_err(|_| {
            format!(
                "Secure storage returned an invalid store key length for {store_id}: expected {}, got {secret_len}",
                STORE_KEY_LENGTH
            )
        })?;

        Ok(Some(key_bytes))
    }

    fn store_key_entry_id(store_id: &str) -> String {
        format!("{STORE_KEY_PREFIX}::{store_id}")
    }

    fn account_store_id(homeserver_url: &str, account_hint: &str) -> String {
        let homeserver = URL_SAFE_NO_PAD.encode(homeserver_url.as_bytes());
        let account = URL_SAFE_NO_PAD.encode(account_hint.as_bytes());
        format!("v1__hs_{homeserver}__acct_{account}")
    }

    fn decode_homeserver_url_from_store_id(store_id: &str) -> Option<String> {
        let encoded_homeserver = store_id.strip_prefix("v1__hs_")?.split_once("__acct_")?.0;

        let decoded = URL_SAFE_NO_PAD.decode(encoded_homeserver).ok()?;
        String::from_utf8(decoded).ok()
    }

    fn accounts_root_dir(&self, app: &AppHandle) -> Result<PathBuf, String> {
        Ok(app
            .path()
            .app_data_dir()
            .map_err(|err| format!("Failed to resolve app data directory: {err}"))?
            .join("matrix-accounts"))
    }

    fn client_homeserver_url(client: &Client) -> String {
        client
            .homeserver()
            .to_string()
            .trim_end_matches('/')
            .to_owned()
    }
}
