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
mod host;
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

use crate::host::{AppHandle, State};

async fn login_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: LoginRequest,
) -> Result<AccountSummary, String> {
    utils::tracing::report_command_future("login_account", "account", async {
        let account = manager.login(&app, request).await?;
        shell_manager
            .ensure_active_account_sync(&app, &manager)
            .await?;
        Ok(account)
    })
    .await
}

async fn list_accounts(
    app: AppHandle,
    manager: State<'_, AccountManager>,
) -> Result<Vec<AccountSummary>, String> {
    utils::tracing::report_command_future("list_accounts", "account", manager.list_accounts(&app))
        .await
}

async fn switch_active_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    account_key: String,
) -> Result<(), String> {
    utils::tracing::report_command_future("switch_active_account", "account", async {
        manager.switch_active_account(&app, &account_key).await?;
        shell_manager
            .ensure_active_account_sync(&app, &manager)
            .await
    })
    .await
}

async fn active_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
) -> Result<Option<AccountSummary>, String> {
    utils::tracing::report_command_future("active_account", "account", manager.active_account(&app))
        .await
}

async fn sign_out_active_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
) -> Result<Option<AccountSummary>, String> {
    utils::tracing::report_command_future("sign_out_active_account", "account", async {
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
    })
    .await
}

async fn validate_active_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
) -> Result<Option<AccountSummary>, String> {
    utils::tracing::report_command_future(
        "validate_active_account",
        "account",
        manager.validate_active_account(&app),
    )
    .await
}

async fn list_registration_homeservers(
    manager: State<'_, AccountManager>,
) -> Result<HomeserverDirectory, String> {
    utils::tracing::report_command_future(
        "list_registration_homeservers",
        "account",
        manager.list_registration_homeservers(),
    )
    .await
}

async fn register_account(
    app: AppHandle,
    manager: State<'_, AccountManager>,
    request: RegisterAccountRequest,
) -> Result<RegistrationOutcome, String> {
    utils::tracing::report_command_future(
        "register_account",
        "account",
        Box::pin(manager.register_account(&app, request)),
    )
    .await
}

async fn list_room_threads(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: Option<ListRoomThreadsRequest>,
) -> Result<Vec<RoomThreadSummary>, String> {
    utils::tracing::report_command_future("list_room_threads", "shell.room", async {
        let active_account = account_manager.require_active_account(&app).await?;
        shell_manager
            .list_room_threads(
                &app,
                &account_manager,
                &active_account,
                request.unwrap_or(ListRoomThreadsRequest { search_query: None }),
            )
            .await
    })
    .await
}

async fn get_room_summary(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: GetRoomSummaryRequest,
) -> Result<RoomSummary, String> {
    utils::tracing::report_command_future("get_room_summary", "shell.room", async {
        let active_account = account_manager.require_active_account(&app).await?;
        shell_manager
            .get_room_summary(&app, &account_manager, &active_account, request)
            .await
    })
    .await
}

async fn get_room_timeline(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: GetRoomTimelineRequest,
) -> Result<RoomTimeline, String> {
    utils::tracing::report_command_future(
        "get_room_timeline",
        "shell.timeline",
        Box::pin(async {
            let active_account = account_manager.require_active_account(&app).await?;
            shell_manager
                .get_room_timeline(&app, &account_manager, &active_account, request)
                .await
        }),
    )
    .await
}

async fn get_room_event_context(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: GetRoomEventContextRequest,
) -> Result<RoomTimeline, String> {
    utils::tracing::report_command_future(
        "get_room_event_context",
        "shell.timeline",
        Box::pin(async {
            let active_account = account_manager.require_active_account(&app).await?;
            shell_manager
                .get_room_event_context(&app, &account_manager, &active_account, request)
                .await
        }),
    )
    .await
}

async fn paginate_room_timeline_backwards(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: PaginateRoomTimelineRequest,
) -> Result<RoomTimelinePaginationResponse, String> {
    utils::tracing::report_command_future(
        "paginate_room_timeline_backwards",
        "shell.timeline",
        Box::pin(async {
            let active_account = account_manager.require_active_account(&app).await?;
            shell_manager
                .paginate_room_timeline_backwards(&app, &account_manager, &active_account, request)
                .await
        }),
    )
    .await
}

async fn resolve_room_reply_preview(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: ResolveRoomReplyPreviewRequest,
) -> Result<RoomTimelineReplyPreview, String> {
    utils::tracing::report_command_future(
        "resolve_room_reply_preview",
        "shell.timeline",
        Box::pin(async {
            let active_account = account_manager.require_active_account(&app).await?;
            shell_manager
                .resolve_room_reply_preview(&app, &account_manager, &active_account, request)
                .await
        }),
    )
    .await
}

async fn send_room_message(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: SendRoomMessageRequest,
) -> Result<SendRoomMessageResponse, String> {
    utils::tracing::report_command_future(
        "send_room_message",
        "shell.room",
        Box::pin(async {
            let active_account = account_manager.require_active_account(&app).await?;
            shell_manager
                .send_room_message(&app, &account_manager, &active_account, request)
                .await
        }),
    )
    .await
}

async fn edit_room_message(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: EditRoomMessageRequest,
) -> Result<(), String> {
    utils::tracing::report_command_future(
        "edit_room_message",
        "shell.room",
        Box::pin(async {
            let active_account = account_manager.require_active_account(&app).await?;
            shell_manager
                .edit_room_message(&active_account, request)
                .await
        }),
    )
    .await
}

