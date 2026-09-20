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

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryEntityKind {
    User,
    Room,
    Space,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiscoverySource {
    All,
    MatrixRoomsInfo,
    Homeserver,
}

#[derive(Debug, Deserialize)]
pub struct SearchDiscoveryEntitiesRequest {
    pub kind: DiscoveryEntityKind,
    pub query: String,
    pub source: Option<DiscoverySource>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct JoinDiscoveryRoomRequest {
    pub room_id_or_alias: String,
    pub via: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct InviteUserToRoomRequest {
    pub user_id: String,
    pub room_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ListInviteTargetsRequest {
    pub user_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiscoveryEntity {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub alias: Option<String>,
    pub description: Option<String>,
    pub avatar_url: Option<String>,
    pub member_count: Option<u64>,
    pub join_rule: Option<String>,
    pub world_readable: Option<bool>,
    pub source: String,
    pub already_joined: bool,
    pub parent_space_labels: Vec<String>,
    pub via: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JoinDiscoveryRoomResponse {
    pub room_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InviteTarget {
    pub room_id: String,
    pub title: String,
    pub description: String,
    pub is_space: bool,
}

#[derive(Debug, Deserialize)]
pub struct MatrixRoomsInfoEntry {
    #[serde(default)]
    pub room_id: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub topic: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub members: Option<u64>,
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default)]
    pub room_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum MatrixRoomsInfoSearchResponse {
    List(Vec<MatrixRoomsInfoEntry>),
    Wrapped {
        #[serde(default)]
        rooms: Vec<MatrixRoomsInfoEntry>,
        #[serde(default)]
        results: Vec<MatrixRoomsInfoEntry>,
    },
}
