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

/// Qt-facing IPC surface. JSON strings keep the bridge independent of the
/// Rust-only request and response DTOs used by the Matrix services.
use std::{future::Future, pin::Pin, sync::OnceLock, thread};

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use serde::Serialize;
use tokio::runtime::Runtime;

use crate::{
    account::{AccountManager, LoginRequest, RegisterAccountRequest},
    native::host::{AppHandle, State},
    settings::encryption::{RoomKeyFileRequest, export_room_keys, import_room_keys},
    settings::theme::{
        get_theme_mode as load_theme_mode, get_theme_preset as load_theme_preset,
        set_theme_mode as save_theme_mode, set_theme_preset as save_theme_preset,
    },
    shell::{
        service::{
            ShellManager,
            discovery::types::{
                InviteUserToRoomRequest, JoinDiscoveryRoomRequest, ListInviteTargetsRequest,
                SearchDiscoveryEntitiesRequest,
            },
        },
        types::{
            EditRoomMessageRequest, GetRoomEventContextRequest, GetRoomSummaryRequest,
            GetRoomTimelineRequest, GlobalSearchIndexStatus, GlobalSearchRequest,
            GlobalSearchResponse, ListRoomThreadsRequest, ListSpacesRequest,
            PaginateRoomTimelineRequest, RedactRoomMessageRequest, ReplyToRoomMessageRequest,
            ResolveRoomReplyPreviewRequest, SendRoomMessageRequest, SetRoomTypingRequest,
            ToggleRoomReactionRequest,
        },
    },
    utils,
};

static IPC_RUNTIME: OnceLock<Runtime> = OnceLock::new();

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[namespace = "hyperion"]
        type HyperionIpc = super::HyperionIpcRust;

        #[qsignal]
        #[cxx_name = "commandCompleted"]
        fn command_completed(self: Pin<&mut Self>, request_id: i64, result_json: &QString);

        #[qinvokable]
        #[cxx_name = "initializeAppPaths"]
        fn initialize_app_paths(
            self: Pin<&mut Self>,
            app_data_url: &QString,
            app_cache_url: &QString,
        );

        // Declare each QML-callable method here, then implement its Rust body in
        // `impl qobject::HyperionIpc` below. Keep bridge arguments CXX-compatible;
        // serialize Rust-only request and response types as JSON strings.
        #[qinvokable]
        #[cxx_name = "loginAccount"]
        fn login_account(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "listAccounts"]
        fn list_accounts(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "switchActiveAccount"]
        fn switch_active_account(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "activeAccount"]
        fn active_account(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "signOutActiveAccount"]
        fn sign_out_active_account(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "validateActiveAccount"]
        fn validate_active_account(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "listRegistrationHomeservers"]
        fn list_registration_homeservers(
            self: Pin<&mut Self>,
            request_id: i64,
            request_json: &QString,
        );
        #[qinvokable]
        #[cxx_name = "registerAccount"]
        fn register_account(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "listRoomThreads"]
        fn list_room_threads(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "getRoomSummary"]
        fn get_room_summary(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "getRoomTimeline"]
        fn get_room_timeline(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "getRoomEventContext"]
        fn get_room_event_context(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "paginateRoomTimelineBackwards"]
        fn paginate_room_timeline_backwards(
            self: Pin<&mut Self>,
            request_id: i64,
            request_json: &QString,
        );
        #[qinvokable]
        #[cxx_name = "resolveRoomReplyPreview"]
        fn resolve_room_reply_preview(
            self: Pin<&mut Self>,
            request_id: i64,
            request_json: &QString,
        );
        #[qinvokable]
        #[cxx_name = "sendRoomMessage"]
        fn send_room_message(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "editRoomMessage"]
        fn edit_room_message(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "redactRoomMessage"]
        fn redact_room_message(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "replyToRoomMessage"]
        fn reply_to_room_message(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "toggleRoomReaction"]
        fn toggle_room_reaction(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "setRoomTyping"]
        fn set_room_typing(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "listSpaces"]
        fn list_spaces(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "globalSearch"]
        fn global_search(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "searchDiscoveryEntities"]
        fn search_discovery_entities(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "joinDiscoveryRoom"]
        fn join_discovery_room(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "inviteUserToRoom"]
        fn invite_user_to_room(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "listInviteTargets"]
        fn list_invite_targets(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "getThemePreset"]
        fn get_theme_preset(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "setThemePreset"]
        fn set_theme_preset(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "getThemeMode"]
        fn get_theme_mode(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "setThemeMode"]
        fn set_theme_mode(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "exportRoomKeys"]
        fn export_room_keys(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
        #[qinvokable]
        #[cxx_name = "importRoomKeys"]
        fn import_room_keys(self: Pin<&mut Self>, request_id: i64, request_json: &QString);
    }

    impl cxx_qt::Threading for HyperionIpc {}
}

