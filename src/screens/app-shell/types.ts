/**
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

import {
  mapRoomThreadSummary,
  type RoomThreadKindFilter,
  type RoomThreadSort,
} from "../conversations";
import type {
  BackendRoomTimelineItem,
  RoomSummary,
  RoomTimeline,
  AccountSummary,
  AuthenticatedShellView,
  SpaceSummary,
} from "./appShellAdapters";
import type { PaginationState } from "./pagination";
import type { SearchResultGroup } from "./search";

export type ShellSyncUpdatedPayload = {
  account_key: string;
  changed_room_ids: string[];
  room_list_may_have_changed: boolean;
  updated_at_unix_ms: number;
};
export type ShellTimelineUpdatedPayload = {
  account_key: string;
  room_id: string;
  items: BackendRoomTimelineItem[];
  redacted_event_ids: string[];
  updated_at_unix_ms: number;
};
export type ShellTypingUpdatedPayload = {
  account_key: string;
  room_id: string;
  users: string[];
  updated_at_unix_ms: number;
};
export type TimelineJumpTarget = {
  roomId: string;
  eventId: string;
};
export type FeedbackMessage = {
  tone: "success" | "error" | "info" | "warning";
  text: string;
};
export type SelectedRoomSnapshot = {
  summary: RoomSummary;
  timeline: RoomTimeline;
};
export type RoomComposerDraft = {
  body: string;
  editEventId: string | null;
  replyEventId: string | null;
};
export type RoomThread = ReturnType<typeof mapRoomThreadSummary>;
export type UseAppShellStateOptions = {
  activeAccount: AccountSummary;
  onActiveAccountChange: (nextAccount: AccountSummary) => void;
};

export type UseAppShellStateResult = {
  activeView: AuthenticatedShellView;
  composerValue: string;
  feedbackMessage: FeedbackMessage | null;
  globalSearchQuery: string;
  globalSearchResults: SearchResultGroup[];
  globalSearchStatusNotice: string | null;
  isAccountCenterOpen: boolean;
  isGlobalSearchOpen: boolean;
  isDiscoveryOpen: boolean;
  isLoadingOlderMessages: boolean;
  paginationState: PaginationState;
  isLoadingShell: boolean;
  isSendingMessage: boolean;
  isSortMenuOpen: boolean;
  isThreadOpen: boolean;
  activeComposerMode: "message" | "edit" | "reply";
  selectedTypingUsers: string[];
  selectedRoomSummary: RoomSummary | null;
  selectedSpace: SpaceSummary | null;
  selectedThread: ReturnType<typeof mapRoomThreadSummary> | null;
  selectedTimeline: RoomTimeline | null;
  switchableAccounts: AccountSummary[];
  switchingAccountKey: string | null;
  threadKindFilter: RoomThreadKindFilter;
  threadSort: RoomThreadSort;
  visibleSpaces: SpaceSummary[];
  visibleThreads: ReturnType<typeof mapRoomThreadSummary>[];
  closeThread: () => void;
  closeGlobalSearch: () => void;
  closeDiscovery: () => void;
  openGlobalSearch: () => void;
  openDiscovery: () => void;
  openMessagesView: () => void;
  openSettingsView: () => void;
  openSpacesView: () => void;
  selectSpace: (spaceId: string) => void;
  selectSort: (sort: RoomThreadSort) => void;
  selectThread: (roomId: string) => void;
  sendMessage: () => Promise<void>;
  beginEditMessage: (eventId: string, body: string) => void;
  beginReplyToMessage: (eventId: string) => void;
  cancelComposerMode: () => void;
  redactMessage: (eventId: string) => Promise<void>;
  toggleReaction: (eventId: string, reactionKey: string) => Promise<void>;
  setComposerValue: (value: string) => void;
  setRoomTyping: (isTyping: boolean) => Promise<void>;
  setGlobalSearchQuery: (value: string) => void;
  setThreadKindFilter: (value: RoomThreadKindFilter) => void;
  switchAccount: (nextAccount: AccountSummary) => Promise<void>;
  handleDiscoveryInviteSent: () => void;
  handleDiscoveryError: (message: string) => void;
  handleDiscoveryJoined: () => Promise<void>;
  toggleAccountCenter: () => void;
  toggleSortMenu: () => void;
  handleGlobalSearchResult: (
    threadId?: string,
    targetView?: AuthenticatedShellView,
    eventId?: string,
  ) => void;
  loadOlderMessages: () => Promise<void>;
};
