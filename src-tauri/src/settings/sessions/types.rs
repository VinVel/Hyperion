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

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionOverview {
    pub has_active_account: bool,
    pub account_key: Option<String>,
    pub user_id: Option<String>,
    pub current_device_id: Option<String>,
    pub current_session_verified: bool,
    #[serde(default)]
    pub sessions: Vec<SessionInfo>,
    #[serde(default)]
    pub last_refreshed_at_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionInfo {
    pub device_id: String,
    pub display_name: Option<String>,
    #[serde(default)]
    pub trust: SessionTrustInfo,
    pub current: bool,
    pub last_seen_ip: Option<String>,
    pub last_seen_ts_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionTrustInfo {
    pub verified: bool,
    pub verified_with_cross_signing: bool,
    #[serde(default)]
    pub can_verify_current_session: bool,
}

#[derive(Debug, Deserialize)]
pub struct StartSessionVerificationRequest {
    pub device_id: String,
}

#[derive(Debug, Serialize)]
pub struct VerificationStart {
    pub flow_id: String,
    pub device_id: String,
    #[serde(default)]
    pub supported_methods: Vec<String>,
    pub state: VerificationState,
}

#[derive(Debug, Clone, Serialize)]
pub struct IncomingSessionVerification {
    pub account_key: String,
    pub flow_id: String,
    pub device_id: String,
    pub event_kind: String,
    #[serde(default)]
    pub supported_methods: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct VerificationFlowRequest {
    pub flow_id: String,
}

#[derive(Debug, Serialize)]
pub struct VerificationState {
    pub flow_id: String,
    pub label: String,
    pub done: bool,
    pub cancelled: bool,
    pub cancel_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SasVerificationView {
    pub flow_id: String,
    pub label: String,
    pub done: bool,
    pub cancelled: bool,
    pub cancel_reason: Option<String>,
    #[serde(default)]
    pub emojis: Vec<SasEmoji>,
    pub decimals: Option<[u16; 3]>,
    pub can_be_presented: bool,
}

#[derive(Debug, Serialize)]
pub struct SasEmoji {
    pub symbol: String,
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct DeauthorizeSessionsRequest {
    pub device_ids: Vec<String>,
    pub password: Option<String>,
    pub auth_session: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeauthorizeSessionsOutcome {
    Completed,
    PasswordRequired { auth_session: Option<String> },
    AccountManagementRequired { account_management_url: String },
}
