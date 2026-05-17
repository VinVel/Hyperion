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
    Client,
    encryption::{
        VerificationState as MatrixVerificationState,
        verification::{Verification, VerificationRequest, VerificationRequestState},
    },
    ruma::{
        OwnedDeviceId, OwnedUserId,
        api::client::{
            device::delete_device,
            discovery::get_authorization_server_metadata::v1::{
                AccountManagementActionData, DeviceDeleteData,
            },
            uiaa::{self, AuthData},
        },
        events::key::verification::{
            VerificationMethod, request::ToDeviceKeyVerificationRequestEvent,
            start::ToDeviceKeyVerificationStartEvent,
        },
    },
};
use tauri::{AppHandle, Emitter};

use crate::{
    account::{AccountClientSnapshot, AccountManager},
    settings::account as account_settings,
    utils::time::now_unix_ms,
};

pub use super::types::{
    DeauthorizeSessionsOutcome, DeauthorizeSessionsRequest, IncomingSessionVerification, SasEmoji,
    SasVerificationView, SessionInfo, SessionOverview, SessionTrustInfo,
    StartSessionVerificationRequest, VerificationFlowRequest, VerificationStart, VerificationState,
};

pub const SESSION_OVERVIEW_UPDATED_EVENT: &str = "hyperion://session-overview-updated";
pub const SESSION_VERIFICATION_REQUEST_RECEIVED_EVENT: &str =
    "hyperion://session-verification-request-received";

pub fn register_session_verification_event_handler(
    app: &AppHandle,
    account: &AccountClientSnapshot,
) {
    let request_event_app = app.clone();
    let account_key = account.account_key.clone();
    account
        .client
        .add_event_handler(move |event: ToDeviceKeyVerificationRequestEvent| {
            let app = request_event_app.clone();
            let account_key = account_key.clone();
            async move {
                let flow_id = event.content.transaction_id.to_string();
                let payload = IncomingSessionVerification {
                    account_key,
                    flow_id,
                    device_id: event.content.from_device.to_string(),
                    event_kind: String::from("request"),
                    supported_methods: event
                        .content
                        .methods
                        .into_iter()
                        .map(|method| method.to_string())
                        .collect(),
                };

                if let Err(error) = app.emit(SESSION_VERIFICATION_REQUEST_RECEIVED_EVENT, payload) {
                    eprintln!("Failed to emit incoming verification request: {error}");
                }
            }
        });
    let start_event_app = app.clone();
    let account_key = account.account_key.clone();
    account
        .client
        .add_event_handler(move |event: ToDeviceKeyVerificationStartEvent| {
            let app = start_event_app.clone();
            let account_key = account_key.clone();
            async move {
                let payload = IncomingSessionVerification {
                    account_key,
                    flow_id: event.content.transaction_id.to_string(),
                    device_id: event.content.from_device.to_string(),
                    event_kind: String::from("start"),
                    supported_methods: Vec::new(),
                };

                if let Err(error) = app.emit(SESSION_VERIFICATION_REQUEST_RECEIVED_EVENT, payload) {
                    eprintln!("Failed to emit incoming verification start: {error}");
                }
            }
        });
}

#[tauri::command]
pub async fn get_session_overview(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
) -> Result<SessionOverview, String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Ok(SessionOverview {
            has_active_account: false,
            account_key: None,
            user_id: None,
            current_device_id: None,
            current_session_verified: false,
            sessions: Vec::new(),
            last_refreshed_at_unix_ms: None,
        });
    };

    let cached_overview = account_settings::read_session_overview(&account.store_dir)?;
    schedule_session_overview_refresh(app, account.clone());
    if let Some(overview) = cached_overview {
        return Ok(overview);
    }

    Ok(SessionOverview {
        has_active_account: true,
        account_key: Some(account.account_key),
        user_id: account.client.user_id().map(ToString::to_string),
        current_device_id: account.client.device_id().map(ToString::to_string),
        current_session_verified: false,
        sessions: Vec::new(),
        last_refreshed_at_unix_ms: None,
    })
}

