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

use std::{cmp::Reverse, collections::HashSet, time::Duration};

use futures_util::{
    future::{Either, select},
    pin_mut,
};
use matrix_sdk::{
    Room, RoomMemberships, RoomState,
    ruma::{
        OwnedServerName, RoomId, RoomOrAliasId, ServerName, UserId,
        api::client::directory::get_public_rooms_filtered::v3::Request as PublicRoomsFilterRequest,
        directory::{Filter, PublicRoomsChunk, RoomTypeFilter},
        room::{JoinRuleKind, RoomType},
    },
    sleep::sleep,
};
use reqwest::Client as HttpClient;

use crate::{
    account::AccountManager,
    shell::{
        service::{ShellCacheState, ShellManager, ShellRoomListKind},
        sync::ShellSyncManager,
        types::{RoomThreadSummary, SpaceSummary},
    },
    utils::{http::external_http_client, url::encode_path_segment},
};

use super::super::{room::list::snapshot_room_list_for_account, runtime::ShellDiscoveryService};

use super::types::{
    DiscoveryEntity, DiscoveryEntityKind, DiscoverySource, InviteTarget, InviteUserToRoomRequest,
    JoinDiscoveryRoomRequest, JoinDiscoveryRoomResponse, ListInviteTargetsRequest,
    MatrixRoomsInfoEntry, MatrixRoomsInfoSearchResponse, SearchDiscoveryEntitiesRequest,
};

const MATRIXROOMS_INFO_BASE_URL: &str = "https://api.matrixrooms.info";
const DEFAULT_DISCOVERY_LIMIT: usize = 20;
const MAX_DISCOVERY_LIMIT: usize = 50;
const DEFAULT_ROOM_RECOMMENDATION_QUERY: &str = "matrix";
const DEFAULT_SPACE_RECOMMENDATION_QUERY: &str = "space";
const MAX_USER_RECOMMENDATION_ROOMS: usize = 12;
// Public room and space joins can involve remote federation. Keep the UI from
// waiting indefinitely when a homeserver does not complete the join promptly.
const DISCOVERY_JOIN_TIMEOUT_MS: u64 = 45_000;

impl ShellManager {
    pub async fn search_discovery_entities(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: SearchDiscoveryEntitiesRequest,
    ) -> Result<Vec<DiscoveryEntity>, String> {
        self.discovery_service
            .search_discovery_entities(app, account_manager, &self.sync_manager, request)
            .await
    }

    pub async fn join_discovery_room(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: JoinDiscoveryRoomRequest,
    ) -> Result<JoinDiscoveryRoomResponse, String> {
        self.discovery_service
            .join_discovery_room(app, account_manager, &self.sync_manager, request)
            .await
    }

    pub async fn invite_user_to_room(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: InviteUserToRoomRequest,
    ) -> Result<(), String> {
        self.discovery_service
            .invite_user_to_room(app, account_manager, request)
            .await
    }

    pub async fn list_invite_targets(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: ListInviteTargetsRequest,
    ) -> Result<Vec<InviteTarget>, String> {
        self.discovery_service
            .list_invite_targets(app, account_manager, request)
            .await
    }
}

impl ShellDiscoveryService {
    pub(super) async fn search_discovery_entities(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        sync_manager: &ShellSyncManager,
        request: SearchDiscoveryEntitiesRequest,
    ) -> Result<Vec<DiscoveryEntity>, String> {
        account_manager.ensure_loaded(app).await?;
        let Some(account) = account_manager.active_account_client_loaded() else {
            return Err(String::from("No active account is available"));
        };

        let limit = bounded_discovery_limit(request.limit);
        let source = request.source.unwrap_or(DiscoverySource::All);
        let joined_rooms = joined_conversation_ids(sync_manager, &account.account_key).await?;
        let joined_spaces = joined_space_ids(sync_manager, &account.account_key).await?;

        let offset = request.offset.unwrap_or_default();
        if request.kind == DiscoveryEntityKind::User {
            if request.query.trim().is_empty() {
                return recommend_joined_room_users(&account.client, limit, offset).await;
            }

            return search_users(&account.client, &request.query, limit, offset).await;
        }

        let mut results = Vec::new();
        if matches!(
            source,
            DiscoverySource::All | DiscoverySource::MatrixRoomsInfo
        ) {
            let matrix_rooms_info_results = search_matrixrooms_info(
                request.kind,
                &request.query,
                limit,
                offset,
                &joined_rooms,
                &joined_spaces,
            )
            .await
            .unwrap_or_default();
            results.extend(matrix_rooms_info_results);
        }

        if matches!(source, DiscoverySource::All | DiscoverySource::Homeserver) {
            let homeserver_results = search_public_room_directory(
                &account.client,
                request.kind,
                &request.query,
                limit,
                offset,
                &joined_rooms,
            )
            .await
            .unwrap_or_default();
            results.extend(homeserver_results);
        }

        Ok(dedupe_discovery_entities(results, limit))
    }

