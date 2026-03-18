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

use std::collections::BTreeMap;

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
    #[serde(default)]
    pub homeserver_url: Option<String>,
    #[serde(default)]
    pub registration_flow: RegistrationFlow,
    #[serde(default)]
    pub supports_display_name: bool,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub client_domain: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub isp: Option<String>,
    #[serde(default)]
    pub staff_jur: Option<String>,
    #[serde(default)]
    pub rules: Option<String>,
    #[serde(default)]
    pub privacy: Option<String>,
    #[serde(default)]
    pub using_vanilla_reg: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub reg_method: Option<String>,
    #[serde(default)]
    pub reg_link: Option<String>,
    #[serde(default)]
    pub reg_note: Option<String>,
    #[serde(default)]
    pub software: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub captcha: Option<bool>,
    #[serde(default)]
    pub captcha_note: Option<String>,
    #[serde(default)]
    pub email: Option<bool>,
    #[serde(default)]
    pub longstanding: Option<bool>,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub online_status: Option<i64>,
    #[serde(default)]
    pub server_domain: Option<String>,
    #[serde(default)]
    pub ver_status: Option<i64>,
    #[serde(default)]
    pub room_directory: Option<i64>,
    #[serde(default)]
    pub sliding_sync: Option<bool>,
    #[serde(default)]
    pub ipv6: Option<bool>,
    #[serde(default)]
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
