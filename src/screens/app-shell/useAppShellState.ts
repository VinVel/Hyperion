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

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  type AccountSummary,
  type AuthenticatedShellView,
  type BackendRoomSummary,
  type BackendRoomThreadSummary,
  type BackendRoomTimeline,
  type BackendSpaceSummary,
  type RoomTimeline,
  type RoomTimelineItem,
  type RoomSummary,
  type RoomThreadSort,
  type SpaceSummary,
  filterAndSortRoomThreads,
  mapRoomSummary,
  mapRoomThreadSummary,
  mapRoomTimeline,
  mapSpaceSummary,
} from "./appShellAdapters";
import {
  type BackendGlobalSearchResponse,
  type SearchResultGroup,
  globalSearchStatusNotice,
  mapGlobalSearchResponse,
} from "./search";

const SHELL_SYNC_UPDATED_EVENT = "hyperion://shell-sync-updated";

// Room-list sync can arrive in bursts, so collection refreshes need a modest
// debounce to avoid rebuilding the whole shell too often.
const shellSyncCollectionRefreshDebounceMilliseconds = 250;
// Timeline-only updates should feel close to instant because they carry local
// echoes and active-room messages from matrix-sdk-ui Timeline subscriptions.
const shellSyncTimelineRefreshDebounceMilliseconds = 30;
// Nearby event context keeps jumps readable without over-fetching timeline history.
const roomEventContextLimit = 8;
// Timeline pages are intentionally small enough to keep refreshes responsive.
const roomTimelinePageSize = 30;
// Global search waits briefly so every keystroke does not call the backend.
const globalSearchDebounceMilliseconds = 150;
// Each search group is capped to keep the overlay compact.
const globalSearchLimitPerGroup = 4;
// Keep recently opened room views in memory so switching rooms is an immediate
// render operation while the backend refresh catches up.
const maximumInMemoryRoomSnapshots = 24;
const cachedRoomThreadsStoragePrefix = "hyperion.appShell.roomThreads";
const cachedSpacesStoragePrefix = "hyperion.appShell.spaces";
const cachedRoomSnapshotsStoragePrefix = "hyperion.appShell.roomSnapshots";
// Startup retries cover the common mobile flow where the WebView returns before
// the native Matrix client is ready.
const shellStartupRetryDelayMilliseconds = [1_000, 3_000, 7_000, 15_000];
// Keep nudging the cache/live merge path; the SDK still owns the real sync loop.
const shellCollectionRefreshIntervalMilliseconds = 15_000;

type ShellSyncUpdatedPayload = {
  account_key: string;
  changed_room_ids: string[];
  room_list_may_have_changed: boolean;
  updated_at_unix_ms: number;
};

type TimelineJumpTarget = {
  roomId: string;
  eventId: string;
};

type FeedbackMessage = {
  tone: "success" | "error" | "info" | "warning";
  text: string;
};

type SelectedRoomSnapshot = {
  summary: RoomSummary;
  timeline: RoomTimeline;
};

type RoomThread = ReturnType<typeof mapRoomThreadSummary>;

type UseAppShellStateOptions = {
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
  isLoadingOlderMessages: boolean;
  isLoadingShell: boolean;
  isSendingMessage: boolean;
  isSortMenuOpen: boolean;
  isThreadOpen: boolean;
  selectedRoomSummary: RoomSummary | null;
  selectedSpace: SpaceSummary | null;
  selectedThread: ReturnType<typeof mapRoomThreadSummary> | null;
  selectedTimeline: RoomTimeline | null;
  spaceSearchQuery: string;
  switchableAccounts: AccountSummary[];
  switchingAccountKey: string | null;
  threadSearchQuery: string;
  threadSort: RoomThreadSort;
  visibleSpaces: SpaceSummary[];
  visibleThreads: ReturnType<typeof mapRoomThreadSummary>[];
  closeThread: () => void;
  closeGlobalSearch: () => void;
  openGlobalSearch: () => void;
  openMessagesView: () => void;
  openSettingsView: () => void;
  openSpacesView: () => void;
  selectSpace: (spaceId: string) => void;
  selectSort: (sort: RoomThreadSort) => void;
  selectThread: (roomId: string) => void;
  sendMessage: () => Promise<void>;
  setComposerValue: (value: string) => void;
  setGlobalSearchQuery: (value: string) => void;
  setSpaceSearchQuery: (value: string) => void;
  setThreadSearchQuery: (value: string) => void;
  switchAccount: (nextAccount: AccountSummary) => Promise<void>;
  toggleAccountCenter: () => void;
  toggleSortMenu: () => void;
  handleGlobalSearchResult: (
    threadId?: string,
    targetView?: AuthenticatedShellView,
    eventId?: string,
  ) => void;
  loadOlderMessages: () => Promise<void>;
};