    pub(super) async fn join_discovery_room(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        sync_manager: &ShellSyncManager,
        request: JoinDiscoveryRoomRequest,
    ) -> Result<JoinDiscoveryRoomResponse, String> {
        account_manager.ensure_loaded(app).await?;
        let Some(account) = account_manager.active_account_client_loaded() else {
            return Err(String::from("No active account is available"));
        };

        let room_or_alias = RoomOrAliasId::parse(&request.room_id_or_alias)
            .map_err(|error| format!("Invalid room id or alias: {error}"))?;
        let via = parse_via_servers(request.via.unwrap_or_default())?;
        let room =
            join_room_by_id_or_alias_with_timeout(&account.client, &room_or_alias, &via).await?;

        ensure_sync_in_background(app, account_manager, sync_manager);

        Ok(JoinDiscoveryRoomResponse {
            room_id: room.room_id().to_string(),
        })
    }

    pub(super) async fn invite_user_to_room(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: InviteUserToRoomRequest,
    ) -> Result<(), String> {
        account_manager.ensure_loaded(app).await?;
        let Some(account) = account_manager.active_account_client_loaded() else {
            return Err(String::from("No active account is available"));
        };
        let room_id =
            RoomId::parse(&request.room_id).map_err(|error| format!("Invalid room id: {error}"))?;
        let user_id =
            UserId::parse(&request.user_id).map_err(|error| format!("Invalid user id: {error}"))?;
        let room = account
            .client
            .get_room(&room_id)
            .ok_or_else(|| String::from("Invite target is not a joined room"))?;
        room.invite_user_by_id(&user_id)
            .await
            .map_err(|error| format!("Failed to invite user: {error}"))
    }

    pub(super) async fn list_invite_targets(
        &self,
        app: &tauri::AppHandle,
        account_manager: &AccountManager,
        request: ListInviteTargetsRequest,
    ) -> Result<Vec<InviteTarget>, String> {
        account_manager.ensure_loaded(app).await?;
        let Some(account) = account_manager.active_account_client_loaded() else {
            return Err(String::from("No active account is available"));
        };
        UserId::parse(&request.user_id).map_err(|error| format!("Invalid user id: {error}"))?;

        let conversations = ShellCacheState::cached_room_threads(
            &account.account_key,
            &account.store_dir,
            &crate::shell::types::ListRoomThreadsRequest { search_query: None },
        )
        .unwrap_or_default();
        let spaces = ShellCacheState::cached_spaces(
            &account.store_dir,
            &crate::shell::types::ListSpacesRequest { search_query: None },
        )
        .unwrap_or_default();

        Ok(invite_targets_from_summaries(
            &account.client,
            conversations,
            spaces,
        ))
    }
}

async fn joined_conversation_ids(
    sync_manager: &ShellSyncManager,
    account_key: &str,
) -> Result<HashSet<String>, String> {
    let rooms =
        snapshot_room_list_for_account(sync_manager, account_key, ShellRoomListKind::Conversations)
            .await
            .unwrap_or_default();
    Ok(rooms
        .into_iter()
        .map(|room| room.room_id().to_string())
        .collect())
}

async fn joined_space_ids(
    sync_manager: &ShellSyncManager,
    account_key: &str,
) -> Result<HashSet<String>, String> {
    let spaces =
        snapshot_room_list_for_account(sync_manager, account_key, ShellRoomListKind::Spaces)
            .await
            .unwrap_or_default();
    Ok(spaces
        .into_iter()
        .map(|room| room.room_id().to_string())
        .collect())
}

fn ensure_sync_in_background(
    app: &tauri::AppHandle,
    account_manager: &AccountManager,
    sync_manager: &ShellSyncManager,
) {
    let app = app.clone();
    let account_manager = account_manager.clone();
    let sync_manager = sync_manager.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = ensure_active_account_sync(&app, &account_manager, &sync_manager).await
        {
            eprintln!("Failed to refresh shell discovery data in background: {error}");
        }
    });
}

