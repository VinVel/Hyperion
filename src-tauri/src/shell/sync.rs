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

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use futures_util::StreamExt;
use matrix_sdk::sliding_sync::Version as SlidingSyncVersion;
use serde::Serialize;
use tauri::async_runtime::{self, JoinHandle};

use crate::account::{AccountClientSnapshot, AccountManager};

mod runtime;

use self::runtime::{
    build_shell_sliding_sync, emit_shell_sync_updated, is_to_device_since_error,
    is_unknown_pos_error, now_unix_ms, promote_focused_room_if_needed, record_room_update_stamps,
    run_classic_crypto_sync_loop, run_classic_sync_loop,
};

pub const SHELL_SYNC_UPDATED_EVENT: &str = "hyperion://shell-sync-updated";

// Keep sync requests long-polled so idle accounts do not spin, while still
// letting the shell react quickly enough when the server has new data.
const SYNC_TIMEOUT_SECONDS: u64 = 30;
// Errors should retry soon enough to recover from transient failures, but not
// in a tight loop that hammers the homeserver or floods local logs.
const SYNC_RETRY_DELAY_SECONDS: u64 = 5;
// Expired sliding-sync positions are expected to recover by starting a fresh
// session, so use a short reset delay instead of treating them as hard faults.
const SLIDING_SYNC_SESSION_RESET_RETRY_DELAY_SECONDS: u64 = 1;
// The two list names are part of the SDK's sticky sliding-sync identity, so
// keep them stable across restarts to preserve cached list behavior.
const SLIDING_SYNC_FULL_LIST_NAME: &str = "full-sync";
const SLIDING_SYNC_ACTIVE_LIST_NAME: &str = "active-list";
// Grow the background list in moderate batches so the shell can warm room
// inventory without forcing every room into the first interactive response.
const SLIDING_SYNC_BATCH_SIZE: u32 = 20;
// The active list mirrors the visible conversation window rather than the full
// account, so its selective range stays intentionally small and predictable.
const SLIDING_SYNC_ACTIVE_LIST_END_INDEX: u32 = 19;
// The active list only needs a shallow timeline for previews; deeper history is
// fetched by room-specific commands when the user actually opens a room.
const SLIDING_SYNC_ACTIVE_TIMELINE_LIMIT: u32 = 5;
// Focused rooms stay promoted briefly after interaction so rapid navigation and
// send/reply flows do not constantly churn the focused-room subscription.
const FOCUSED_ROOM_TTL_SECONDS: u64 = 90;
// The focused room gets a larger live timeline window than the list preview so
// an open conversation feels immediate without inflating every room equally.
const FOCUSED_ROOM_TIMELINE_LIMIT: u32 = 40;
#[derive(Clone, Copy, PartialEq, Eq)]
enum SlidingSyncCryptoMode {
    FullyIntegrated,
    ClassicCompanion,
}

#[derive(Clone, Serialize)]
struct ShellSyncUpdatedPayload {
    account_key: String,
    changed_room_ids: Vec<String>,
    room_list_may_have_changed: bool,
    updated_at_unix_ms: u64,
}

#[derive(Clone)]
struct FocusedRoomState {
    room_id: String,
    last_touched_unix_ms: u64,
}

#[derive(Clone, Default)]
pub struct ShellSyncManager {
    started_accounts: Arc<RwLock<HashSet<String>>>,
    task_handles: Arc<RwLock<HashMap<String, Vec<JoinHandle<()>>>>>,
    focused_rooms: Arc<RwLock<HashMap<String, FocusedRoomState>>>,
    stop_flags: Arc<RwLock<HashMap<String, Arc<AtomicBool>>>>,
    crypto_modes: Arc<RwLock<HashMap<String, SlidingSyncCryptoMode>>>,
    room_update_stamps: Arc<RwLock<HashMap<String, u64>>>,
}

