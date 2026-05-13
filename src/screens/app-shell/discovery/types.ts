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

export type DiscoveryKind = "user" | "room" | "space";
export type DiscoverySource = "all" | "matrix_rooms_info" | "homeserver";

export type BackendDiscoveryEntity = {
  id: string;
  kind: DiscoveryKind;
  title: string;
  alias?: string | null;
  description?: string | null;
  avatar_url?: string | null;
  member_count?: number | null;
  join_rule?: string | null;
  world_readable?: boolean | null;
  source: string;
  already_joined: boolean;
  parent_space_labels: string[];
  via: string[];
};

export type DiscoveryEntity = {
  id: string;
  kind: DiscoveryKind;
  title: string;
  alias: string;
  description: string;
  avatarUrl: string;
  memberCount: number | null;
  joinRule: string;
  worldReadable: boolean | null;
  source: string;
  alreadyJoined: boolean;
  parentSpaceLabels: string[];
  via: string[];
};

export type BackendInviteTarget = {
  room_id: string;
  title: string;
  description: string;
  is_space: boolean;
};

export type InviteTarget = {
  roomId: string;
  title: string;
  description: string;
  isSpace: boolean;
};
