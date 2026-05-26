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

mod account;
mod settings;
mod shell;
mod utils;

use account::{
    AccountManager, AccountSummary, HomeserverDirectory, LoginRequest, RegisterAccountRequest,
    RegistrationOutcome,
};
use settings::{
    encryption::{
        create_recovery_key, delete_recovery, disable_server_key_storage,
        enable_server_key_storage, export_room_keys, get_encryption_overview, import_room_keys,
        recover_with_recovery_key, reset_crypto_identity, rotate_recovery_key,
        set_share_encrypted_history_on_invite, set_verified_devices_only,
    },
    sessions::{
        accept_sas_verification, accept_session_verification_request, cancel_sas_verification,
        confirm_sas_verification, deauthorize_sessions, deny_session_verification_request,
        get_sas_verification, get_session_overview, start_current_session_verification,
        start_sas_verification, start_session_verification,
    },
    theme::{
        get_theme_mode as load_theme_mode, get_theme_preset as load_theme_preset,
        set_theme_mode as save_theme_mode, set_theme_preset as save_theme_preset,
    },
};

use shell::{
    service::{
        ShellManager,
        discovery::types::{
            DiscoveryEntity, InviteTarget, InviteUserToRoomRequest, JoinDiscoveryRoomRequest,
            JoinDiscoveryRoomResponse, ListInviteTargetsRequest, SearchDiscoveryEntitiesRequest,
        },
    },
    types::{
        EditRoomMessageRequest, GetRoomEventContextRequest, GetRoomSummaryRequest,
        GetRoomTimelineRequest, GlobalSearchIndexStatus, GlobalSearchRequest, GlobalSearchResponse,
        ListRoomThreadsRequest, ListSpacesRequest, PaginateRoomTimelineRequest,
        RedactRoomMessageRequest, ReplyToRoomMessageRequest, ResolveRoomReplyPreviewRequest,
        RoomSummary, RoomThreadSummary, RoomTimeline, RoomTimelinePaginationResponse,
        RoomTimelineReplyPreview, SendRoomMessageRequest, SendRoomMessageResponse,
        SetRoomTypingRequest, SpaceSummary, ToggleRoomReactionRequest, ToggleRoomReactionResponse,
    },
};

use tauri::{AppHandle, RunEvent, State};

use tauri_plugin_dialog as dialog;
use tauri_plugin_fs as fs;

#[cfg(mobile)]
use tauri_plugin_app_events as app_events;
#[cfg(mobile)]
use tauri_plugin_mobile_webview_overlay as mobile_overlay_webview;

#[tauri::command]
async fn login_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: LoginRequest,
) -> Result<AccountSummary, String> {
    let account = manager.login(&app, request).await?;
    shell_manager
        .ensure_active_account_sync(&app, &manager)
        .await?;
    Ok(account)
}

#[tauri::command]
async fn list_accounts(
    app: AppHandle,
    manager: State<'_, AccountManager>,
) -> Result<Vec<AccountSummary>, String> {
    manager.list_accounts(&app).await
}

#[tauri::command]
async fn switch_active_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    account_key: String,
) -> Result<(), String> {
    manager.switch_active_account(&app, &account_key).await?;
    shell_manager
        .ensure_active_account_sync(&app, &manager)
        .await
}

#[tauri::command]
async fn active_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
) -> Result<Option<AccountSummary>, String> {
    manager.active_account(&app).await
}

#[tauri::command]
async fn sign_out_active_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
) -> Result<Option<AccountSummary>, String> {
    let active_account = manager.active_account(&app).await?;
    if let Some(account) = active_account {
        shell_manager.stop_account(&account.account_key).await;
    }

    let next_account = manager.sign_out_active_account(&app).await?;
    if next_account.is_some() {
        shell_manager
            .ensure_active_account_sync(&app, &manager)
            .await?;
    } else {
        shell_manager.stop_all_accounts().await;
    }

    Ok(next_account)
}

#[tauri::command]
async fn validate_active_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
) -> Result<Option<AccountSummary>, String> {
    manager.validate_active_account(&app).await
}

#[tauri::command]
async fn list_registration_homeservers(
    manager: State<'_, AccountManager>,
) -> Result<HomeserverDirectory, String> {
    manager.list_registration_homeservers().await
}

#[tauri::command]
async fn register_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
    request: RegisterAccountRequest,
) -> Result<RegistrationOutcome, String> {
    manager.register_account(&app, request).await
}