pub struct HyperionIpcRust {
    app: crate::native::host::AppHandle,
    account_manager: crate::account::AccountManager,
    shell_manager: crate::shell::service::ShellManager,
}

impl Default for HyperionIpcRust {
    fn default() -> Self {
        Self {
            app: crate::native::host::AppHandle,
            account_manager: crate::account::AccountManager::new(),
            shell_manager: crate::shell::service::ShellManager::new(),
        }
    }
}

impl qobject::HyperionIpc {
    fn initialize_app_paths(self: Pin<&mut Self>, app_data_url: &QString, app_cache_url: &QString) {
        let result = crate::native::host::set_qt_app_directories(
            &app_data_url.to_string(),
            &app_cache_url.to_string(),
        );
        let _ = crate::utils::tracing::report_command_result("initializeAppPaths", "host", result);
    }

    /// Clone the QObject-owned backend dependencies before a command future is dispatched.
    /// The future must own its inputs so it can safely outlive the synchronous QML call.
    fn backend_context(self: Pin<&Self>) -> (AppHandle, AccountManager, ShellManager) {
        let backend = self.rust();
        (
            backend.app,
            backend.account_manager.clone(),
            backend.shell_manager.clone(),
        )
    }

    /// Run the command future off the Qt event loop, then queue its serialized
    /// result back onto the QObject's thread and emit `command_completed`.
    fn submit<T>(
        self: Pin<&mut Self>,
        request_id: i64,
        future: impl Future<Output = Result<T, String>> + Send + 'static,
    ) where
        T: Serialize + Send + 'static,
    {
        let qt_thread: CxxQtThread<Self> = self.qt_thread();

        thread::spawn(move || {
            let runtime = IPC_RUNTIME.get_or_init(|| {
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("failed to initialize the Tokio runtime for Qt IPC")
            });
            let result = runtime.block_on(future);
            let result_json = QString::from(encode_command_result(result).as_str());
            let _ = qt_thread.queue(move |object| {
                object.command_completed(request_id, &result_json);
            });
        });
    }

    // To add a command, declare its QML signature in the bridge above and add one
    // method here. Decode any JSON request inside the `async move` block, obtain
    // the backend values it needs with `backend_context`, then pass that future
    // to `submit`; it handles worker execution and emits the correlated result.

