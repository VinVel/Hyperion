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

use matrix_sdk::{
    Client, Error as MatrixError,
    ruma::api::client::account::register::v3::{
        Request as MatrixRegistrationRequest, Response as MatrixRegistrationResponse,
    },
};
use reqwest::Client as HttpClient;
use tauri::{AppHandle, Manager};

use super::types::{
    AccountSummary, HomeserverDirectory, HomeserverDirectoryEntry, LoginRequest,
    RegisterAccountRequest, RegistrationFlow, RegistrationOutcome,
};

const HOMESERVER_DIRECTORY_URL: &str = "https://servers.joinmatrix.org/servers.json";

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
        let client = self
            .login_client_with_recovery(&store_dir, &request)
            .await?;
        let user_id = client
            .user_id()
            .ok_or_else(|| String::from("Login succeeded, but user id is not available"))?
            .to_string();

        Ok(self.store_logged_in_account(
            user_id.clone(),
            user_id,
            request.homeserver_url,
            store_dir,
            client,
        ))
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
        // the final Matrix user id returned by the server. This keeps each
        // account in its own sqlite database, which is required for
        // multi-account support with the Matrix Rust SDK.
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
        let store_dir = self.account_store_dir(app, &homeserver_url, &request.username)?;
        let client = Self::build_client(&homeserver_target, &store_dir).await?;
        let response = match Self::perform_registration(&client, &request).await {
            Ok(response) => response,
            Err(error) if error.as_uiaa_response().is_some() => {
                return self.handle_uiaa_registration_requirement(homeserver, error);
            }
            Err(error) => {
                return Err(format!("Registration failed: {error}"));
            }
        };

        let mut notes = Vec::new();

        // Matrix registration creates the account first; optional profile data
        // such as the display name is set once the SDK has established a session.
        if let Some(display_name) = request.display_name.as_deref() {
            if let Err(error) = client.account().set_display_name(Some(display_name)).await {
                notes.push(format!(
                    "The account was created, but setting the display name failed: {error}"
                ));
            }
        }

        if request.email.is_some() {
            notes.push(String::from(
                "Email was collected in the request, but Matrix email verification flows are not implemented in Hyperion yet.",
            ));
        }

        let user_id = response.user_id.to_string();
        let account = self.store_logged_in_account(
            user_id.clone(),
            user_id,
            homeserver_url,
            store_dir,
            client,
        );

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
        request: &LoginRequest,
    ) -> Result<Client, String> {
        let client = Self::build_client(&request.homeserver_url, store_dir).await?;

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

    async fn build_client(homeserver_target: &str, store_dir: &Path) -> Result<Client, String> {
        Client::builder()
            .server_name_or_homeserver_url(homeserver_target)
            .sqlite_store(store_dir, None)
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

    fn reset_store_dir(store_dir: &Path) -> Result<(), String> {
        const RESET_RETRY_ATTEMPTS: usize = 20;
        const RESET_RETRY_DELAY: Duration = Duration::from_millis(100);

        for attempt in 0..RESET_RETRY_ATTEMPTS {
            if store_dir.exists() {
                match fs::remove_dir_all(store_dir) {
                    Ok(()) => {}
                    Err(err)
                        if err.raw_os_error() == Some(32) && attempt + 1 < RESET_RETRY_ATTEMPTS =>
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