#[tauri::command]
async fn list_room_threads(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: Option<ListRoomThreadsRequest>,
) -> Result<Vec<RoomThreadSummary>, String> {
    let active_account = account_manager.require_active_account(&app).await?;
    shell_manager
        .list_room_threads(
            &app,
            &account_manager,
            &active_account,
            request.unwrap_or(ListRoomThreadsRequest { search_query: None }),
        )
        .await
}

#[tauri::command]
async fn get_room_summary(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: GetRoomSummaryRequest,
) -> Result<RoomSummary, String> {
    let active_account = account_manager.require_active_account(&app).await?;
    shell_manager
        .get_room_summary(&app, &account_manager, &active_account, request)
        .await
}

#[tauri::command]
async fn get_room_timeline(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: GetRoomTimelineRequest,
) -> Result<RoomTimeline, String> {
    let active_account = account_manager.require_active_account(&app).await?;
    shell_manager
        .get_room_timeline(&app, &account_manager, &active_account, request)
        .await
}

#[tauri::command]
async fn get_room_event_context(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: GetRoomEventContextRequest,
) -> Result<RoomTimeline, String> {
    let active_account = account_manager.require_active_account(&app).await?;
    shell_manager
        .get_room_event_context(&app, &account_manager, &active_account, request)
        .await
}

#[tauri::command]
async fn paginate_room_timeline_backwards(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: PaginateRoomTimelineRequest,
) -> Result<RoomTimelinePaginationResponse, String> {
    let active_account = account_manager.require_active_account(&app).await?;
    shell_manager
        .paginate_room_timeline_backwards(&app, &account_manager, &active_account, request)
        .await
}

#[tauri::command]
async fn resolve_room_reply_preview(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: ResolveRoomReplyPreviewRequest,
) -> Result<RoomTimelineReplyPreview, String> {
    let active_account = account_manager.require_active_account(&app).await?;
    shell_manager
        .resolve_room_reply_preview(&app, &account_manager, &active_account, request)
        .await
}

#[tauri::command]
async fn send_room_message(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: SendRoomMessageRequest,
) -> Result<SendRoomMessageResponse, String> {
    let active_account = account_manager.require_active_account(&app).await?;
    shell_manager
        .send_room_message(&app, &account_manager, &active_account, request)
        .await
}

#[tauri::command]
async fn edit_room_message(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: EditRoomMessageRequest,
) -> Result<(), String> {
    let active_account = account_manager.require_active_account(&app).await?;
    shell_manager
        .edit_room_message(&active_account, request)
        .await
}

#[tauri::command]
async fn redact_room_message(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: RedactRoomMessageRequest,
) -> Result<(), String> {
    let active_account = account_manager.require_active_account(&app).await?;
    shell_manager
        .redact_room_message(&active_account, request)
        .await
}

#[tauri::command]
async fn reply_to_room_message(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: ReplyToRoomMessageRequest,
) -> Result<(), String> {
    let active_account = account_manager.require_active_account(&app).await?;
    shell_manager
        .reply_to_room_message(&active_account, request)
        .await
}

#[tauri::command]
async fn toggle_room_reaction(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: ToggleRoomReactionRequest,
) -> Result<ToggleRoomReactionResponse, String> {
    let active_account = account_manager.require_active_account(&app).await?;
    shell_manager
        .toggle_room_reaction(&active_account, request)
        .await
}

#[tauri::command]
async fn set_room_typing(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: SetRoomTypingRequest,
) -> Result<(), String> {
    let active_account = account_manager.require_active_account(&app).await?;
    shell_manager
        .set_room_typing(&app, &active_account, request)
        .await
}

#[tauri::command]
async fn list_spaces(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: Option<ListSpacesRequest>,
) -> Result<Vec<SpaceSummary>, String> {
    let active_account = account_manager.require_active_account(&app).await?;
    shell_manager
        .list_spaces(
            &app,
            &account_manager,
            &active_account,
            request.unwrap_or(ListSpacesRequest { search_query: None }),
        )
        .await
}

#[tauri::command]
async fn global_search(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: GlobalSearchRequest,
) -> Result<GlobalSearchResponse, String> {
    let Some(active_account) = account_manager.optional_active_account(&app).await? else {
        return Ok(GlobalSearchResponse {
            rooms: Vec::new(),
            spaces: Vec::new(),
            messages: Vec::new(),
            status: GlobalSearchIndexStatus::default(),
        });
    };
    shell_manager
        .global_search(&app, &account_manager, &active_account, request)
        .await
}