#[tauri::command]
pub async fn start_session_verification(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
    request: StartSessionVerificationRequest,
) -> Result<VerificationStart, String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };
    let user_id = active_user_id(&account)?;
    let device_id = parse_device_id(&request.device_id)?;
    if Some(device_id.as_ref()) == account.client.device_id() {
        return Err(String::from(
            "This session cannot verify itself. Open another verified session to verify this one.",
        ));
    }

    let encryption = account.client.encryption();
    let Some(device) = encryption
        .get_device(&user_id, &device_id)
        .await
        .map_err(|error| format!("Failed to read device verification state: {error}"))?
    else {
        return Err(String::from(
            "The selected session is not available for verification",
        ));
    };

    let verification = device
        .request_verification_with_methods(sas_verification_methods())
        .await
        .map_err(|error| format!("Failed to request session verification: {error}"))?;
    let state = verification_request_state(verification.flow_id(), verification.state());
    let supported_methods = verification
        .their_supported_methods()
        .unwrap_or_default()
        .into_iter()
        .map(|method| method.to_string())
        .collect();

    Ok(VerificationStart {
        flow_id: verification.flow_id().to_owned(),
        device_id: request.device_id,
        supported_methods,
        state,
    })
}

#[tauri::command]
pub async fn start_current_session_verification(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
) -> Result<VerificationStart, String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };
    let user_id = active_user_id(&account)?;
    let encryption = account.client.encryption();
    let has_devices_to_verify_against = encryption
        .has_devices_to_verify_against()
        .await
        .map_err(|error| format!("Failed to check verified sessions: {error}"))?;
    if !has_devices_to_verify_against {
        return Err(String::from(
            "No verified session is available to verify this session.",
        ));
    }

    let Some(identity) = encryption
        .request_user_identity(&user_id)
        .await
        .map_err(|error| format!("Failed to read account identity: {error}"))?
    else {
        return Err(String::from(
            "The account identity is not available for verification.",
        ));
    };
    let verification = identity
        .request_verification_with_methods(sas_verification_methods())
        .await
        .map_err(|error| format!("Failed to request current-session verification: {error}"))?;
    let state = verification_request_state(verification.flow_id(), verification.state());
    let supported_methods = verification
        .their_supported_methods()
        .unwrap_or_default()
        .into_iter()
        .map(|method| method.to_string())
        .collect();

    Ok(VerificationStart {
        flow_id: verification.flow_id().to_owned(),
        device_id: account
            .client
            .device_id()
            .map(ToString::to_string)
            .unwrap_or_default(),
        supported_methods,
        state,
    })
}

#[tauri::command]
pub async fn accept_session_verification_request(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
    request: VerificationFlowRequest,
) -> Result<VerificationState, String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };
    let verification_request = active_verification_request(&account, &request.flow_id).await?;
    accept_verification_request_if_pending(&verification_request).await;

    Ok(verification_request_state(
        verification_request.flow_id(),
        verification_request.state(),
    ))
}

#[tauri::command]
pub async fn deny_session_verification_request(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
    request: VerificationFlowRequest,
) -> Result<(), String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };
    let verification_request = active_verification_request(&account, &request.flow_id).await?;
    verification_request
        .cancel()
        .await
        .map_err(|error| format!("Failed to deny verification request: {error}"))
}

#[tauri::command]
pub async fn start_sas_verification(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
    request: VerificationFlowRequest,
) -> Result<SasVerificationView, String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };
    let verification_request = active_verification_request(&account, &request.flow_id).await?;
    advance_sas_verification(&verification_request).await
}

#[tauri::command]
pub async fn accept_sas_verification(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
    request: VerificationFlowRequest,
) -> Result<SasVerificationView, String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };
    let sas = get_sas_for_flow_raw(&account, &request.flow_id).await?;
    sas.accept()
        .await
        .map_err(|error| format!("Failed to accept emoji verification: {error}"))?;

    Ok(sas_view(&request.flow_id, &sas))
}

#[tauri::command]
pub async fn get_sas_verification(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
    request: VerificationFlowRequest,
) -> Result<SasVerificationView, String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };
    let verification_request = active_verification_request(&account, &request.flow_id).await?;
    advance_sas_verification(&verification_request).await
}