    pub fn login_account(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<LoginRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future("login_account", "account", async move {
                let account = account_manager.login(&app, request).await?;
                shell_manager
                    .ensure_active_account_sync(&app, &account_manager)
                    .await?;
                Ok(account)
            })
            .await
        });
    }

    pub fn list_accounts(self: Pin<&mut Self>, request_id: i64, _request_json: &QString) {
        let (app, account_manager, _) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let account_manager = State(&account_manager);
            utils::tracing::report_command_future(
                "list_accounts",
                "account",
                account_manager.list_accounts(&app),
            )
            .await
        });
    }

    pub fn switch_active_account(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<SwitchActiveAccountRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future("switch_active_account", "account", async move {
                account_manager
                    .switch_active_account(&app, &request.account_key)
                    .await?;
                shell_manager
                    .ensure_active_account_sync(&app, &account_manager)
                    .await
            })
            .await
        });
    }

    pub fn active_account(self: Pin<&mut Self>, request_id: i64, _request_json: &QString) {
        let (app, account_manager, _) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let account_manager = State(&account_manager);
            utils::tracing::report_command_future(
                "active_account",
                "account",
                account_manager.active_account(&app),
            )
            .await
        });
    }

    pub fn sign_out_active_account(self: Pin<&mut Self>, request_id: i64, _request_json: &QString) {
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future(
                "sign_out_active_account",
                "account",
                async move {
                    let active_account = account_manager.active_account(&app).await?;
                    if let Some(account) = active_account {
                        shell_manager.stop_account(&account.account_key).await;
                    }
                    let next_account = account_manager.sign_out_active_account(&app).await?;
                    if next_account.is_some() {
                        shell_manager
                            .ensure_active_account_sync(&app, &account_manager)
                            .await?;
                    } else {
                        shell_manager.stop_all_accounts().await;
                    }
                    Ok(next_account)
                },
            )
            .await
        });
    }

    pub fn validate_active_account(self: Pin<&mut Self>, request_id: i64, _request_json: &QString) {
        let (app, account_manager, _) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let account_manager = State(&account_manager);
            utils::tracing::report_command_future(
                "validate_active_account",
                "account",
                account_manager.validate_active_account(&app),
            )
            .await
        });
    }

    pub fn list_registration_homeservers(
        self: Pin<&mut Self>,
        request_id: i64,
        _request_json: &QString,
    ) {
        let (_, account_manager, _) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let account_manager = State(&account_manager);
            utils::tracing::report_command_future(
                "list_registration_homeservers",
                "account",
                account_manager.list_registration_homeservers(),
            )
            .await
        });
    }

    pub fn register_account(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, _) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<RegisterAccountRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            utils::tracing::report_command_future(
                "register_account",
                "account",
                Box::pin(account_manager.register_account(&app, request)),
            )
            .await
        });
    }

    pub fn list_room_threads(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<Option<ListRoomThreadsRequest>>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future("list_room_threads", "shell.room", async move {
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
        });
    }

    pub fn get_room_summary(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<GetRoomSummaryRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future("get_room_summary", "shell.room", async move {
                let active_account = account_manager.require_active_account(&app).await?;
                shell_manager
                    .get_room_summary(&app, &account_manager, &active_account, request)
                    .await
            })
            .await
        });
    }

    pub fn get_room_timeline(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<GetRoomTimelineRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future(
                "get_room_timeline",
                "shell.timeline",
                async move {
                    let active_account = account_manager.require_active_account(&app).await?;
                    shell_manager
                        .get_room_timeline(&app, &account_manager, &active_account, request)
                        .await
                },
            )
            .await
        });
    }

    pub fn get_room_event_context(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<GetRoomEventContextRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future(
                "get_room_event_context",
                "shell.timeline",
                async move {
                    let active_account = account_manager.require_active_account(&app).await?;
                    shell_manager
                        .get_room_event_context(&app, &account_manager, &active_account, request)
                        .await
                },
            )
            .await
        });
    }

    pub fn paginate_room_timeline_backwards(
        self: Pin<&mut Self>,
        request_id: i64,
        request_json: &QString,
    ) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<PaginateRoomTimelineRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future(
                "paginate_room_timeline_backwards",
                "shell.timeline",
                async move {
                    let active_account = account_manager.require_active_account(&app).await?;
                    shell_manager
                        .paginate_room_timeline_backwards(
                            &app,
                            &account_manager,
                            &active_account,
                            request,
                        )
                        .await
                },
            )
            .await
        });
    }

    pub fn resolve_room_reply_preview(
        self: Pin<&mut Self>,
        request_id: i64,
        request_json: &QString,
    ) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<ResolveRoomReplyPreviewRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future(
                "resolve_room_reply_preview",
                "shell.timeline",
                async move {
                    let active_account = account_manager.require_active_account(&app).await?;
                    shell_manager
                        .resolve_room_reply_preview(
                            &app,
                            &account_manager,
                            &active_account,
                            request,
                        )
                        .await
                },
            )
            .await
        });
    }

    pub fn send_room_message(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<SendRoomMessageRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future("send_room_message", "shell.room", async move {
                let active_account = account_manager.require_active_account(&app).await?;
                shell_manager
                    .send_room_message(&app, &account_manager, &active_account, request)
                    .await
            })
            .await
        });
    }

    pub fn edit_room_message(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<EditRoomMessageRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future("edit_room_message", "shell.room", async move {
                let active_account = account_manager.require_active_account(&app).await?;
                shell_manager
                    .edit_room_message(&active_account, request)
                    .await
            })
            .await
        });
    }

    pub fn redact_room_message(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<RedactRoomMessageRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future("redact_room_message", "shell.room", async move {
                let active_account = account_manager.require_active_account(&app).await?;
                shell_manager
                    .redact_room_message(&active_account, request)
                    .await
            })
            .await
        });
    }

    pub fn reply_to_room_message(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<ReplyToRoomMessageRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future(
                "reply_to_room_message",
                "shell.room",
                async move {
                    let active_account = account_manager.require_active_account(&app).await?;
                    shell_manager
                        .reply_to_room_message(&active_account, request)
                        .await
                },
            )
            .await
        });
    }

    pub fn toggle_room_reaction(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<ToggleRoomReactionRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future(
                "toggle_room_reaction",
                "shell.room",
                async move {
                    let active_account = account_manager.require_active_account(&app).await?;
                    shell_manager
                        .toggle_room_reaction(&active_account, request)
                        .await
                },
            )
            .await
        });
    }

    pub fn set_room_typing(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<SetRoomTypingRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future("set_room_typing", "shell.room", async move {
                let active_account = account_manager.require_active_account(&app).await?;
                shell_manager
                    .set_room_typing(&app, &active_account, request)
                    .await
            })
            .await
        });
    }

    pub fn list_spaces(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<Option<ListSpacesRequest>>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future("list_spaces", "shell.room", async move {
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
        });
    }

    pub fn global_search(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<GlobalSearchRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future("global_search", "shell.search", async move {
                let Some(active_account) = account_manager.optional_active_account(&app).await?
                else {
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
        });
    }

    pub fn search_discovery_entities(
        self: Pin<&mut Self>,
        request_id: i64,
        request_json: &QString,
    ) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<SearchDiscoveryEntitiesRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future(
                "search_discovery_entities",
                "shell.discovery",
                async move {
                    let active_account = account_manager.require_active_account(&app).await?;
                    shell_manager
                        .search_discovery_entities(&active_account, request)
                        .await
                },
            )
            .await
        });
    }

    pub fn join_discovery_room(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<JoinDiscoveryRoomRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future(
                "join_discovery_room",
                "shell.discovery",
                async move {
                    let active_account = account_manager.require_active_account(&app).await?;
                    shell_manager
                        .join_discovery_room(&app, &account_manager, &active_account, request)
                        .await
                },
            )
            .await
        });
    }

    pub fn invite_user_to_room(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, shell_manager) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<InviteUserToRoomRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            let shell_manager = State(&shell_manager);
            utils::tracing::report_command_future(
                "invite_user_to_room",
                "shell.discovery",
                async move {
                    let active_account = account_manager.require_active_account(&app).await?;
                    shell_manager
                        .invite_user_to_room(&active_account, request)
                        .await
                },
            )
            .await
        });
    }

    pub fn list_invite_targets(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, _) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<ListInviteTargetsRequest>(&request_json)?;
            let account_manager = State(&account_manager);
            utils::tracing::report_command_future(
                "list_invite_targets",
                "shell.discovery",
                async move {
                    let active_account = account_manager.require_active_account(&app).await?;
                    ShellManager::list_invite_targets(&active_account, &request)
                },
            )
            .await
        });
    }

    pub fn get_theme_preset(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, _, _) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<ThemePresetRequest>(&request_json)?;
            utils::tracing::report_command_result(
                "get_theme_preset",
                "settings.theme",
                load_theme_preset(&app, &request.supported_presets, &request.default_preset),
            )
        });
    }

    pub fn set_theme_preset(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, _, _) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<SetThemePresetRequest>(&request_json)?;
            utils::tracing::report_command_result(
                "set_theme_preset",
                "settings.theme",
                save_theme_preset(
                    &app,
                    &request.preset,
                    &request.supported_presets,
                    &request.default_preset,
                ),
            )
        });
    }

    pub fn get_theme_mode(self: Pin<&mut Self>, request_id: i64, _request_json: &QString) {
        let (app, _, _) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            utils::tracing::report_command_result(
                "get_theme_mode",
                "settings.theme",
                load_theme_mode(&app),
            )
        });
    }

    pub fn set_theme_mode(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, _, _) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<ThemeModeRequest>(&request_json)?;
            utils::tracing::report_command_result(
                "set_theme_mode",
                "settings.theme",
                save_theme_mode(&app, &request.mode),
            )
        });
    }

    pub fn export_room_keys(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, _) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<RoomKeyFileRequest>(&request_json)?;
            export_room_keys(app, State(&account_manager), request).await
        });
    }

    pub fn import_room_keys(self: Pin<&mut Self>, request_id: i64, request_json: &QString) {
        let request_json = request_json.to_string();
        let (app, account_manager, _) = self.as_ref().backend_context();
        self.submit(request_id, async move {
            let request = decode_request::<RoomKeyFileRequest>(&request_json)?;
            import_room_keys(app, State(&account_manager), request).await
        });
    }
}

