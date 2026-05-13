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

import type {
  BackendDiscoveryEntity,
  BackendInviteTarget,
  DiscoveryEntity,
  InviteTarget,
} from "./types";

export function mapDiscoveryEntity(
  entity: BackendDiscoveryEntity,
): DiscoveryEntity {
  return {
    id: entity.id,
    kind: entity.kind,
    title: entity.title,
    alias: entity.alias ?? "",
    description: entity.description ?? "",
    avatarUrl: entity.avatar_url ?? "",
    memberCount: entity.member_count ?? null,
    joinRule: entity.join_rule ?? "",
    worldReadable: entity.world_readable ?? null,
    source: entity.source,
    alreadyJoined: entity.already_joined,
    parentSpaceLabels: entity.parent_space_labels,
    via: entity.via,
  };
}

export function mapInviteTarget(target: BackendInviteTarget): InviteTarget {
  return {
    roomId: target.room_id,
    title: target.title,
    description: target.description,
    isSpace: target.is_space,
  };
}

export function discoverySourceLabel(source: string): string {
  if (source === "matrixrooms_info") {
    return "MatrixRooms.info";
  }

  if (source === "homeserver") {
    return "Homeserver";
  }

  if (source === "joined") {
    return "Joined";
  }

  return "Discovery";
}