#[tauri::command]
pub async fn confirm_sas_verification(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
    request: VerificationFlowRequest,
) -> Result<SasVerificationView, String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };
    let sas = get_sas_for_flow_raw(&account, &request.flow_id).await?;
    sas.confirm()
        .await
        .map_err(|error| format!("Failed to confirm emoji verification: {error}"))?;

    Ok(sas_view(&request.flow_id, &sas))
}

#[tauri::command]
pub async fn cancel_sas_verification(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
    request: VerificationFlowRequest,
) -> Result<SasVerificationView, String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };
    let sas = get_sas_for_flow_raw(&account, &request.flow_id).await?;
    sas.mismatch()
        .await
        .map_err(|error| format!("Failed to cancel emoji verification: {error}"))?;

    Ok(sas_view(&request.flow_id, &sas))
}

#[tauri::command]
pub async fn deauthorize_sessions(
    app: AppHandle,
    account_manager: tauri::State<'_, AccountManager>,
    request: DeauthorizeSessionsRequest,
) -> Result<DeauthorizeSessionsOutcome, String> {
    let Some(account) = account_manager.active_account_client(&app).await? else {
        return Err(String::from("No active account is available"));
    };
    let current_device_id = account.client.device_id().map(ToOwned::to_owned);
    let device_ids =
        parse_deauthorization_device_ids(&request.device_ids, current_device_id.as_ref())?;
    let auth_data = password_auth_data(
        &account,
        request.auth_session.as_ref(),
        request.password.as_deref(),
    )?;

    match account
        .client
        .delete_devices(&device_ids, auth_data.clone())
        .await
    {
        Ok(_response) => Ok(DeauthorizeSessionsOutcome::Completed),
        Err(error) => {
            if let Some(uiaa_response) = error.as_uiaa_response() {
                return Ok(DeauthorizeSessionsOutcome::PasswordRequired {
                    auth_session: uiaa_response.session.clone(),
                });
            }

            if is_unrecognized_endpoint_error(&error.to_string()) {
                if let Some(account_management_url) =
                    account_management_url_for_first_device(&account, &device_ids).await
                {
                    return Ok(DeauthorizeSessionsOutcome::AccountManagementRequired {
                        account_management_url,
                    });
                }

                return deauthorize_sessions_individually(
                    &account,
                    &device_ids,
                    request.password.as_deref(),
                )
                .await;
            }

            Err(format!("Failed to deauthorize sessions: {error}"))
        }
    }
}

async fn deauthorize_sessions_individually(
    account: &AccountClientSnapshot,
    device_ids: &[OwnedDeviceId],
    password: Option<&str>,
) -> Result<DeauthorizeSessionsOutcome, String> {
    for device_id in device_ids {
        match delete_single_device(&account.client, device_id, None).await {
            Ok(_response) => {}
            Err(error) => {
                let Some(uiaa_response) = error.as_uiaa_response() else {
                    if is_unrecognized_endpoint_error(&error.to_string())
                        && let Some(account_management_url) =
                            account_management_url_for_first_device(account, device_ids).await
                    {
                        return Ok(DeauthorizeSessionsOutcome::AccountManagementRequired {
                            account_management_url,
                        });
                    }

                    return Err(format!(
                        "Failed to deauthorize session {device_id}: {error}"
                    ));
                };
                let Some(password) = password else {
                    return Ok(DeauthorizeSessionsOutcome::PasswordRequired {
                        auth_session: uiaa_response.session.clone(),
                    });
                };
                let auth_data =
                    password_auth_data(account, uiaa_response.session.as_ref(), Some(password))?;
                delete_single_device(&account.client, device_id, auth_data)
                    .await
                    .map_err(|retry_error| {
                        format!("Failed to deauthorize session {device_id}: {retry_error}")
                    })?;
            }
        }
    }

    Ok(DeauthorizeSessionsOutcome::Completed)
}

async fn account_management_url_for_first_device(
    account: &AccountClientSnapshot,
    device_ids: &[OwnedDeviceId],
) -> Option<String> {
    let action = account_management_action_for_device_ids(device_ids)?;
    let metadata = account.client.oauth().cached_server_metadata().await.ok()?;

    metadata
        .account_management_url_with_action(action)
        .map(|url| url.to_string())
}

