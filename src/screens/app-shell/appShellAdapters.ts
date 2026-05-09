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

export type AccountSummary = {
  account_key: string;
  user_id: string;
  homeserver_url: string;
  is_active: boolean;
};

export type AuthenticatedShellView = "messages" | "spaces" | "settings";

export type BackendRoomSummary = {
  room_id: string;
  title: string;
  participant_label: string;
  homeserver_label: string;
  topic?: string | null;
  is_direct: boolean;
  can_send_messages: boolean;
};

export type RoomSummary = {
  id: string;
  title: string;
  participantLabel: string;
  homeserverLabel: string;
  topic: string;
  isDirect: boolean;
  canSendMessages: boolean;
};

export type BackendRoomTimelineItem = {
  event_id: string;
  sender_id: string;
  sender_display_name?: string | null;
  body: string;
  timestamp_unix_ms: number;
  is_edited?: boolean;
  is_own_message: boolean;
};

export type BackendRoomTimeline = {
  room_id: string;
  items: BackendRoomTimelineItem[];
  next_before?: string | null;
  focused_event_id?: string | null;
  redacted_event_ids?: string[];
};

export type RoomTimelineItem = {
  id: string;
  senderId: string;
  senderDisplayName: string;
  body: string;
  timestampUnixMs: number;
  timeLabel: string;
  isEdited: boolean;
  isOwnMessage: boolean;
};

export type RoomTimeline = {
  roomId: string;
  items: RoomTimelineItem[];
  nextBefore: string | null;
  focusedEventId: string | null;
  redactedEventIds: string[];
};

export type BackendSpaceSummary = {
  space_id: string;
  name: string;
  description: string;
  member_label: string;
  activity_label: string;
  accent_label?: string | null;
  is_official?: boolean;
};

export type SpaceSummary = {
  id: string;
  name: string;
  description: string;
  memberLabel: string;
  activityLabel: string;
  accentLabel: string;
  isOfficial?: boolean;
};

export function mapRoomSummary(
  backendSummary: BackendRoomSummary,
): RoomSummary {
  return {
    id: backendSummary.room_id,
    title: backendSummary.title,
    participantLabel: backendSummary.participant_label,
    homeserverLabel: backendSummary.homeserver_label,
    topic: backendSummary.topic ?? "",
    isDirect: backendSummary.is_direct,
    canSendMessages: backendSummary.can_send_messages,
  };
}

export function mapRoomTimeline(
  backendTimeline: BackendRoomTimeline,
): RoomTimeline {
  return {
    roomId: backendTimeline.room_id,
    items: backendTimeline.items.map((item) => ({
      id: item.event_id,
      senderId: item.sender_id,
      senderDisplayName: item.sender_display_name?.trim() || item.sender_id,
      body: item.body,
      timestampUnixMs: item.timestamp_unix_ms,
      timeLabel: formatTimelineTime(item.timestamp_unix_ms),
      isEdited: item.is_edited ?? false,
      isOwnMessage: item.is_own_message,
    })),
    nextBefore: backendTimeline.next_before ?? null,
    focusedEventId: backendTimeline.focused_event_id ?? null,
    redactedEventIds: backendTimeline.redacted_event_ids ?? [],
  };
}

export function mapSpaceSummary(
  backendSpace: BackendSpaceSummary,
): SpaceSummary {
  return {
    id: backendSpace.space_id,
    name: backendSpace.name,
    description: backendSpace.description,
    memberLabel: backendSpace.member_label,
    activityLabel: backendSpace.activity_label,
    accentLabel:
      backendSpace.accent_label?.trim() ||
      backendSpace.name.slice(0, 1).toUpperCase() ||
      "S",
    isOfficial: backendSpace.is_official,
  };
}

export function accountInitials(account: AccountSummary): string {
  const sanitizedUserId = account.user_id.replace(/^@/, "");
  const [leadingSegment = "H"] = sanitizedUserId.split(":");
  return leadingSegment.slice(0, 2).toUpperCase();
}

function formatTimelineTime(timestampUnixMs: number): string {
  if (timestampUnixMs <= 0) {
    return "";
  }

  const date = new Date(timestampUnixMs);
  return date.toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}
