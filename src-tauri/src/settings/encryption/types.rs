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

#[derive(Debug, Serialize)]
pub struct EncryptionOverview {
    pub has_active_account: bool,
    pub account_key: Option<String>,
    pub user_id: Option<String>,
    pub device_id: Option<String>,
    pub ed25519_key: Option<String>,
    pub curve25519_key: Option<String>,
    pub recovery_state: Option<String>,
    pub backup_state: Option<String>,
    pub server_key_storage_opted_out: bool,
    pub verified_devices_only: bool,
}

#[derive(Debug, Deserialize)]
pub struct RecoveryKeyRequest {
    pub recovery_key: String,
}

#[derive(Debug, Deserialize)]
pub struct RoomKeyFileRequest {
    pub passphrase: String,
}

#[derive(Debug, Serialize)]
pub struct GeneratedRecoveryKey {
    pub recovery_key: String,
}

#[derive(Debug, Serialize)]
pub struct RoomKeyImportSummary {
    pub imported_count: usize,
    pub total_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CryptoIdentityResetOutcome {
    Completed,
    UiaaRequired,
    OAuthRequired { approval_url: String },
}