impl ShellSyncManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn ensure_started_for_manager(
        &self,
        manager: &AccountManager,
        app: &tauri::AppHandle,
    ) -> Result<(), String> {
        let Some(active_account) = manager.active_account_client(app).await? else {
            self.stop_all_accounts();
            return Ok(());
        };

        self.stop_other_accounts(&active_account.account_key);
        self.ensure_started(app, active_account).await
    }

    pub fn touch_focused_room(&self, account_key: &str, room_id: &str) {
        let now = now_unix_ms();
        let mut focused_rooms = self
            .focused_rooms
            .write()
            .expect("shell sync manager focused-rooms lock poisoned");

        focused_rooms.retain(|_, room| {
            now.saturating_sub(room.last_touched_unix_ms)
                < Duration::from_secs(FOCUSED_ROOM_TTL_SECONDS).as_millis() as u64
        });
        focused_rooms.insert(
            account_key.to_owned(),
            FocusedRoomState {
                room_id: room_id.to_owned(),
                last_touched_unix_ms: now,
            },
        );
    }

    async fn ensure_started(
        &self,
        app: &tauri::AppHandle,
        account: AccountClientSnapshot,
    ) -> Result<(), String> {
        {
            let started_accounts = self
                .started_accounts
                .read()
                .expect("shell sync manager started-accounts lock poisoned");
            if started_accounts.contains(&account.account_key) {
                return Ok(());
            }
        }

        let _ = account.client.event_cache().subscribe();

        let account_key = account.account_key.clone();
        let client = account.client.clone();
        let stop_flag = Arc::new(AtomicBool::new(false));
        self.stop_flags
            .write()
            .expect("shell sync manager stop-flags lock poisoned")
            .insert(account_key.clone(), stop_flag.clone());
        let prefers_sliding_sync =
            matches!(client.sliding_sync_version(), SlidingSyncVersion::Native);
        let handle = if prefers_sliding_sync {
            self.spawn_sliding_sync_task(
                app.clone(),
                account_key.clone(),
                client.clone(),
                stop_flag,
            )
        } else {
            self.spawn_classic_sync_task(
                app.clone(),
                account_key.clone(),
                client.clone(),
                stop_flag,
            )
        };
        let room_update_handle =
            self.spawn_room_update_listener_task(app.clone(), account_key.clone(), client.clone());

        self.task_handles
            .write()
            .expect("shell sync manager task-handles lock poisoned")
            .insert(
                account.account_key.clone(),
                vec![handle, room_update_handle],
            );

        self.started_accounts
            .write()
            .expect("shell sync manager started-accounts lock poisoned")
            .insert(account.account_key);

        Ok(())
    }

    fn spawn_classic_sync_task(
        &self,
        app: tauri::AppHandle,
        account_key: String,
        client: matrix_sdk::Client,
        stop_flag: Arc<AtomicBool>,
    ) -> JoinHandle<()> {
        async_runtime::spawn_blocking(move || {
            run_classic_sync_loop(&app, &account_key, &client, &stop_flag)
        })
    }

    fn spawn_sliding_sync_task(
        &self,
        app: tauri::AppHandle,
        account_key: String,
        client: matrix_sdk::Client,
        stop_flag: Arc<AtomicBool>,
    ) -> JoinHandle<()> {
        let focused_rooms = self.focused_rooms.clone();
        let crypto_modes = self.crypto_modes.clone();

        async_runtime::spawn_blocking(move || {
            let mut crypto_mode = crypto_modes
                .read()
                .expect("shell sync manager crypto-modes lock poisoned")
                .get(&account_key)
                .copied()
                .unwrap_or(SlidingSyncCryptoMode::FullyIntegrated);
            let mut classic_crypto_companion_started = false;

            loop {
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }

                let sliding_sync_result = async_runtime::block_on(async {
                    build_shell_sliding_sync(
                        &client,
                        &account_key,
                        crypto_mode == SlidingSyncCryptoMode::FullyIntegrated,
                    )
                    .await
                });

                match sliding_sync_result {
                    Ok(sliding_sync) => {
                        let sync_result = async_runtime::block_on(async {
                            let stream = sliding_sync.sync();
                            futures_util::pin_mut!(stream);
                            let mut last_promoted_room_id: Option<String> = None;

                            promote_focused_room_if_needed(
                                &sliding_sync,
                                &focused_rooms,
                                &account_key,
                                &mut last_promoted_room_id,
                            )?;

                            while let Some(update) = stream.next().await {
                                if stop_flag.load(Ordering::Relaxed) {
                                    break;
                                }
                                update.map_err(|error| format!("{error}"))?;
                                promote_focused_room_if_needed(
                                    &sliding_sync,
                                    &focused_rooms,
                                    &account_key,
                                    &mut last_promoted_room_id,
                                )?;
                            }

                            Ok::<(), String>(())
                        });

                        let _ = sliding_sync.stop_sync();
                        if stop_flag.load(Ordering::Relaxed) {
                            break;
                        }

                        if let Err(error) = sync_result {
                            if is_unknown_pos_error(&error) {
                                eprintln!(
                                    "Sliding shell sync for account {account_key} returned an expired position ({error}); restarting sliding sync"
                                );
                                thread::sleep(Duration::from_secs(
                                    SLIDING_SYNC_SESSION_RESET_RETRY_DELAY_SECONDS,
                                ));
                                continue;
                            }

                            if is_to_device_since_error(&error)
                                && crypto_mode == SlidingSyncCryptoMode::FullyIntegrated
                            {
                                eprintln!(
                                    "Sliding shell sync for account {account_key} hit a to-device token compatibility issue ({error}); keeping sliding sync for rooms and starting a classic crypto companion"
                                );
                                crypto_mode = SlidingSyncCryptoMode::ClassicCompanion;
                                crypto_modes
                                    .write()
                                    .expect("shell sync manager crypto-modes lock poisoned")
                                    .insert(account_key.clone(), crypto_mode);

                                if !classic_crypto_companion_started {
                                    classic_crypto_companion_started = true;
                                    let companion_account_key = account_key.clone();
                                    let companion_client = client.clone();
                                    let companion_stop_flag = stop_flag.clone();
                                    let companion_app = app.clone();
                                    thread::spawn(move || {
                                        run_classic_crypto_sync_loop(
                                            &companion_app,
                                            &companion_account_key,
                                            &companion_client,
                                            &companion_stop_flag,
                                        )
                                    });
                                }

                                thread::sleep(Duration::from_secs(
                                    SLIDING_SYNC_SESSION_RESET_RETRY_DELAY_SECONDS,
                                ));
                                continue;
                            }

                            eprintln!(
                                "Sliding shell sync for account {account_key} stopped with error: {error}"
                            );
                        } else {
                            eprintln!(
                                "Sliding shell sync for account {account_key} ended without an error"
                            );
                        }
                    }
                    Err(error) => {
                        eprintln!(
                            "Sliding shell sync for account {account_key} could not be initialized: {error}"
                        );
                    }
                }

                thread::sleep(Duration::from_secs(SYNC_RETRY_DELAY_SECONDS));
            }
        })
    }

    fn spawn_room_update_listener_task(
        &self,
        app: tauri::AppHandle,
        account_key: String,
        client: matrix_sdk::Client,
    ) -> JoinHandle<()> {
        let room_update_stamps = self.room_update_stamps.clone();
        async_runtime::spawn(async move {
            let mut room_updates = client.subscribe_to_all_room_updates();

            loop {
                match room_updates.recv().await {
                    Ok(updates) => {
                        record_room_update_stamps(&room_update_stamps, &account_key, &updates);
                        emit_shell_sync_updated(&app, &account_key, Some(&updates));
                    }
                    Err(error) => {
                        eprintln!(
                            "Shell room update listener for account {account_key} stopped with error: {error}"
                        );
                        break;
                    }
                }
            }
        })
    }

    fn stop_other_accounts(&self, active_account_key: &str) {
        let inactive_account_keys = {
            let started_accounts = self
                .started_accounts
                .read()
                .expect("shell sync manager started-accounts lock poisoned");
            started_accounts
                .iter()
                .filter(|account_key| account_key.as_str() != active_account_key)
                .cloned()
                .collect::<Vec<_>>()
        };

        for account_key in inactive_account_keys {
            self.stop_account(&account_key);
        }
    }

    fn stop_all_accounts(&self) {
        let account_keys = self
            .started_accounts
            .read()
            .expect("shell sync manager started-accounts lock poisoned")
            .iter()
            .cloned()
            .collect::<Vec<_>>();

        for account_key in account_keys {
            self.stop_account(&account_key);
        }
    }

    fn stop_account(&self, account_key: &str) {
        if let Some(stop_flag) = self
            .stop_flags
            .write()
            .expect("shell sync manager stop-flags lock poisoned")
            .remove(account_key)
        {
            stop_flag.store(true, Ordering::Relaxed);
        }

        if let Some(handles) = self
            .task_handles
            .write()
            .expect("shell sync manager task-handles lock poisoned")
            .remove(account_key)
        {
            for handle in handles {
                handle.abort();
            }
        }

        self.started_accounts
            .write()
            .expect("shell sync manager started-accounts lock poisoned")
            .remove(account_key);
        self.crypto_modes
            .write()
            .expect("shell sync manager crypto-modes lock poisoned")
            .remove(account_key);
        self.focused_rooms
            .write()
            .expect("shell sync manager focused-rooms lock poisoned")
            .remove(account_key);
        self.room_update_stamps
            .write()
            .expect("shell sync manager room-update-stamps lock poisoned")
            .retain(|state_key, _| !state_key.starts_with(&format!("{account_key}::")));
    }
}
