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

export type RoomThreadSort =
  "newest" | "oldest" | "mostMessages" | "alphabetical";

export type RoomThreadKindFilter = "direct" | "group";

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
    avatarLabel:
      backendThread.avatar_label?.trim() ||
      backendThread.title.slice(0, 1).toUpperCase() ||
      "R",
    homeserverLabel: backendThread.homeserver_label,
    isDirect: backendThread.is_direct,
  };
}

export function filterAndSortRoomThreads(
  threads: RoomThreadSummary[],
  sort: RoomThreadSort,
  kindFilter: RoomThreadKindFilter,
): RoomThreadSummary[] {
  const filteredThreads = threads.filter(
    (thread) => thread.isDirect === (kindFilter === "direct"),
  );
  const sortedThreads = [...filteredThreads];
  sortedThreads.sort((left, right) => {
    if (sort === "alphabetical") {
      return left.title.localeCompare(right.title);
    }

    if (sort === "mostMessages") {
      return right.messageCount - left.messageCount;
    }

    if (sort === "oldest") {
      return left.lastActivityOrder - right.lastActivityOrder;
    }

    return right.lastActivityOrder - left.lastActivityOrder;
  });

  return sortedThreads;
}

export const roomThreadSortLabels: Record<RoomThreadSort, string> = {
  newest: "Newest activity",
  oldest: "Oldest activity",
  mostMessages: "Most messages",
  alphabetical: "Alphabetical",
};

// Labels are shared by the room-list switch and state so the copy stays aligned.
export const roomThreadKindFilterLabels: Record<RoomThreadKindFilter, string> =
  {
    direct: "Direct Chats",
    group: "Group Chats",
  };