async fn ensure_active_account_sync(
    app: &tauri::AppHandle,
    account_manager: &AccountManager,
    sync_manager: &ShellSyncManager,
) -> Result<(), String> {
    account_manager.ensure_loaded(app).await?;
    let Some(account) = account_manager.active_account_client_loaded() else {
        sync_manager.stop_all_accounts().await;
        return Ok(());
    };

    sync_manager
        .ensure_started_for_account(app, account_manager, account)
        .await
}

async fn search_users(
    client: &matrix_sdk::Client,
    query: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<DiscoveryEntity>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let response = client
        .search_users(query, (limit + offset) as u64)
        .await
        .map_err(|error| format!("User search failed: {error}"))?;

    Ok(response
        .results
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|user| DiscoveryEntity {
            id: user.user_id.to_string(),
            kind: String::from("user"),
            title: user
                .display_name
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| user.user_id.to_string()),
            alias: Some(user.user_id.to_string()),
            description: Some(String::from("Matrix user")),
            avatar_url: user.avatar_url.map(|value| value.to_string()),
            member_count: None,
            join_rule: None,
            world_readable: None,
            source: String::from("homeserver"),
            already_joined: false,
            parent_space_labels: Vec::new(),
            via: Vec::new(),
        })
        .collect())
}

async fn recommend_joined_room_users(
    client: &matrix_sdk::Client,
    limit: usize,
    offset: usize,
) -> Result<Vec<DiscoveryEntity>, String> {
    let own_user_id = client.user_id().map(ToOwned::to_owned);
    let mut seen_user_ids = HashSet::new();
    let mut recommendations = Vec::new();
    let mut rooms = client.joined_rooms();
    rooms.sort_by_key(|room| Reverse(latest_room_activity(room)));

    for room in rooms.into_iter().take(MAX_USER_RECOMMENDATION_ROOMS) {
        let Ok(members) = room.members_no_sync(RoomMemberships::JOIN).await else {
            continue;
        };

        for member in members {
            if own_user_id
                .as_ref()
                .is_some_and(|own_user_id| member.user_id() == own_user_id)
            {
                continue;
            }

            let user_id = member.user_id().to_string();
            if !seen_user_ids.insert(user_id.clone()) {
                continue;
            }

            recommendations.push(DiscoveryEntity {
                id: user_id.clone(),
                kind: String::from("user"),
                title: member
                    .display_name()
                    .filter(|value| !value.trim().is_empty())
                    .map_or_else(|| user_id.clone(), ToOwned::to_owned),
                alias: Some(user_id),
                description: Some(format!("Member of {}", local_room_title(&room))),
                avatar_url: member.avatar_url().map(ToString::to_string),
                member_count: None,
                join_rule: None,
                world_readable: None,
                source: String::from("joined"),
                already_joined: false,
                parent_space_labels: Vec::new(),
                via: Vec::new(),
            });
        }
    }

    Ok(recommendations
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect())
}