#[derive(serde::Deserialize)]
struct SwitchActiveAccountRequest {
    account_key: String,
}

#[derive(serde::Deserialize)]
struct ThemePresetRequest {
    supported_presets: Vec<String>,
    default_preset: String,
}

#[derive(serde::Deserialize)]
struct SetThemePresetRequest {
    preset: String,
    supported_presets: Vec<String>,
    default_preset: String,
}

#[derive(serde::Deserialize)]
struct ThemeModeRequest {
    mode: String,
}

/// Deserialize a QML JSON request after its command future starts on the worker thread.
fn decode_request<T: serde::de::DeserializeOwned>(request_json: &str) -> Result<T, String> {
    serde_json::from_str(request_json).map_err(|error| format!("invalid IPC request: {error}"))
}

/// Encode the common `{ ok, value }` / `{ ok, error }` response sent by `command_completed`.
fn encode_command_result<T: Serialize>(result: Result<T, String>) -> String {
    let response = match result {
        Ok(value) => match serde_json::to_value(value) {
            Ok(value) => serde_json::json!({ "ok": true, "value": value }),
            Err(error) => serde_json::json!({
                "ok": false,
                "error": format!("failed to encode IPC result: {error}"),
            }),
        },
        Err(error) => serde_json::json!({ "ok": false, "error": error }),
    };
    response.to_string()
}