fn account_management_action_for_device_ids(
    device_ids: &[OwnedDeviceId],
) -> Option<AccountManagementActionData<'_>> {
    if device_ids.len() == 1 {
        return device_ids.first().map(|device_id| {
            AccountManagementActionData::DeviceDelete(DeviceDeleteData::new(device_id))
        });
    }

    if device_ids.is_empty() {
        None
    } else {
        Some(AccountManagementActionData::DevicesList)
    }
}

async fn delete_single_device(
    client: &Client,
    device_id: &OwnedDeviceId,
    auth_data: Option<AuthData>,
) -> matrix_sdk::HttpResult<delete_device::v3::Response> {
    let mut request = delete_device::v3::Request::new(device_id.clone());
    request.auth = auth_data;
    client.send(request).await
}

async fn refreshed_session_overview(
    account: &AccountClientSnapshot,
) -> Result<SessionOverview, String> {
    let user_id = active_user_id(account)?;
    let current_device_id = account.client.device_id().map(ToString::to_string);
    let _devices_to_verify = account
        .client
        .encryption()
        .has_devices_to_verify_against()
        .await;
    let current_session_verified = matches!(
        account.client.encryption().verification_state().get(),
        MatrixVerificationState::Verified
    );
    let device_response = account
        .client
        .devices()
        .await
        .map_err(|error| format!("Failed to read account sessions: {error}"))?;
    let crypto_devices = account
        .client
        .encryption()
        .get_user_devices(&user_id)
        .await
        .ok();
    let mut sessions = Vec::with_capacity(device_response.devices.len());

    for device in device_response.devices {
        let device_id = device.device_id.to_string();
        let crypto_device = crypto_devices
            .as_ref()
            .and_then(|devices| devices.get(&device.device_id));
        let verified_with_cross_signing = crypto_device.as_ref().is_some_and(
            matrix_sdk::encryption::identities::Device::is_verified_with_cross_signing,
        );
        let signed_by_owner = crypto_device
            .as_ref()
            .is_some_and(matrix_sdk::encryption::identities::Device::is_cross_signed_by_owner);
        let is_current = current_device_id.as_deref() == Some(device_id.as_str());
        let verified = if is_current {
            current_session_verified
        } else {
            signed_by_owner || verified_with_cross_signing
        };

        sessions.push(SessionInfo {
            current: is_current,
            device_id,
            display_name: device.display_name,
            trust: SessionTrustInfo {
                verified,
                verified_with_cross_signing,
                can_verify_current_session: !is_current && signed_by_owner,
            },
            last_seen_ip: device.last_seen_ip,
            last_seen_ts_unix_ms: device.last_seen_ts.map(|timestamp| timestamp.0.into()),
        });
    }

    sessions.sort_by(|left, right| {
        right
            .current
            .cmp(&left.current)
            .then_with(|| right.last_seen_ts_unix_ms.cmp(&left.last_seen_ts_unix_ms))
            .then_with(|| left.device_id.cmp(&right.device_id))
    });

    Ok(SessionOverview {
        has_active_account: true,
        account_key: Some(account.account_key.clone()),
        user_id: Some(user_id.to_string()),
        current_device_id,
        current_session_verified,
        sessions,
        last_refreshed_at_unix_ms: Some(now_unix_ms()),
    })
}

fn schedule_session_overview_refresh(app: AppHandle, account: AccountClientSnapshot) {
    tauri::async_runtime::spawn(async move {
        let overview = match refreshed_session_overview(&account).await {
            Ok(overview) => overview,
            Err(error) => {
                eprintln!("Failed to refresh session overview in background: {error}");
                return;
            }
        };

        if let Err(error) = account_settings::write_session_overview(&account.store_dir, &overview)
        {
            eprintln!("Failed to persist refreshed session overview: {error}");
        }

        if let Err(error) = app.emit(SESSION_OVERVIEW_UPDATED_EVENT, overview) {
            eprintln!("Failed to emit session overview update: {error}");
        }
    });
}

async fn active_verification_request(
    account: &AccountClientSnapshot,
    flow_id: &str,
) -> Result<matrix_sdk::encryption::verification::VerificationRequest, String> {
    let user_id = active_user_id(account)?;
    account
        .client
        .encryption()
        .get_verification_request(&user_id, flow_id)
        .await
        .ok_or_else(|| String::from("The verification request is no longer available"))
}