async fn redact_room_message(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: RedactRoomMessageRequest,
) -> Result<(), String> {
    utils::tracing::report_command_future(
        "redact_room_message",
        "shell.room",
        Box::pin(async {
            let active_account = account_manager.require_active_account(&app).await?;
            shell_manager
                .redact_room_message(&active_account, request)
                .await
        }),
    )
    .await
}

async fn reply_to_room_message(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: ReplyToRoomMessageRequest,
) -> Result<(), String> {
    utils::tracing::report_command_future(
        "reply_to_room_message",
        "shell.room",
        Box::pin(async {
            let active_account = account_manager.require_active_account(&app).await?;
            shell_manager
                .reply_to_room_message(&active_account, request)
                .await
        }),
    )
    .await
}

async fn toggle_room_reaction(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: ToggleRoomReactionRequest,
) -> Result<ToggleRoomReactionResponse, String> {
    utils::tracing::report_command_future(
        "toggle_room_reaction",
        "shell.room",
        Box::pin(async {
            let active_account = account_manager.require_active_account(&app).await?;
            shell_manager
                .toggle_room_reaction(&active_account, request)
                .await
        }),
    )
    .await
}

async fn set_room_typing(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: SetRoomTypingRequest,
) -> Result<(), String> {
    utils::tracing::report_command_future("set_room_typing", "shell.room", async {
        let active_account = account_manager.require_active_account(&app).await?;
        shell_manager
            .set_room_typing(&app, &active_account, request)
            .await
    })
    .await
}

async fn list_spaces(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: Option<ListSpacesRequest>,
) -> Result<Vec<SpaceSummary>, String> {
    utils::tracing::report_command_future("list_spaces", "shell.room", async {
        let active_account = account_manager.require_active_account(&app).await?;
        shell_manager
            .list_spaces(
                &app,
                &account_manager,
                &active_account,
                request.unwrap_or(ListSpacesRequest { search_query: None }),
            )
            .await
    })
    .await
}

async fn global_search(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: GlobalSearchRequest,
) -> Result<GlobalSearchResponse, String> {
    utils::tracing::report_command_future("global_search", "shell.search", async {
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
    })
    .await
}

async fn search_discovery_entities(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: SearchDiscoveryEntitiesRequest,
) -> Result<Vec<DiscoveryEntity>, String> {
    utils::tracing::report_command_future("search_discovery_entities", "shell.discovery", async {
        let active_account = account_manager.require_active_account(&app).await?;
        shell_manager
            .search_discovery_entities(&active_account, request)
            .await
    })
    .await
}

async fn join_discovery_room(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: JoinDiscoveryRoomRequest,
) -> Result<JoinDiscoveryRoomResponse, String> {
    utils::tracing::report_command_future("join_discovery_room", "shell.discovery", async {
        let active_account = account_manager.require_active_account(&app).await?;
        shell_manager
            .join_discovery_room(&app, &account_manager, &active_account, request)
            .await
    })
    .await
}

async fn invite_user_to_room(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    shell_manager: State<'_, ShellManager>,
    request: InviteUserToRoomRequest,
) -> Result<(), String> {
    utils::tracing::report_command_future("invite_user_to_room", "shell.discovery", async {
        let active_account = account_manager.require_active_account(&app).await?;
        shell_manager
            .invite_user_to_room(&active_account, request)
            .await
    })
    .await
}

async fn list_invite_targets(
    app: AppHandle,
    account_manager: State<'_, AccountManager>,
    request: ListInviteTargetsRequest,
) -> Result<Vec<InviteTarget>, String> {
    utils::tracing::report_command_future("list_invite_targets", "shell.discovery", async {
        let active_account = account_manager.require_active_account(&app).await?;
        ShellManager::list_invite_targets(&active_account, &request)
    })
    .await
}

#[allow(clippy::needless_pass_by_value)]
fn get_theme_preset(
    app: AppHandle,
    supported_presets: Vec<String>,
    default_preset: String,
) -> Result<String, String> {
    utils::tracing::report_command_result(
        "get_theme_preset",
        "settings.theme",
        load_theme_preset(&app, &supported_presets, &default_preset),
    )
}

#[allow(clippy::needless_pass_by_value)]
fn set_theme_preset(
    app: AppHandle,
    preset: String,
    supported_presets: Vec<String>,
    default_preset: String,
) -> Result<String, String> {
    utils::tracing::report_command_result(
        "set_theme_preset",
        "settings.theme",
        save_theme_preset(&app, &preset, &supported_presets, &default_preset),
    )
}

#[allow(clippy::needless_pass_by_value)]
fn get_theme_mode(app: AppHandle) -> Result<String, String> {
    utils::tracing::report_command_result("get_theme_mode", "settings.theme", load_theme_mode(&app))
}

#[allow(clippy::needless_pass_by_value)]
fn set_theme_mode(app: AppHandle, mode: String) -> Result<String, String> {
    utils::tracing::report_command_result(
        "set_theme_mode",
        "settings.theme",
        save_theme_mode(&app, &mode),
    )
}

/// Runs the Qt application backend.
///
/// # Panics
///
/// The native Qt host still needs to be connected to the backend.
pub fn run() {
    unimplemented!("Qt application host must initialize the backend and IPC bridge")
}