function accountScopedStorageKey(prefix: string, accountKey: string): string {
  return `${prefix}.${accountKey}`;
}

function readCachedJson<T>(storageKey: string, fallback: T): T {
  try {
    const rawValue = window.localStorage.getItem(storageKey);
    if (!rawValue) {
      return fallback;
    }

    return JSON.parse(rawValue) as T;
  } catch {
    window.localStorage.removeItem(storageKey);
    return fallback;
  }
}

function writeCachedJson<T>(storageKey: string, value: T) {
  window.localStorage.setItem(storageKey, JSON.stringify(value));
}

function cachedRoomThreadsKey(accountKey: string): string {
  return accountScopedStorageKey(cachedRoomThreadsStoragePrefix, accountKey);
}

function cachedSpacesKey(accountKey: string): string {
  return accountScopedStorageKey(cachedSpacesStoragePrefix, accountKey);
}

function cachedRoomSnapshotKey(accountKey: string, roomId: string): string {
  return accountScopedStorageKey(
    cachedRoomSnapshotsStoragePrefix,
    `${accountKey}.${roomId}`,
  );
}

function setGenericErrorFeedback(
  setFeedbackMessage: (feedback: FeedbackMessage | null) => void,
  text: string,
) {
  setFeedbackMessage({
    tone: "error",
    text,
  });
}

function fallbackRoomSummaryFromThread(thread: RoomThread): RoomSummary {
  return {
    id: thread.id,
    title: thread.title,
    participantLabel: thread.participantLabel,
    homeserverLabel: thread.homeserverLabel,
    topic: "",
    isDirect: thread.isDirect,
    canSendMessages: false,
  };
}

function emptyRoomTimeline(roomId: string): RoomTimeline {
  return {
    roomId,
    items: [],
    nextBefore: null,
    focusedEventId: null,
  };
}

function readCachedRoomSnapshot(
  accountKey: string,
  roomId: string,
): SelectedRoomSnapshot | null {
  return readCachedJson<SelectedRoomSnapshot | null>(
    cachedRoomSnapshotKey(accountKey, roomId),
    null,
  );
}

function writeCachedRoomSnapshot(
  accountKey: string,
  roomSnapshot: SelectedRoomSnapshot,
) {
  writeCachedJson(
    cachedRoomSnapshotKey(accountKey, roomSnapshot.timeline.roomId),
    roomSnapshot,
  );
}

function retainCurrentSelection<T extends { id: string }>(
  currentId: string | null,
  items: T[],
): string | null {
  if (currentId && items.some((item) => item.id === currentId)) {
    return currentId;
  }

  return null;
}

function timelineAnchorForRoom(
  roomId: string,
  timelineJumpTarget: TimelineJumpTarget | null,
): string | null {
  if (timelineJumpTarget?.roomId !== roomId) {
    return null;
  }

  if (timelineJumpTarget.eventId.trim().length === 0) {
    return null;
  }

  return timelineJumpTarget.eventId;
}

function shouldRefreshSelectedRoomFromSync(
  selectedThreadId: string | null,
  pendingRoomIds: Set<string>,
  pendingAmbiguousRoomListUpdate: boolean,
): boolean {
  if (!selectedThreadId) {
    return false;
  }

  return pendingRoomIds.has(selectedThreadId) || pendingAmbiguousRoomListUpdate;
}

function mergeOlderTimelineItems(
  currentItems: RoomTimelineItem[],
  olderItems: RoomTimelineItem[],
): RoomTimelineItem[] {
  const seenItemIds = new Set(currentItems.map((item) => item.id));
  const uniqueOlderItems = olderItems.filter(
    (item) => !seenItemIds.has(item.id),
  );

  return [...uniqueOlderItems, ...currentItems];
}