async fn get_sas_for_flow_raw(
    account: &AccountClientSnapshot,
    flow_id: &str,
) -> Result<matrix_sdk::encryption::verification::SasVerification, String> {
    let user_id = active_user_id(account)?;
    account
        .client
        .encryption()
        .get_verification(&user_id, flow_id)
        .await
        .and_then(matrix_sdk::encryption::verification::Verification::sas)
        .ok_or_else(|| String::from("The emoji verification flow is not available yet"))
}

async fn accept_verification_request_if_pending(verification_request: &VerificationRequest) {
    if !matches!(
        verification_request.state(),
        VerificationRequestState::Requested { .. }
    ) {
        return;
    }

    if let Err(error) = verification_request
        .accept_with_methods(sas_verification_methods())
        .await
    {
        eprintln!(
            "Failed to accept incoming verification request {}: {error}",
            verification_request.flow_id()
        );
    }
}

fn sas_verification_methods() -> Vec<VerificationMethod> {
    vec![VerificationMethod::SasV1]
}

async fn advance_sas_verification(
    verification_request: &VerificationRequest,
) -> Result<SasVerificationView, String> {
    let flow_id = verification_request.flow_id().to_owned();
    match verification_request.state() {
        VerificationRequestState::Requested { .. } => {
            verification_request
                .accept_with_methods(sas_verification_methods())
                .await
                .map_err(|error| format!("Failed to accept verification request: {error}"))?;
            Ok(pending_sas_view(&flow_id, "Accepted"))
        }
        VerificationRequestState::Ready { .. } => {
            let Some(sas) = verification_request
                .start_sas()
                .await
                .map_err(|error| format!("Failed to start emoji verification: {error}"))?
            else {
                return Ok(pending_sas_view(&flow_id, "Waiting for emoji verification"));
            };

            Ok(sas_view(&flow_id, &sas))
        }
        VerificationRequestState::Transitioned {
            verification: Verification::SasV1(sas),
        } => {
            if !sas.is_done() && !sas.is_cancelled() && !sas.can_be_presented() {
                sas.accept()
                    .await
                    .map_err(|error| format!("Failed to accept emoji verification: {error}"))?;
            }

            Ok(sas_view(&flow_id, &sas))
        }
        VerificationRequestState::Transitioned { .. } => Ok(pending_sas_view(
            &flow_id,
            "Using another verification method",
        )),
        VerificationRequestState::Done => Ok(done_sas_view(&flow_id)),
        VerificationRequestState::Cancelled(cancel_info) => Ok(cancelled_sas_view(
            &flow_id,
            Some(cancel_info.reason().to_owned()),
        )),
        VerificationRequestState::Created { .. } => {
            Ok(pending_sas_view(&flow_id, "Waiting for other session"))
        }
    }
}

fn sas_view(
    flow_id: &str,
    sas: &matrix_sdk::encryption::verification::SasVerification,
) -> SasVerificationView {
    let emojis = sas
        .emoji()
        .map(|emoji_set| {
            emoji_set
                .into_iter()
                .map(|emoji| SasEmoji {
                    symbol: emoji.symbol.to_owned(),
                    description: emoji.description.to_owned(),
                })
                .collect()
        })
        .unwrap_or_default();
    let decimals = sas
        .decimals()
        .map(|(first, second, third)| [first, second, third]);
    let cancel_reason = sas.cancel_info().map(|info| info.reason().to_owned());

    SasVerificationView {
        flow_id: flow_id.to_owned(),
        label: sas_state_label(sas),
        done: sas.is_done(),
        cancelled: sas.is_cancelled(),
        cancel_reason,
        emojis,
        decimals,
        can_be_presented: sas.can_be_presented(),
    }
}

fn pending_sas_view(flow_id: &str, label: &str) -> SasVerificationView {
    SasVerificationView {
        flow_id: flow_id.to_owned(),
        label: label.to_owned(),
        done: false,
        cancelled: false,
        cancel_reason: None,
        emojis: Vec::new(),
        decimals: None,
        can_be_presented: false,
    }
}

