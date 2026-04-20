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
    collections::HashMap,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use matrix_sdk::{
    SlidingSyncList, SlidingSyncMode,
    config::SyncSettings,
    ruma::{
        RoomId,
        api::client::{
            filter::FilterDefinition,
            sync::{sync_events::v3 as classic_sync_http, sync_events::v5 as sliding_sync_http},
        },
        events::StateEventType,
    },
    sliding_sync::SlidingSync,
};
use tauri::Emitter;
use tauri::async_runtime;

use super::{
    FOCUSED_ROOM_TIMELINE_LIMIT, FOCUSED_ROOM_TTL_SECONDS, FocusedRoomState,
    SHELL_SYNC_UPDATED_EVENT, SLIDING_SYNC_ACTIVE_LIST_END_INDEX, SLIDING_SYNC_ACTIVE_LIST_NAME,
    SLIDING_SYNC_ACTIVE_TIMELINE_LIMIT, SLIDING_SYNC_BATCH_SIZE, SLIDING_SYNC_FULL_LIST_NAME,
    SYNC_RETRY_DELAY_SECONDS, SYNC_TIMEOUT_SECONDS, ShellSyncUpdatedPayload,
};

pub(super) fn record_room_update_stamps(
    room_update_stamps: &Arc<RwLock<HashMap<String, u64>>>,
    account_key: &str,
    updates: &matrix_sdk::sync::RoomUpdates,
) {
    let now = now_unix_ms();
    let mut stamps = room_update_stamps
        .write()
        .expect("shell sync manager room-update-stamps lock poisoned");

    for room_id in updates.iter_all_room_ids() {
        stamps.insert(room_state_key(account_key, room_id.as_str()), now);
    }
}

pub(super) async fn build_shell_sliding_sync(
    client: &matrix_sdk::Client,
    account_key: &str,
    enable_to_device_crypto_extensions: bool,
) -> Result<SlidingSync, String> {
    // Follow the SDK's intended two-list model:
    // - a cached growing list warms the broader room inventory in background
    // - a small selective active list keeps the visible shell window snappy
    let builder = client
        .sliding_sync(sliding_sync_identifier(account_key))
        .map_err(|error| format!("Failed to create the sliding-sync builder: {error}"))?;

    let mut account_data_extension = sliding_sync_http::request::AccountData::default();
    account_data_extension.enabled = Some(true);
    let mut builder = builder.with_account_data_extension(account_data_extension);

    if enable_to_device_crypto_extensions {
        let mut e2ee_extension = sliding_sync_http::request::E2EE::default();
        e2ee_extension.enabled = Some(true);
        builder = builder.with_e2ee_extension(e2ee_extension);

        let mut to_device_extension = sliding_sync_http::request::ToDevice::default();
        to_device_extension.enabled = Some(true);
        builder = builder.with_to_device_extension(to_device_extension);
    }

    let active_list = SlidingSyncList::builder(SLIDING_SYNC_ACTIVE_LIST_NAME)
        .sync_mode(
            SlidingSyncMode::new_selective().add_range(0..=SLIDING_SYNC_ACTIVE_LIST_END_INDEX),
        )
        .timeline_limit(SLIDING_SYNC_ACTIVE_TIMELINE_LIMIT)
        .required_state(vec![
            (StateEventType::RoomEncryption, String::new()),
            (StateEventType::RoomName, String::new()),
            (StateEventType::RoomTopic, String::new()),
            (StateEventType::RoomAvatar, String::new()),
        ]);

    let full_list = SlidingSyncList::builder(SLIDING_SYNC_FULL_LIST_NAME)
        .sync_mode(SlidingSyncMode::new_growing(SLIDING_SYNC_BATCH_SIZE))
        .required_state(vec![(StateEventType::RoomEncryption, String::new())]);

    let builder = builder
        .add_list(active_list)
        .add_cached_list(full_list)
        .await
        .map_err(|error| format!("Failed to prepare the cached sliding-sync lists: {error}"))?;

    builder
        .build()
        .await
        .map_err(|error| format!("Failed to build sliding sync: {error}"))
}