async fn search_matrixrooms_info(
    kind: DiscoveryEntityKind,
    query: &str,
    limit: usize,
    offset: usize,
    joined_rooms: &HashSet<String>,
    joined_spaces: &HashSet<String>,
) -> Result<Vec<DiscoveryEntity>, String> {
    let mut query = query.trim();
    if query.is_empty() {
        query = match kind {
            DiscoveryEntityKind::Room => DEFAULT_ROOM_RECOMMENDATION_QUERY,
            DiscoveryEntityKind::Space => DEFAULT_SPACE_RECOMMENDATION_QUERY,
            DiscoveryEntityKind::User => return Ok(Vec::new()),
        };
    }

    let url = format!(
        "{MATRIXROOMS_INFO_BASE_URL}/search/{}/{limit}/{offset}",
        encode_path_segment(query)
    );
    let http_client = discovery_http_client()?;

    let response = http_client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("MatrixRooms.info search failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("MatrixRooms.info search failed: {error}"))?;
    let entries = response
        .json::<MatrixRoomsInfoSearchResponse>()
        .await
        .map_err(|error| format!("MatrixRooms.info response could not be parsed: {error}"))?
        .into_entries();

    Ok(entries
        .into_iter()
        .filter_map(|entry| map_matrixrooms_info_entry(kind, entry, joined_rooms, joined_spaces))
        .collect())
}

impl MatrixRoomsInfoSearchResponse {
    fn into_entries(self) -> Vec<MatrixRoomsInfoEntry> {
        match self {
            Self::List(entries) => entries,
            Self::Wrapped { rooms, results } => {
                if rooms.is_empty() {
                    results
                } else {
                    rooms
                }
            }
        }
    }
}

async fn search_public_room_directory(
    client: &matrix_sdk::Client,
    kind: DiscoveryEntityKind,
    query: &str,
    limit: usize,
    offset: usize,
    joined_rooms: &HashSet<String>,
) -> Result<Vec<DiscoveryEntity>, String> {
    let mut filter = Filter::new();
    filter.generic_search_term = (!query.trim().is_empty()).then(|| query.trim().to_owned());
    filter.room_types = match kind {
        DiscoveryEntityKind::Room => vec![RoomTypeFilter::Default],
        DiscoveryEntityKind::Space => vec![RoomTypeFilter::from(Some(RoomType::Space))],
        DiscoveryEntityKind::User => Vec::new(),
    };

    let mut request = PublicRoomsFilterRequest::new();
    request.filter = filter;
    request.limit = Some(u32::try_from(limit + offset).unwrap_or(u32::MAX).into());
    let response = client
        .public_rooms_filtered(request)
        .await
        .map_err(|error| format!("Public room directory search failed: {error}"))?;

    Ok(response
        .chunk
        .into_iter()
        .skip(offset)
        .filter_map(|description| map_public_room_description(kind, description, joined_rooms))
        .take(limit)
        .collect())
}

fn map_matrixrooms_info_entry(
    kind: DiscoveryEntityKind,
    entry: MatrixRoomsInfoEntry,
    joined_rooms: &HashSet<String>,
    joined_spaces: &HashSet<String>,
) -> Option<DiscoveryEntity> {
    let is_space = entry.room_type.as_deref().is_some_and(|value| {
        value.eq_ignore_ascii_case("m.space") || value.eq_ignore_ascii_case("space")
    });
    if kind == DiscoveryEntityKind::Space && !is_space {
        return None;
    }
    if kind == DiscoveryEntityKind::Room && is_space {
        return None;
    }

    let id = entry
        .room_id
        .or(entry.id)
        .or_else(|| entry.alias.clone())
        .unwrap_or_default();
    if id.is_empty() {
        return None;
    }

    let already_joined = if is_space {
        joined_spaces.contains(&id)
    } else {
        joined_rooms.contains(&id)
    };
    if already_joined {
        return None;
    }

    let via = discovery_via_servers(entry.server, &id, entry.alias.as_deref());
    Some(DiscoveryEntity {
        id,
        kind: if is_space { "space" } else { "room" }.to_owned(),
        title: entry
            .name
            .filter(|value| !value.trim().is_empty())
            .or_else(|| entry.alias.clone())
            .unwrap_or_else(|| String::from("Unnamed room")),
        alias: entry.alias,
        description: entry.topic,
        avatar_url: entry.avatar_url,
        member_count: entry.members,
        join_rule: Some(String::from("public")),
        world_readable: None,
        source: String::from("matrixrooms_info"),
        already_joined,
        parent_space_labels: Vec::new(),
        via,
    })
}

fn map_public_room_description(
    kind: DiscoveryEntityKind,
    description: PublicRoomsChunk,
    joined_rooms: &HashSet<String>,
) -> Option<DiscoveryEntity> {
    if joined_rooms.contains(description.room_id.as_str()) {
        return None;
    }

    Some(DiscoveryEntity {
        id: description.room_id.to_string(),
        kind: match kind {
            DiscoveryEntityKind::Room => "room",
            DiscoveryEntityKind::Space => "space",
            DiscoveryEntityKind::User => "user",
        }
        .to_owned(),
        title: description
            .name
            .or_else(|| {
                description
                    .canonical_alias
                    .as_ref()
                    .map(ToString::to_string)
            })
            .unwrap_or_else(|| description.room_id.to_string()),
        alias: description.canonical_alias.map(|value| value.to_string()),
        description: description.topic,
        avatar_url: description.avatar_url.map(|value| value.to_string()),
        member_count: Some(u64::from(description.num_joined_members)),
        join_rule: Some(join_rule_label(&description.join_rule)),
        world_readable: Some(description.world_readable),
        source: String::from("homeserver"),
        already_joined: false,
        parent_space_labels: Vec::new(),
        via: description
            .room_id
            .server_name()
            .map(|server_name| vec![server_name.to_string()])
            .unwrap_or_default(),
    })
}

fn latest_room_activity(room: &Room) -> u64 {
    room.latest_event()
        .timestamp()
        .map(|timestamp| u64::from(timestamp.0))
        .unwrap_or_default()
}

fn local_room_title(room: &Room) -> String {
    room.name()
        .or_else(|| room.canonical_alias().map(|alias| alias.to_string()))
        .unwrap_or_else(|| room.room_id().to_string())
}

fn invite_targets_from_summaries(
    client: &matrix_sdk::Client,
    conversations: Vec<RoomThreadSummary>,
    spaces: Vec<SpaceSummary>,
) -> Vec<InviteTarget> {
    let mut targets = Vec::new();

    for conversation in conversations {
        if !conversation_is_invite_candidate(&conversation)
            || !can_invite_to_room(client, &conversation.room_id)
        {
            continue;
        }

        targets.push(InviteTarget {
            room_id: conversation.room_id,
            title: conversation.title,
            description: conversation.participant_label,
            is_space: false,
        });
    }

    for space in spaces {
        if !can_invite_to_room(client, &space.space_id) {
            continue;
        }

        targets.push(InviteTarget {
            room_id: space.space_id,
            title: space.name,
            description: space.member_label,
            is_space: true,
        });
    }

    targets
}

fn can_invite_to_room(client: &matrix_sdk::Client, room_id: &str) -> bool {
    let Ok(room_id) = RoomId::parse(room_id) else {
        return false;
    };
    client
        .get_room(&room_id)
        .is_some_and(|room| matches!(room.state(), RoomState::Joined))
}

fn conversation_is_invite_candidate(conversation: &RoomThreadSummary) -> bool {
    !conversation.is_direct
}

fn dedupe_discovery_entities(entities: Vec<DiscoveryEntity>, limit: usize) -> Vec<DiscoveryEntity> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();

    for entity in entities {
        if entity.kind == "user" {
            deduped.push(entity);
            if deduped.len() >= limit {
                break;
            }
            continue;
        }

        let key = entity.alias.clone().unwrap_or_else(|| entity.id.clone());
        if !seen.insert(key) {
            continue;
        }

        deduped.push(entity);
        if deduped.len() >= limit {
            break;
        }
    }

    deduped
}

