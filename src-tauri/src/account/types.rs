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

use std::{collections::BTreeMap, path::PathBuf};

use matrix_sdk::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub homeserver_url: String,
    pub username: String,
    pub password: String,
    pub device_display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterAccountRequest {
    pub server_id: String,
    pub username: String,
    pub password: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub device_display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountSummary {
    pub account_key: String,
    pub user_id: String,
    pub homeserver_url: String,
    pub is_active: bool,
}

#[derive(Clone)]
pub struct AccountClientSnapshot {
    pub account_key: String,
    pub homeserver_url: String,
    pub client: Client,
    pub store_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct StoredAccountMetadata {
    pub user_id: String,
    pub homeserver_url: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptionPreferences {
    #[serde(default)]
    pub server_key_storage_opted_out: bool,
    #[serde(default)]
    pub verified_devices_only: bool,
    #[serde(default)]
    pub share_encrypted_history_on_invite: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationFlow {
    MatrixSdk,
    ExternalLink,
    #[default]
    InfoOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeserverDirectory {
    #[serde(default)]
    pub public_servers: Vec<HomeserverDirectoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeserverDirectoryEntry {
    #[serde(default)]
    pub server_id: String,
    pub homeserver_url: Option<String>,
    #[serde(default)]
    pub registration_flow: RegistrationFlow,
    #[serde(default)]
    pub supports_display_name: bool,
    #[serde(default)]
    pub name: String,
    pub client_domain: Option<String>,
    pub homepage: Option<String>,
    pub isp: Option<String>,
    pub staff_jur: Option<String>,
    pub rules: Option<String>,
    pub privacy: Option<String>,
    pub using_vanilla_reg: Option<bool>,
    pub description: Option<String>,
    pub reg_method: Option<String>,
    pub reg_link: Option<String>,
    pub reg_note: Option<String>,
    pub software: Option<String>,
    pub version: Option<String>,
    pub captcha: Option<bool>,
    pub captcha_note: Option<String>,
    pub email: Option<bool>,
    pub longstanding: Option<bool>,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub features: Vec<String>,
    pub online_status: Option<i64>,
    pub server_domain: Option<String>,
    pub ver_status: Option<i64>,
    pub room_directory: Option<i64>,
    pub sliding_sync: Option<bool>,
    pub ipv6: Option<bool>,
    pub cloudflare: Option<bool>,
    #[serde(default, flatten)]
    pub extra_fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RegistrationOutcome {
    Registered {
        account: AccountSummary,
        homeserver: HomeserverDirectoryEntry,
        email_submitted: bool,
        email_applied: bool,
        note: Option<String>,
    },
    ExternalRegistrationOpened {
        homeserver: HomeserverDirectoryEntry,
        reg_link: String,
    },
    InformationOnly {
        homeserver: HomeserverDirectoryEntry,
        message: String,
    },
}
