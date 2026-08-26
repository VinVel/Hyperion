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

use crate::{
    account::{AccountClientSnapshot, AccountManager},
    shell::service::caching::remove_legacy_timeline_view_cache,
};

use super::{coordinator::ShellSyncCoordinator, diagnostics::emit_sync_diagnostic};

impl ShellSyncCoordinator {
    pub(in crate::shell::service) async fn ensure_account_running(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        account: AccountClientSnapshot,
    ) -> Result<(), String> {
        emit_sync_diagnostic(
            "sync.account.ensure",
            &[("account_key", &account.account_key)],
        );
        remove_legacy_timeline_view_cache(&account.store_dir);
        self.clear_inactive_account_focus(&account.account_key);
        self.clear_inactive_account_typing_state(&account.account_key);
        self.sync_manager
            .ensure_started_for_account(app, account_manager, account)
            .await
    }
    pub(in crate::shell::service) async fn stop_account(&self, account_key: &str) {
        emit_sync_diagnostic("sync.account.stop", &[("account_key", account_key)]);
        self.clear_account_focus(account_key);
        self.clear_account_typing_state(account_key, "account_stop");
        self.sync_manager.stop_account(account_key).await;
    }
    pub(in crate::shell::service) async fn stop_all_accounts(&self) {
        emit_sync_diagnostic("sync.account.stop_all", &[]);
        self.clear_all_account_focus();
        self.clear_all_typing_state("all_accounts_stop");
        self.sync_manager.stop_all_accounts().await;
    }
}