fn bounded_discovery_limit(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(DEFAULT_DISCOVERY_LIMIT)
        .clamp(1, MAX_DISCOVERY_LIMIT)
}

fn parse_via_servers(via: Vec<String>) -> Result<Vec<OwnedServerName>, String> {
    via.into_iter()
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            ServerName::parse(value.trim()).map_err(|error| format!("Invalid via server: {error}"))
        })
        .collect()
}

fn discovery_http_client() -> Result<HttpClient, String> {
    external_http_client()
        .map_err(|error| format!("Failed to build discovery HTTP client: {error}"))
}

async fn join_room_by_id_or_alias_with_timeout(
    client: &matrix_sdk::Client,
    room_or_alias: &RoomOrAliasId,
    via: &[OwnedServerName],
) -> Result<Room, String> {
    let join_future = client.join_room_by_id_or_alias(room_or_alias, via);
    let timeout_future = sleep(Duration::from_millis(DISCOVERY_JOIN_TIMEOUT_MS));
    pin_mut!(join_future);
    pin_mut!(timeout_future);

    match select(join_future, timeout_future).await {
        Either::Left((join_result, _timeout_future)) => {
            join_result.map_err(|error| format!("Failed to join room: {error}"))
        }
        Either::Right(((), _join_future)) => Err(String::from(
            "Joining this room is taking longer than expected. Try again in a moment.",
        )),
    }
}

fn discovery_via_servers(
    explicit_server: Option<String>,
    id: &str,
    alias: Option<&str>,
) -> Vec<String> {
    let mut servers = Vec::new();

    if let Some(server) = explicit_server.filter(|value| !value.trim().is_empty()) {
        push_unique_server(&mut servers, server.trim());
    }

    push_server_from_matrix_identifier(&mut servers, id);
    if let Some(alias) = alias {
        push_server_from_matrix_identifier(&mut servers, alias);
    }

    servers
}

fn push_server_from_matrix_identifier(servers: &mut Vec<String>, value: &str) {
    if let Some((_localpart, server_name)) = value.rsplit_once(':') {
        push_unique_server(servers, server_name);
    }
}

fn push_unique_server(servers: &mut Vec<String>, server_name: &str) {
    if server_name.trim().is_empty() {
        return;
    }

    if !servers.iter().any(|server| server == server_name) {
        servers.push(server_name.to_owned());
    }
}