pub(super) fn promote_focused_room_if_needed(
    sliding_sync: &SlidingSync,
    focused_rooms: &Arc<RwLock<HashMap<String, FocusedRoomState>>>,
    account_key: &str,
    last_promoted_room_id: &mut Option<String>,
) -> Result<(), String> {
    let now = now_unix_ms();
    let focused_room = focused_rooms
        .read()
        .expect("shell sync manager focused-rooms lock poisoned")
        .get(account_key)
        .cloned()
        .filter(|room| {
            now.saturating_sub(room.last_touched_unix_ms)
                < Duration::from_secs(FOCUSED_ROOM_TTL_SECONDS).as_millis() as u64
        });

    let Some(focused_room) = focused_room else {
        return Ok(());
    };

    if last_promoted_room_id.as_deref() == Some(focused_room.room_id.as_str()) {
        return Ok(());
    }

    let room_id = RoomId::parse(&focused_room.room_id)
        .map_err(|error| format!("Invalid focused room id: {error}"))?;
    let mut subscription = sliding_sync_http::request::RoomSubscription::default();
    // Some homeservers validate `required_state` as a required field and the
    // ruma type omits empty vectors during serialization, so send a minimal
    // state request explicitly instead of relying on an empty list.
    subscription.required_state = vec![(StateEventType::RoomEncryption, String::new())];
    subscription.timeline_limit = FOCUSED_ROOM_TIMELINE_LIMIT.into();

    // matrix-sdk 0.16 exposes room subscription but not room unsubscription,
    // so focused-room promotion is additive for the current session.
    sliding_sync.subscribe_to_rooms(&[&room_id], Some(subscription), true);
    *last_promoted_room_id = Some(focused_room.room_id);

    Ok(())
}

pub(super) fn run_classic_sync_loop(
    _app: &tauri::AppHandle,
    account_key: &str,
    client: &matrix_sdk::Client,
    stop_flag: &AtomicBool,
) {
    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        let sync_settings = SyncSettings::new().timeout(Duration::from_secs(SYNC_TIMEOUT_SECONDS));
        let sync_result = async_runtime::block_on(async { client.sync(sync_settings).await });
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        match sync_result {
            Ok(_) => {}
            Err(error) => {
                eprintln!(
                    "Classic shell sync for account {account_key} stopped with error: {error}"
                );
                thread::sleep(Duration::from_secs(SYNC_RETRY_DELAY_SECONDS));
            }
        }
    }
}

pub(super) fn run_classic_crypto_sync_loop(
    _app: &tauri::AppHandle,
    account_key: &str,
    client: &matrix_sdk::Client,
    stop_flag: &AtomicBool,
) {
    let sync_filter = classic_sync_http::Filter::FilterDefinition(FilterDefinition::ignore_all());

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        let sync_settings = SyncSettings::new()
            .timeout(Duration::from_secs(SYNC_TIMEOUT_SECONDS))
            .filter(sync_filter.clone());
        let sync_result = async_runtime::block_on(async { client.sync(sync_settings).await });
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        match sync_result {
            Ok(_) => {}
            Err(error) => {
                eprintln!(
                    "Classic crypto companion sync for account {account_key} stopped with error: {error}"
                );
                thread::sleep(Duration::from_secs(SYNC_RETRY_DELAY_SECONDS));
            }
        }
    }
}

pub(super) fn is_unknown_pos_error(error: &str) -> bool {
    error.contains("M_UNKNOWN_POS")
}

pub(super) fn is_to_device_since_error(error: &str) -> bool {
    error.contains("extensions.to_device.since")
        && (error.contains("M_INVALID_PARAM") || error.contains("invalid"))
}

pub(super) fn emit_shell_sync_updated(
    app: &tauri::AppHandle,
    account_key: &str,
    updates: Option<&matrix_sdk::sync::RoomUpdates>,
) {
    let payload = ShellSyncUpdatedPayload {
        account_key: account_key.to_owned(),
        changed_room_ids: updates
            .map(room_updates_changed_room_ids)
            .unwrap_or_default(),
        room_list_may_have_changed: updates.is_some_and(|updates| !updates.is_empty()),
        updated_at_unix_ms: now_unix_ms(),
    };

    if let Err(error) = app.emit(SHELL_SYNC_UPDATED_EVENT, payload) {
        eprintln!("Failed to emit shell sync update event: {error}");
    }
}

fn room_state_key(account_key: &str, room_id: &str) -> String {
    format!("{account_key}::{room_id}")
}

fn sliding_sync_identifier(account_key: &str) -> String {
    let mut hasher = DefaultHasher::new();
    account_key.hash(&mut hasher);
    let shortened_hash = hasher.finish() & 0x00FF_FFFF_FFFF_FFFF;

    // matrix-sdk 0.16 requires the identifier to be strictly shorter than
    // 16 characters, not less-than-or-equal.
    format!("s{:014x}", shortened_hash)
}

pub(super) fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn room_updates_changed_room_ids(updates: &matrix_sdk::sync::RoomUpdates) -> Vec<String> {
    updates
        .iter_all_room_ids()
        .map(|room_id| room_id.to_string())
        .collect()
}
