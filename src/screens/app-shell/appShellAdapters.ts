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

export type AuthenticatedShellView = 'messages' | 'spaces' | 'settings';

export type RoomThreadSort =
  | 'newest'
  | 'oldest'
  | 'mostMessages'
  | 'alphabetical';

export type BackendRoomThreadSummary = {
  room_id: string;
  title: string;
  preview: string;
  participant_label: string;
  last_activity_unix_ms: number;
  last_activity_label: string;
  message_count: number;
  unread_count: number;
  homeserver_label: string;
  avatar_label?: string | null;
  is_direct: boolean;
};

export type RoomThreadSummary = {
  id: string;
  title: string;
  preview: string;
  participantLabel: string;
  lastActivityLabel: string;
  lastActivityOrder: number;
  messageCount: number;
  unreadCount: number;
  avatarLabel: string;
  homeserverLabel: string;
  isDirect: boolean;
};

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

export type BackendGlobalSearchResponse = {
  rooms: Array<{
    room_id: string;
    title: string;
    description: string;
  }>;
  spaces: Array<{
    space_id: string;
    title: string;
    description: string;
  }>;
  messages: Array<{
    result_id: string;
    room_id: string;
    title: string;
    description: string;
    event_id?: string | null;
  }>;
};

export type SearchResultGroup = {
  title: string;
  items: Array<{
    id: string;
    title: string;
    description: string;
    targetView: AuthenticatedShellView;
    threadId?: string;
    eventId?: string;
  }>;
};

export function mapRoomThreadSummary(
  backendThread: BackendRoomThreadSummary,
): RoomThreadSummary {
  return {
    id: backendThread.room_id,
    title: backendThread.title,
    preview: backendThread.preview,
    participantLabel: backendThread.participant_label,
    lastActivityLabel: backendThread.last_activity_label,
    lastActivityOrder: backendThread.last_activity_unix_ms,
    messageCount: backendThread.message_count,
    unreadCount: backendThread.unread_count,
    avatarLabel: backendThread.avatar_label?.trim() || backendThread.title.slice(0, 1).toUpperCase() || 'R',
    homeserverLabel: backendThread.homeserver_label,
    isDirect: backendThread.is_direct,
  };
}

export function mapRoomSummary(backendSummary: BackendRoomSummary): RoomSummary {
  return {
    id: backendSummary.room_id,
    title: backendSummary.title,
    participantLabel: backendSummary.participant_label,
    homeserverLabel: backendSummary.homeserver_label,
    topic: backendSummary.topic ?? '',
    isDirect: backendSummary.is_direct,
    canSendMessages: backendSummary.can_send_messages,
  };
}

export function mapRoomTimeline(backendTimeline: BackendRoomTimeline): RoomTimeline {
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
  };
}

export function mapSpaceSummary(backendSpace: BackendSpaceSummary): SpaceSummary {
  return {
    id: backendSpace.space_id,
    name: backendSpace.name,
    description: backendSpace.description,
    memberLabel: backendSpace.member_label,
    activityLabel: backendSpace.activity_label,
    accentLabel: backendSpace.accent_label?.trim() || backendSpace.name.slice(0, 1).toUpperCase() || 'S',
    isOfficial: backendSpace.is_official,
  };
}

export function mapGlobalSearchResponse(
  response: BackendGlobalSearchResponse,
): SearchResultGroup[] {
  return [
    {
      title: 'Rooms',
      items: response.rooms.map((item) => ({
        id: item.room_id,
        title: item.title,
        description: item.description,
        targetView: 'messages' as const,
        threadId: item.room_id,
      })),
    },
    {
      title: 'Spaces',
      items: response.spaces.map((item) => ({
        id: item.space_id,
        title: item.title,
        description: item.description,
        targetView: 'spaces' as const,
      })),
    },
    {
      title: 'Messages',
      items: response.messages.map((item) => ({
        id: item.result_id,
        title: item.title,
        description: item.description,
        targetView: 'messages' as const,
        threadId: item.room_id,
        eventId: item.event_id ?? undefined,
      })),
    },
  ].filter((group) => group.items.length > 0);
}

export function filterAndSortRoomThreads(
  threads: RoomThreadSummary[],
  searchQuery: string,
  sort: RoomThreadSort,
): RoomThreadSummary[] {
  const normalizedQuery = searchQuery.trim().toLowerCase();
  const filteredThreads = normalizedQuery.length === 0
    ? threads
    : threads.filter((thread) =>
        [
          thread.title,
          thread.preview,
          thread.participantLabel,
          thread.homeserverLabel,
        ]
          .join(' ')
          .toLowerCase()
          .includes(normalizedQuery),
      );

  const sortedThreads = [...filteredThreads];
  sortedThreads.sort((left, right) => {
    if (sort === 'alphabetical') {
      return left.title.localeCompare(right.title);
    }

    if (sort === 'mostMessages') {
      return right.messageCount - left.messageCount;
    }

    if (sort === 'oldest') {
      return left.lastActivityOrder - right.lastActivityOrder;
    }

    return right.lastActivityOrder - left.lastActivityOrder;
  });

  return sortedThreads;
}

export function accountInitials(account: AccountSummary): string {
  const sanitizedUserId = account.user_id.replace(/^@/, '');
  const [leadingSegment = 'H'] = sanitizedUserId.split(':');
  return leadingSegment.slice(0, 2).toUpperCase();
}

export const roomThreadSortLabels: Record<RoomThreadSort, string> = {
  newest: 'Newest activity',
  oldest: 'Oldest activity',
  mostMessages: 'Most messages',
  alphabetical: 'Alphabetical',
};

function formatTimelineTime(timestampUnixMs: number): string {
  if (timestampUnixMs <= 0) {
    return '';
  }

  const date = new Date(timestampUnixMs);
  return date.toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
}