fn join_rule_label(join_rule: &JoinRuleKind) -> String {
    join_rule.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matrixrooms_info_mapping_marks_space_and_dedupes_joined() {
        let entry = MatrixRoomsInfoEntry {
            room_id: Some(String::from("!space:example.org")),
            id: None,
            name: Some(String::from("Example Space")),
            alias: Some(String::from("#space:example.org")),
            topic: Some(String::from("A public space")),
            avatar_url: None,
            members: Some(42),
            server: Some(String::from("example.org")),
            room_type: Some(String::from("m.space")),
        };
        let joined_rooms = HashSet::new();
        let joined_spaces = HashSet::new();

        let mapped = map_matrixrooms_info_entry(
            DiscoveryEntityKind::Space,
            entry,
            &joined_rooms,
            &joined_spaces,
        )
        .expect("space should map");

        assert_eq!(mapped.kind, "space");
        assert_eq!(mapped.title, "Example Space");
        assert_eq!(mapped.via, vec![String::from("example.org")]);
    }

    #[test]
    fn matrixrooms_info_mapping_derives_via_from_room_id_and_alias() {
        let entry = MatrixRoomsInfoEntry {
            room_id: Some(String::from("!space:matrix.org")),
            id: None,
            name: Some(String::from("Matrix Space")),
            alias: Some(String::from("#space:example.org")),
            topic: None,
            avatar_url: None,
            members: None,
            server: None,
            room_type: Some(String::from("m.space")),
        };
        let joined_rooms = HashSet::new();
        let joined_spaces = HashSet::new();

        let mapped = map_matrixrooms_info_entry(
            DiscoveryEntityKind::Space,
            entry,
            &joined_rooms,
            &joined_spaces,
        )
        .expect("space should map");

        assert_eq!(
            mapped.via,
            vec![String::from("matrix.org"), String::from("example.org")]
        );
    }

    #[test]
    fn joined_room_results_are_filtered() {
        let entry = MatrixRoomsInfoEntry {
            room_id: Some(String::from("!room:example.org")),
            id: None,
            name: Some(String::from("Example Room")),
            alias: None,
            topic: None,
            avatar_url: None,
            members: None,
            server: None,
            room_type: None,
        };
        let joined_rooms = HashSet::from([String::from("!room:example.org")]);
        let joined_spaces = HashSet::new();

        let mapped = map_matrixrooms_info_entry(
            DiscoveryEntityKind::Room,
            entry,
            &joined_rooms,
            &joined_spaces,
        );

        assert!(mapped.is_none());
    }

    #[test]
    fn user_results_are_not_deduped() {
        let results = dedupe_discovery_entities(
            vec![
                DiscoveryEntity {
                    id: String::from("@one:example.org"),
                    kind: String::from("user"),
                    title: String::from("One"),
                    alias: Some(String::from("@one:example.org")),
                    description: None,
                    avatar_url: None,
                    member_count: None,
                    join_rule: None,
                    world_readable: None,
                    source: String::from("homeserver"),
                    already_joined: false,
                    parent_space_labels: Vec::new(),
                    via: Vec::new(),
                },
                DiscoveryEntity {
                    id: String::from("@one:example.org"),
                    kind: String::from("user"),
                    title: String::from("One"),
                    alias: Some(String::from("@one:example.org")),
                    description: None,
                    avatar_url: None,
                    member_count: None,
                    join_rule: None,
                    world_readable: None,
                    source: String::from("homeserver"),
                    already_joined: false,
                    parent_space_labels: Vec::new(),
                    via: Vec::new(),
                },
            ],
            10,
        );

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn invite_targets_exclude_direct_conversations() {
        let direct_conversation = RoomThreadSummary {
            room_id: String::from("!direct:example.org"),
            title: String::from("Direct"),
            preview: String::new(),
            participant_label: String::from("Direct chat"),
            last_activity_unix_ms: 0,
            last_activity_label: String::new(),
            message_count: 0,
            unread_count: 0,
            homeserver_label: String::from("example.org"),
            avatar_label: None,
            is_direct: true,
        };

        assert!(!conversation_is_invite_candidate(&direct_conversation));
    }

    #[test]
    fn path_segment_encoding_preserves_safe_bytes() {
        assert_eq!(encode_path_segment("matrix rooms"), "matrix%20rooms");
        assert_eq!(
            encode_path_segment("server:matrix.org"),
            "server%3Amatrix.org"
        );
    }
}
