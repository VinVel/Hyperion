/*
 * Copyright (c) 2026 VinVel
 *
 * SPDX-License-Identifier: AGPL-3.0-only
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, version 3 only.
 *
 * Project home: hyperion.velcore.net
 */

use crate::{account::AccountManager, shell::types::GlobalSearchIndexStatus};

use super::{ShellManager, runtime::ShellSearchService, search::commands};
use crate::shell::types::{GlobalSearchRequest, GlobalSearchResponse};

impl ShellManager {
    pub async fn global_search(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: GlobalSearchRequest,
    ) -> Result<GlobalSearchResponse, String> {
        self.search_service
            .global_search(app, account_manager, &self.sync_manager, request)
            .await
    }
}

impl ShellSearchService {
    pub(super) async fn global_search(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        sync_manager: &crate::shell::sync::ShellSyncManager,
        request: GlobalSearchRequest,
    ) -> Result<GlobalSearchResponse, String> {
        account_manager.ensure_loaded(app).await?;
        let Some(account) = account_manager.active_account_client_loaded() else {
            return Ok(GlobalSearchResponse {
                rooms: Vec::new(),
                spaces: Vec::new(),
                messages: Vec::new(),
                status: GlobalSearchIndexStatus::default(),
            });
        };

        sync_manager
            .ensure_started_for_account(app, account_manager, account.clone())
            .await?;

        let query = request.query.trim();
        if query.is_empty() {
            return Ok(GlobalSearchResponse {
                rooms: Vec::new(),
                spaces: Vec::new(),
                messages: Vec::new(),
                status: super::search::SearchStatusReporter::status_for_account(
                    &account.account_key,
                    &account.store_dir,
                ),
            });
        }

        let limit = request
            .limit_per_group
            .unwrap_or(commands::DEFAULT_SEARCH_LIMIT_PER_GROUP);

        commands::global_search(&account.account_key, &account.store_dir, query, limit)
    }
}