function rememberRoomSnapshot(
  currentSnapshots: Record<string, SelectedRoomSnapshot>,
  roomSnapshot: SelectedRoomSnapshot,
): Record<string, SelectedRoomSnapshot> {
  const nextSnapshots = {
    ...currentSnapshots,
    [roomSnapshot.timeline.roomId]: roomSnapshot,
  };
  const snapshotEntries = Object.entries(nextSnapshots);
  if (snapshotEntries.length <= maximumInMemoryRoomSnapshots) {
    return nextSnapshots;
  }

  const [oldestRoomId] = snapshotEntries[0] ?? [];
  if (!oldestRoomId) {
    return nextSnapshots;
  }

  const trimmedSnapshots = { ...nextSnapshots };
  delete trimmedSnapshots[oldestRoomId];
  return trimmedSnapshots;
}

export default function useAppShellState({
  activeAccount,
  onActiveAccountChange,
}: UseAppShellStateOptions): UseAppShellStateResult {
  const [activeView, setActiveView] =
    useState<AuthenticatedShellView>("messages");
  const [knownAccounts, setKnownAccounts] = useState<AccountSummary[]>([
    activeAccount,
  ]);
  const [roomThreads, setRoomThreads] = useState<RoomThread[]>(() =>
    readCachedJson(cachedRoomThreadsKey(activeAccount.account_key), []),
  );
  const [spaces, setSpaces] = useState<SpaceSummary[]>(() =>
    readCachedJson(cachedSpacesKey(activeAccount.account_key), []),
  );
  const [selectedThreadId, setSelectedThreadId] = useState<string | null>(null);
  const [selectedSpaceId, setSelectedSpaceId] = useState<string | null>(null);
  const [selectionAccountKey, setSelectionAccountKey] = useState(
    activeAccount.account_key,
  );
  const [selectedRoomSummary, setSelectedRoomSummary] =
    useState<RoomSummary | null>(null);
  const [selectedTimeline, setSelectedTimeline] = useState<RoomTimeline | null>(
    null,
  );
  const [roomSnapshotsByRoomId, setRoomSnapshotsByRoomId] = useState<
    Record<string, SelectedRoomSnapshot>
  >({});
  const [timelineJumpTarget, setTimelineJumpTarget] =
    useState<TimelineJumpTarget | null>(null);
  const [composerValue, setComposerValue] = useState("");
  const [isSendingMessage, setIsSendingMessage] = useState(false);
  const [isLoadingOlderMessages, setIsLoadingOlderMessages] = useState(false);
  const [threadSearchQuery, setThreadSearchQuery] = useState("");
  const [threadSort, setThreadSort] = useState<RoomThreadSort>("newest");
  const [isSortMenuOpen, setIsSortMenuOpen] = useState(false);
  const [spaceSearchQuery, setSpaceSearchQuery] = useState("");
  const [globalSearchQuery, setGlobalSearchQuery] = useState("");
  const [isGlobalSearchOpen, setIsGlobalSearchOpen] = useState(false);
  const [globalSearchResults, setGlobalSearchResults] = useState<
    SearchResultGroup[]
  >([]);
  const [globalSearchNotice, setGlobalSearchNotice] = useState<string | null>(
    null,
  );
  const [isAccountCenterOpen, setIsAccountCenterOpen] = useState(false);
  const [feedbackMessage, setFeedbackMessage] =
    useState<FeedbackMessage | null>(null);
  const [switchingAccountKey, setSwitchingAccountKey] = useState<string | null>(
    null,
  );
  const [isLoadingShell, setIsLoadingShell] = useState(true);
  const activeAccountKeyRef = useRef(activeAccount.account_key);

  const clearAccountRestoringFeedback = useCallback(() => {
    setFeedbackMessage(null);
  }, []);

  const refreshRoomCollections = useCallback(async () => {
    const backendThreads =
      await invoke<BackendRoomThreadSummary[]>("list_room_threads");
    clearAccountRestoringFeedback();
    const mappedThreads = backendThreads.map(mapRoomThreadSummary);
    setRoomThreads(mappedThreads);
    writeCachedJson(
      cachedRoomThreadsKey(activeAccount.account_key),
      mappedThreads,
    );
    setSelectedThreadId((currentThreadId) =>
      retainCurrentSelection(currentThreadId, mappedThreads),
    );

    void invoke<BackendSpaceSummary[]>("list_spaces")
      .then((backendSpaces) => {
        const mappedSpaces = backendSpaces.map(mapSpaceSummary);
        setSpaces(mappedSpaces);
        writeCachedJson(
          cachedSpacesKey(activeAccount.account_key),
          mappedSpaces,
        );
        setSelectedSpaceId((currentSpaceId) =>
          retainCurrentSelection(currentSpaceId, mappedSpaces),
        );
      })
      .catch(() => {});
  }, [activeAccount.account_key, clearAccountRestoringFeedback]);

  const refreshShellSnapshot = useCallback(async () => {
    try {
      const accounts = await invoke<AccountSummary[]>("list_accounts");
      setKnownAccounts(accounts);
    } catch {
      // Account-list refresh is best-effort; cached account data remains usable.
    }
    await refreshRoomCollections();
  }, [refreshRoomCollections]);

  useEffect(() => {
    let cancelled = false;
    const timeoutIds = shellStartupRetryDelayMilliseconds.map((delay) =>
      window.setTimeout(() => {
        if (cancelled) {
          return;
        }

        void refreshRoomCollections().catch(() => {});
      }, delay),
    );

    const intervalId = window.setInterval(() => {
      void refreshRoomCollections().catch(() => {});
    }, shellCollectionRefreshIntervalMilliseconds);

    return () => {
      cancelled = true;
      for (const timeoutId of timeoutIds) {
        window.clearTimeout(timeoutId);
      }
      window.clearInterval(intervalId);
    };
  }, [refreshRoomCollections]);

  const selectedThreadIdRef = useRef(selectedThreadId);

  useEffect(() => {
    selectedThreadIdRef.current = selectedThreadId;
  }, [selectedThreadId]);

  const loadSelectedRoomSnapshot = useCallback(
    async (
      roomId: string,
      anchoredEventId?: string | null,
    ): Promise<SelectedRoomSnapshot> => {
      const timelineRequest =
        anchoredEventId && anchoredEventId.trim().length > 0
          ? invoke<BackendRoomTimeline>("get_room_event_context", {
              request: {
                room_id: roomId,
                event_id: anchoredEventId,
                context_limit: roomEventContextLimit,
              },
            })
          : invoke<BackendRoomTimeline>("get_room_timeline", {
              request: { room_id: roomId, limit: roomTimelinePageSize },
            });
      const [backendSummary, backendTimeline] = await Promise.all([
        invoke<BackendRoomSummary>("get_room_summary", {
          request: { room_id: roomId },
        }),
        timelineRequest,
      ]);

      return {
        summary: mapRoomSummary(backendSummary),
        timeline: mapRoomTimeline(backendTimeline),
      };
    },
    [],
  );

  useEffect(() => {
    if (activeAccountKeyRef.current === activeAccount.account_key) {
      return;
    }

    activeAccountKeyRef.current = activeAccount.account_key;
    setSelectionAccountKey(activeAccount.account_key);
    setRoomThreads(
      readCachedJson(cachedRoomThreadsKey(activeAccount.account_key), []),
    );
    setSpaces(readCachedJson(cachedSpacesKey(activeAccount.account_key), []));
    setSelectedThreadId(null);
    setSelectedSpaceId(null);
    setSelectedRoomSummary(null);
    setSelectedTimeline(null);
    setRoomSnapshotsByRoomId({});
    setTimelineJumpTarget(null);
    setComposerValue("");
    setIsLoadingOlderMessages(false);
  }, [activeAccount.account_key]);

  useEffect(() => {
    let cancelled = false;

    async function loadShellData() {
      setIsLoadingShell(true);

      try {
        await refreshShellSnapshot();

        if (cancelled) {
          return;
        }
        setFeedbackMessage(null);
      } catch {
        // Shell startup refresh is best-effort; cached room data remains usable.
      } finally {
        if (!cancelled) {
          setIsLoadingShell(false);
        }
      }
    }

    void loadShellData();

    return () => {
      cancelled = true;
    };
  }, [activeAccount, refreshShellSnapshot]);

  useEffect(() => {
    if (selectionAccountKey !== activeAccount.account_key) {
      return;
    }

    if (!selectedThreadId) {
      setSelectedRoomSummary(null);
      setSelectedTimeline(null);
      setTimelineJumpTarget(null);
      setComposerValue("");
      setIsLoadingOlderMessages(false);
      return;
    }

    let cancelled = false;
    const roomId = selectedThreadId;

    async function loadSelectedRoom() {
      try {
        const anchoredEventId = timelineAnchorForRoom(
          roomId,
          timelineJumpTarget,
        );
        const roomSnapshot = await loadSelectedRoomSnapshot(
          roomId,
          anchoredEventId,
        );
        if (cancelled || selectedThreadIdRef.current !== roomId) {
          return;
        }

        setSelectedRoomSummary(roomSnapshot.summary);
        setSelectedTimeline(roomSnapshot.timeline);
        setRoomSnapshotsByRoomId((currentSnapshots) =>
          rememberRoomSnapshot(currentSnapshots, roomSnapshot),
        );
        writeCachedRoomSnapshot(activeAccount.account_key, roomSnapshot);
        if (!anchoredEventId) {
          void refreshRoomCollections().catch(() => {});
        }

        if (cancelled) {
          return;
        }
      } catch {
        if (!cancelled) {
          setGenericErrorFeedback(
            setFeedbackMessage,
            "Could not load this conversation.",
          );
        }
      }
    }

    void loadSelectedRoom();

    return () => {
      cancelled = true;
    };
  }, [
    activeAccount.account_key,
    loadSelectedRoomSnapshot,
    refreshRoomCollections,
    selectionAccountKey,
    selectedThreadId,
    timelineJumpTarget,
  ]);

  useEffect(() => {
    let cancelled = false;
    let collectionRefreshTimeoutId: number | null = null;
    let timelineRefreshTimeoutId: number | null = null;
    const pendingRoomIds = new Set<string>();
    let pendingAmbiguousRoomListUpdate = false;

    async function refreshSelectedRoomAfterSync(roomId: string) {
      const roomSnapshot = await loadSelectedRoomSnapshot(roomId, null);
      if (cancelled || selectedThreadIdRef.current !== roomId) {
        return;
      }

      setSelectedRoomSummary(roomSnapshot.summary);
      setSelectedTimeline(roomSnapshot.timeline);
      setRoomSnapshotsByRoomId((currentSnapshots) =>
        rememberRoomSnapshot(currentSnapshots, roomSnapshot),
      );
      writeCachedRoomSnapshot(activeAccount.account_key, roomSnapshot);
      void refreshRoomCollections().catch(() => {});
    }

    const unlistenPromise = listen<ShellSyncUpdatedPayload>(
      SHELL_SYNC_UPDATED_EVENT,
      (event) => {
        if (
          cancelled ||
          event.payload.account_key !== activeAccount.account_key
        ) {
          return;
        }

        for (const roomId of event.payload.changed_room_ids) {
          pendingRoomIds.add(roomId);
        }
        pendingAmbiguousRoomListUpdate =
          pendingAmbiguousRoomListUpdate ||
          (event.payload.room_list_may_have_changed &&
            event.payload.changed_room_ids.length === 0);

        if (event.payload.room_list_may_have_changed) {
          if (collectionRefreshTimeoutId !== null) {
            window.clearTimeout(collectionRefreshTimeoutId);
          }

          collectionRefreshTimeoutId = window.setTimeout(() => {
            void refreshRoomCollections().catch(() => {});
          }, shellSyncCollectionRefreshDebounceMilliseconds);
        }

        if (timelineRefreshTimeoutId !== null) {
          window.clearTimeout(timelineRefreshTimeoutId);
        }

        timelineRefreshTimeoutId = window.setTimeout(() => {
          if (cancelled) {
            return;
          }

          const selectedRoomMayHaveChanged = shouldRefreshSelectedRoomFromSync(
            selectedThreadId,
            pendingRoomIds,
            pendingAmbiguousRoomListUpdate,
          );

          pendingRoomIds.clear();
          pendingAmbiguousRoomListUpdate = false;

          if (
            activeView !== "messages" ||
            !selectedRoomMayHaveChanged ||
            timelineJumpTarget !== null ||
            !selectedThreadId
          ) {
            return;
          }

          void refreshSelectedRoomAfterSync(selectedThreadId).catch(() => {});
        }, shellSyncTimelineRefreshDebounceMilliseconds);
      },
    );

    return () => {
      cancelled = true;
      if (collectionRefreshTimeoutId !== null) {
        window.clearTimeout(collectionRefreshTimeoutId);
      }
      if (timelineRefreshTimeoutId !== null) {
        window.clearTimeout(timelineRefreshTimeoutId);
      }
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [
    activeAccount.account_key,
    activeView,
    loadSelectedRoomSnapshot,
    refreshRoomCollections,
    selectedThreadId,
    timelineJumpTarget,
  ]);

  useEffect(() => {
    if (!isGlobalSearchOpen) {
      setGlobalSearchResults([]);
      setGlobalSearchNotice(null);
      return;
    }

    const query = globalSearchQuery.trim();
    if (query.length === 0) {
      setGlobalSearchResults([]);
      setGlobalSearchNotice(null);
      return;
    }

    let cancelled = false;

    async function runGlobalSearch() {
      try {
        const response = await invoke<BackendGlobalSearchResponse>(
          "global_search",
          {
            request: { query, limit_per_group: globalSearchLimitPerGroup },
          },
        );

        if (cancelled) {
          return;
        }

        setGlobalSearchResults(mapGlobalSearchResponse(response));
        setGlobalSearchNotice(globalSearchStatusNotice(response));
      } catch {
        if (cancelled) {
          return;
        }

        setGenericErrorFeedback(setFeedbackMessage, "Search failed.");
        setGlobalSearchResults([]);
        setGlobalSearchNotice(null);
      }
    }

    const timeoutId = window.setTimeout(() => {
      void runGlobalSearch();
    }, globalSearchDebounceMilliseconds);

    return () => {
      cancelled = true;
      window.clearTimeout(timeoutId);
    };
  }, [globalSearchQuery, isGlobalSearchOpen]);

  const visibleThreads = useMemo(
    () => filterAndSortRoomThreads(roomThreads, threadSearchQuery, threadSort),
    [roomThreads, threadSearchQuery, threadSort],
  );
  const selectedThread =
    visibleThreads.find((thread) => thread.id === selectedThreadId) ??
    roomThreads.find((thread) => thread.id === selectedThreadId) ??
    null;
  const selectedRoomSummaryForSelectedThread =
    selectedRoomSummary?.id === selectedThreadId ? selectedRoomSummary : null;
  const selectedTimelineForSelectedThread =
    selectedTimeline?.roomId === selectedThreadId ? selectedTimeline : null;
  const visibleSpaces = useMemo(() => {
    const normalizedQuery = spaceSearchQuery.trim().toLowerCase();
    if (normalizedQuery.length === 0) {
      return spaces;
    }

    return spaces.filter((space) =>
      [space.name, space.description]
        .join(" ")
        .toLowerCase()
        .includes(normalizedQuery),
    );
  }, [spaces, spaceSearchQuery]);
  const selectedSpace =
    visibleSpaces.find((space) => space.id === selectedSpaceId) ??
    spaces.find((space) => space.id === selectedSpaceId) ??
    null;
  const isThreadOpen = activeView === "messages" && selectedThread !== null;
  const switchableAccounts = knownAccounts
    .filter((account) => account.account_key !== activeAccount.account_key)
    .sort((left, right) => left.user_id.localeCompare(right.user_id));

  const refreshRoomThreadsAfterSend = useCallback(async () => {
    const backendThreads =
      await invoke<BackendRoomThreadSummary[]>("list_room_threads");
    setRoomThreads(backendThreads.map(mapRoomThreadSummary));
  }, []);

  const openRoomAtLatest = useCallback(
    (roomId: string) => {
      const cachedRoomSnapshot =
        roomSnapshotsByRoomId[roomId] ??
        readCachedRoomSnapshot(activeAccount.account_key, roomId);
      if (cachedRoomSnapshot) {
        setSelectedRoomSummary(cachedRoomSnapshot.summary);
        setSelectedTimeline(cachedRoomSnapshot.timeline);
      } else {
        const thread = roomThreads.find((candidate) => candidate.id === roomId);
        setSelectedRoomSummary(
          thread ? fallbackRoomSummaryFromThread(thread) : null,
        );
        setSelectedTimeline(emptyRoomTimeline(roomId));
      }

      setTimelineJumpTarget(null);
      setSelectedThreadId(roomId);
      setActiveView("messages");
    },
    [activeAccount.account_key, roomSnapshotsByRoomId, roomThreads],
  );

  const openRoomAtEvent = useCallback((roomId: string, eventId: string) => {
    setTimelineJumpTarget({ roomId, eventId });
    setSelectedThreadId(roomId);
    setActiveView("messages");
  }, []);

  const reloadSelectedTimeline = useCallback(
    async (roomId: string) => {
      const roomSnapshot = await loadSelectedRoomSnapshot(roomId, null);
      if (selectedThreadIdRef.current !== roomId) {
        return;
      }

      setSelectedRoomSummary(roomSnapshot.summary);
      setSelectedTimeline(roomSnapshot.timeline);
      setRoomSnapshotsByRoomId((currentSnapshots) =>
        rememberRoomSnapshot(currentSnapshots, roomSnapshot),
      );
      writeCachedRoomSnapshot(activeAccount.account_key, roomSnapshot);
    },
    [activeAccount.account_key, loadSelectedRoomSnapshot],
  );

  const switchAccount = useCallback(
    async (nextAccount: AccountSummary) => {
      setSwitchingAccountKey(nextAccount.account_key);

      try {
        await invoke("switch_active_account", {
          accountKey: nextAccount.account_key,
        });

        const refreshedActiveAccount =
          (await invoke<AccountSummary | null>("active_account")) ??
          nextAccount;

        onActiveAccountChange(refreshedActiveAccount);
        setIsAccountCenterOpen(false);
        setFeedbackMessage(null);
        setSelectedRoomSummary(null);
        setSelectedTimeline(null);
        setGlobalSearchQuery("");
        setGlobalSearchResults([]);
        setGlobalSearchNotice(null);
        setTimelineJumpTarget(null);
        setComposerValue("");
      } catch {
        setGenericErrorFeedback(
          setFeedbackMessage,
          "Could not switch account.",
        );
      } finally {
        setSwitchingAccountKey(null);
      }
    },
    [onActiveAccountChange],
  );

  const sendMessage = useCallback(async () => {
    if (!selectedThreadId) {
      return;
    }

    const body = composerValue.trim();
    if (body.length === 0) {
      return;
    }

    setIsSendingMessage(true);

    try {
      await invoke("send_room_message", {
        request: {
          room_id: selectedThreadId,
          body,
        },
      });

      setComposerValue("");
      setTimelineJumpTarget(null);
      await Promise.all([
        reloadSelectedTimeline(selectedThreadId),
        refreshRoomThreadsAfterSend(),
      ]);
    } catch {
      setGenericErrorFeedback(setFeedbackMessage, "Message could not be sent.");
    } finally {
      setIsSendingMessage(false);
    }
  }, [
    composerValue,
    refreshRoomThreadsAfterSend,
    reloadSelectedTimeline,
    selectedThreadId,
  ]);

  const loadOlderMessages = useCallback(async () => {
    if (
      !selectedThreadId ||
      !selectedTimeline?.nextBefore ||
      isLoadingOlderMessages
    ) {
      return;
    }

    setIsLoadingOlderMessages(true);

    try {
      const backendTimeline = await invoke<BackendRoomTimeline>(
        "get_room_timeline",
        {
          request: {
            room_id: selectedThreadId,
            before: selectedTimeline.nextBefore,
            limit: roomTimelinePageSize,
          },
        },
      );
      const olderTimeline = mapRoomTimeline(backendTimeline);

      setSelectedTimeline((currentTimeline) => {
        if (
          !currentTimeline ||
          currentTimeline.roomId !== olderTimeline.roomId
        ) {
          const roomSnapshot = {
            summary: selectedRoomSummaryForSelectedThread ?? {
              id: olderTimeline.roomId,
              title: selectedThread?.title ?? "",
              participantLabel: selectedThread?.participantLabel ?? "",
              homeserverLabel: selectedThread?.homeserverLabel ?? "",
              topic: "",
              isDirect: selectedThread?.isDirect ?? false,
              canSendMessages: true,
            },
            timeline: olderTimeline,
          };
          setRoomSnapshotsByRoomId((currentSnapshots) =>
            rememberRoomSnapshot(currentSnapshots, roomSnapshot),
          );
          writeCachedRoomSnapshot(activeAccount.account_key, roomSnapshot);
          return olderTimeline;
        }

        const mergedTimeline = {
          ...currentTimeline,
          items: mergeOlderTimelineItems(
            currentTimeline.items,
            olderTimeline.items,
          ),
          nextBefore: olderTimeline.nextBefore,
        };
        const roomSnapshot = {
          summary: selectedRoomSummaryForSelectedThread ?? {
            id: mergedTimeline.roomId,
            title: selectedThread?.title ?? "",
            participantLabel: selectedThread?.participantLabel ?? "",
            homeserverLabel: selectedThread?.homeserverLabel ?? "",
            topic: "",
            isDirect: selectedThread?.isDirect ?? false,
            canSendMessages: true,
          },
          timeline: mergedTimeline,
        };
        setRoomSnapshotsByRoomId((currentSnapshots) =>
          rememberRoomSnapshot(currentSnapshots, roomSnapshot),
        );
        writeCachedRoomSnapshot(activeAccount.account_key, roomSnapshot);
        return mergedTimeline;
      });
    } catch {
      setGenericErrorFeedback(
        setFeedbackMessage,
        "Could not load older messages.",
      );
    } finally {
      setIsLoadingOlderMessages(false);
    }
  }, [
    isLoadingOlderMessages,
    activeAccount.account_key,
    selectedRoomSummaryForSelectedThread,
    selectedThread,
    selectedThreadId,
    selectedTimeline,
  ]);

  const openMessagesView = useCallback(() => {
    setActiveView("messages");
    setIsAccountCenterOpen(false);
    setIsSortMenuOpen(false);
    setSelectedSpaceId(null);
  }, []);

  const openSpacesView = useCallback(() => {
    setActiveView("spaces");
    setIsAccountCenterOpen(false);
    setIsSortMenuOpen(false);
    setSelectedThreadId(null);
  }, []);

  const openSettingsView = useCallback(() => {
    setActiveView("settings");
    setIsAccountCenterOpen(false);
    setIsSortMenuOpen(false);
  }, []);

  const handleGlobalSearchResult = useCallback(
    (
      threadId?: string,
      targetView?: AuthenticatedShellView,
      eventId?: string,
    ) => {
      setIsGlobalSearchOpen(false);
      setGlobalSearchQuery("");
      setGlobalSearchResults([]);
      setGlobalSearchNotice(null);

      if (targetView) {
        setActiveView(targetView);
      }

      if (threadId) {
        if (eventId) {
          openRoomAtEvent(threadId, eventId);
        } else {
          openRoomAtLatest(threadId);
        }
      }
    },
    [openRoomAtEvent, openRoomAtLatest],
  );

  return {
    activeView,
    composerValue,
    feedbackMessage,
    globalSearchQuery,
    globalSearchResults,
    globalSearchStatusNotice: globalSearchNotice,
    isAccountCenterOpen,
    isGlobalSearchOpen,
    isLoadingOlderMessages,
    isLoadingShell,
    isSendingMessage,
    isSortMenuOpen,
    isThreadOpen,
    selectedRoomSummary: selectedRoomSummaryForSelectedThread,
    selectedSpace,
    selectedThread,
    selectedTimeline: selectedTimelineForSelectedThread,
    spaceSearchQuery,
    switchableAccounts,
    switchingAccountKey,
    threadSearchQuery,
    threadSort,
    visibleSpaces,
    visibleThreads,
    closeGlobalSearch: () => setIsGlobalSearchOpen(false),
    closeThread: () => setSelectedThreadId(null),
    handleGlobalSearchResult,
    loadOlderMessages,
    openGlobalSearch: () => {
      setIsGlobalSearchOpen(true);
      setIsAccountCenterOpen(false);
    },
    openMessagesView,
    openSettingsView,
    openSpacesView,
    selectSort: (sort) => {
      setThreadSort(sort);
      setIsSortMenuOpen(false);
    },
    selectSpace: setSelectedSpaceId,
    selectThread: openRoomAtLatest,
    sendMessage,
    setComposerValue,
    setGlobalSearchQuery,
    setSpaceSearchQuery,
    setThreadSearchQuery,
    switchAccount,
    toggleAccountCenter: () =>
      setIsAccountCenterOpen((currentValue) => !currentValue),
    toggleSortMenu: () => setIsSortMenuOpen((currentValue) => !currentValue),
  };
}
