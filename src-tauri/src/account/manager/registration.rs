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
    Client, Error as MatrixError,
    ruma::api::client::account::register::v3::{
        Request as MatrixRegistrationRequest, Response as MatrixRegistrationResponse,
    },
};
use reqwest::Client as HttpClient;
use tauri::AppHandle;

use super::AccountManager;
use crate::account::types::{
    HomeserverDirectory, HomeserverDirectoryEntry, RegisterAccountRequest, RegistrationFlow,
    RegistrationOutcome,
};

const HOMESERVER_DIRECTORY_URL: &str = "https://servers.joinmatrix.org/servers.json";

impl AccountManager {
    pub async fn list_registration_homeservers(&self) -> Result<HomeserverDirectory, String> {
        // Fetch the directory on demand so the UI always sees the latest
        // registration metadata published by joinmatrix.org.
        let http_client = HttpClient::new();
        let response = http_client
            .get(HOMESERVER_DIRECTORY_URL)
            .send()
            .await
            .map_err(|err| format!("Failed to fetch the homeserver directory: {err}"))?;
        let response = response
            .error_for_status()
            .map_err(|err| format!("Failed to fetch the homeserver directory: {err}"))?;
        let mut directory = response
            .json::<HomeserverDirectory>()
            .await
            .map_err(|err| format!("Failed to parse the homeserver directory: {err}"))?;

        for homeserver in &mut directory.public_servers {
            enrich_homeserver_entry(homeserver);
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
            RegistrationFlow::ExternalLink => open_external_registration(homeserver),
            RegistrationFlow::InfoOnly => Ok(RegistrationOutcome::InformationOnly {
                homeserver,
                message: String::from(
                    "This homeserver uses a registration flow that Hyperion does not implement yet. Present its metadata in the UI for manual guidance.",
                ),
            }),
        }
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
        let homeserver_target = registration_homeserver_target(&homeserver);
        let storage = self.account_storage(app, &homeserver_url, &request.username)?;
        let store_key = Self::load_or_create_store_key(app, &storage.store_id)?;
        let client = Self::build_client(
            &homeserver_target,
            &storage.store_dir,
            &storage.cache_dir,
            &store_key,
        )
        .await?;
        if let Err(error) = perform_registration(&client, &request).await {
            if error.as_uiaa_response().is_some() {
                return handle_uiaa_registration_requirement(homeserver, &error);
            }

            return Err(format!("Registration failed: {error}"));
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
}

fn open_external_registration(
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
    homeserver: HomeserverDirectoryEntry,
    error: &MatrixError,
) -> Result<RegistrationOutcome, String> {
    if homeserver.reg_link.is_some() {
        return open_external_registration(homeserver);
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

async fn perform_registration(
    client: &Client,
    request: &RegisterAccountRequest,
) -> Result<MatrixRegistrationResponse, MatrixError> {
    let mut registration_request = MatrixRegistrationRequest::new();
    registration_request.username = Some(request.username.clone());
    registration_request.password = Some(request.password.clone());
    registration_request.initial_device_display_name = request.device_display_name.clone();

    let matrix_auth = client.matrix_auth();
    matrix_auth.register(registration_request).await
}

fn enrich_homeserver_entry(homeserver: &mut HomeserverDirectoryEntry) {
    homeserver.server_id = derive_server_id(homeserver);
    homeserver.homeserver_url = derive_homeserver_url(homeserver);
    homeserver.registration_flow = derive_registration_flow(homeserver);
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
        .map(ensure_https_url)
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
            Some("SSO" | "In-house Element" | "Application Form")
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