#[tauri::command]
async fn search_discovery_entities(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: SearchDiscoveryEntitiesRequest,
) -> Result<Vec<DiscoveryEntity>, String> {
    let active_account = account_manager.require_active_account(&app).await?;
    shell_manager
        .search_discovery_entities(&active_account, request)
        .await
}

#[tauri::command]
async fn join_discovery_room(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: JoinDiscoveryRoomRequest,
) -> Result<JoinDiscoveryRoomResponse, String> {
    let active_account = account_manager.require_active_account(&app).await?;
    shell_manager
        .join_discovery_room(&app, &account_manager, &active_account, request)
        .await
}

#[tauri::command]
async fn invite_user_to_room(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: InviteUserToRoomRequest,
) -> Result<(), String> {
    let active_account = account_manager.require_active_account(&app).await?;
    shell_manager
        .invite_user_to_room(&active_account, request)
        .await
}

#[tauri::command]
async fn list_invite_targets(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    request: ListInviteTargetsRequest,
) -> Result<Vec<InviteTarget>, String> {
    let active_account = account_manager.require_active_account(&app).await?;
    ShellManager::list_invite_targets(&active_account, &request)
}

#[tauri::command]
#[cfg(mobile)]
async fn open_mobile_overlay_webview(
    app: AppHandle,
    url: String,
    title: Option<String>,
    user_agent: Option<String>,
) -> Result<(), String> {
    let resolved_user_agent = user_agent
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or(mobile_overlay_webview::default_desktop_user_agent());

    mobile_overlay_webview::open_url(&app, &url, title.as_deref(), Some(resolved_user_agent))
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn get_theme_preset(
    app: AppHandle,
    supported_presets: Vec<String>,
    default_preset: String,
) -> Result<String, String> {
    load_theme_preset(&app, &supported_presets, &default_preset)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn set_theme_preset(
    app: AppHandle,
    preset: String,
    supported_presets: Vec<String>,
    default_preset: String,
) -> Result<String, String> {
    save_theme_preset(&app, &preset, &supported_presets, &default_preset)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn get_theme_mode(app: AppHandle) -> Result<String, String> {
    load_theme_mode(&app)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn set_theme_mode(app: AppHandle, mode: String) -> Result<String, String> {
    save_theme_mode(&app, &mode)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Runs the Tauri application.
///
/// # Panics
///
/// Panics if Tauri fails to initialize or the application runtime exits with an
/// unrecoverable error.
pub fn run() {
    tauri::Builder::default()
        .manage(AccountManager::new())
        .manage(ShellManager::new())
        .plugin(dialog::init())
        .plugin(fs::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            #[cfg(mobile)]
            {
                app.handle().plugin(app_events::init())?;
                app.handle().plugin(mobile_overlay_webview::init())?;
            }
            #[cfg(not(mobile))]
            let _app_handle = app.handle();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            login_account,
            list_accounts,
            switch_active_account,
            active_account,
            sign_out_active_account,
            validate_active_account,
            list_registration_homeservers,
            register_account,
            list_room_threads,
            get_room_summary,
            get_room_timeline,
            paginate_room_timeline_backwards,
            get_room_event_context,
            resolve_room_reply_preview,
            send_room_message,
            edit_room_message,
            redact_room_message,
            reply_to_room_message,
            toggle_room_reaction,
            set_room_typing,
            list_spaces,
            global_search,
            search_discovery_entities,
            join_discovery_room,
            invite_user_to_room,
            list_invite_targets,
            #[cfg(mobile)]
            open_mobile_overlay_webview,
            get_encryption_overview,
            enable_server_key_storage,
            disable_server_key_storage,
            create_recovery_key,
            rotate_recovery_key,
            delete_recovery,
            recover_with_recovery_key,
            export_room_keys,
            import_room_keys,
            reset_crypto_identity,
            set_share_encrypted_history_on_invite,
            set_verified_devices_only,
            get_session_overview,
            start_session_verification,
            accept_session_verification_request,
            deny_session_verification_request,
            start_current_session_verification,
            start_sas_verification,
            accept_sas_verification,
            get_sas_verification,
            confirm_sas_verification,
            cancel_sas_verification,
            deauthorize_sessions,
            get_theme_mode,
            get_theme_preset,
            set_theme_mode,
            set_theme_preset
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            if matches!(event, RunEvent::Exit) {
                account::secure_storage::unset_default_store();
            }
        });
}