fn done_sas_view(flow_id: &str) -> SasVerificationView {
    SasVerificationView {
        flow_id: flow_id.to_owned(),
        label: String::from("Done"),
        done: true,
        cancelled: false,
        cancel_reason: None,
        emojis: Vec::new(),
        decimals: None,
        can_be_presented: false,
    }
}

fn cancelled_sas_view(flow_id: &str, cancel_reason: Option<String>) -> SasVerificationView {
    SasVerificationView {
        flow_id: flow_id.to_owned(),
        label: String::from("Cancelled"),
        done: false,
        cancelled: true,
        cancel_reason,
        emojis: Vec::new(),
        decimals: None,
        can_be_presented: false,
    }
}

fn sas_state_label(sas: &matrix_sdk::encryption::verification::SasVerification) -> String {
    if sas.is_done() {
        return String::from("Done");
    }

    if sas.is_cancelled() {
        return String::from("Cancelled");
    }

    if sas.can_be_presented() {
        return String::from("Compare emojis");
    }

    String::from("In progress")
}

fn active_user_id(account: &AccountClientSnapshot) -> Result<OwnedUserId, String> {
    account
        .client
        .user_id()
        .map(ToOwned::to_owned)
        .ok_or_else(|| String::from("The active account user ID is not available"))
}

fn parse_device_id(device_id: &str) -> Result<OwnedDeviceId, String> {
    let device_id = device_id.trim();
    if device_id.is_empty() {
        return Err(String::from("Device ID must not be empty"));
    }

    Ok(device_id.into())
}

fn parse_deauthorization_device_ids(
    device_ids: &[String],
    current_device_id: Option<&OwnedDeviceId>,
) -> Result<Vec<OwnedDeviceId>, String> {
    if device_ids.is_empty() {
        return Err(String::from("Select at least one session to deauthorize"));
    }

    let mut parsed = Vec::with_capacity(device_ids.len());
    for device_id in device_ids {
        let parsed_device_id = parse_device_id(device_id)?;
        if current_device_id == Some(&parsed_device_id) {
            return Err(String::from(
                "The current session cannot deauthorize itself from this view",
            ));
        }
        parsed.push(parsed_device_id);
    }

    Ok(parsed)
}

fn password_auth_data(
    account: &AccountClientSnapshot,
    auth_session: Option<&String>,
    password: Option<&str>,
) -> Result<Option<AuthData>, String> {
    let Some(password) = password.map(str::trim) else {
        return Ok(None);
    };
    if password.is_empty() {
        return Ok(None);
    }

    let user_id = active_user_id(account)?;
    let mut password_auth = uiaa::Password::new(
        uiaa::UserIdentifier::Matrix(uiaa::MatrixUserIdentifier::new(user_id.to_string())),
        password.to_owned(),
    );
    password_auth.session = auth_session.cloned();
    Ok(Some(AuthData::Password(password_auth)))
}

fn is_unrecognized_endpoint_error(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("m_unrecognized") || message.contains("unrecognized request")
}

fn verification_request_state(flow_id: &str, state: VerificationRequestState) -> VerificationState {
    match state {
        VerificationRequestState::Done => VerificationState {
            flow_id: flow_id.to_owned(),
            label: String::from("Done"),
            done: true,
            cancelled: false,
            cancel_reason: None,
        },
        VerificationRequestState::Cancelled(cancel_info) => VerificationState {
            flow_id: flow_id.to_owned(),
            label: String::from("Cancelled"),
            done: false,
            cancelled: true,
            cancel_reason: Some(cancel_info.reason().to_owned()),
        },
        VerificationRequestState::Created { .. } => pending_state(flow_id, "Created"),
        VerificationRequestState::Requested { .. } => pending_state(flow_id, "Requested"),
        VerificationRequestState::Ready { .. } => pending_state(flow_id, "Ready"),
        VerificationRequestState::Transitioned { .. } => pending_state(flow_id, "In progress"),
    }
}

fn pending_state(flow_id: &str, label: &str) -> VerificationState {
    VerificationState {
        flow_id: flow_id.to_owned(),
        label: label.to_owned(),
        done: false,
        cancelled: false,
        cancel_reason: None,
    }
}
