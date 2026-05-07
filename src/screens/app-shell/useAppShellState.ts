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
// The last opened room is UI navigation state, so keep it in browser storage
// per Matrix account instead of making the backend infer it from activity.
const appShellSelectionStoragePrefix = "hyperion.appShell.selection";
// Keep recently opened room views in memory so switching rooms is an immediate
// render operation while the backend refresh catches up.
const maximumInMemoryRoomSnapshots = 24;

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
  tone: "success" | "error" | "info";
  text: string;
};

type AccountShellSelection = {
  threadId: string | null;
  spaceId: string | null;
};

type SelectedRoomSnapshot = {
  summary: RoomSummary;
  timeline: RoomTimeline;
};

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

function getErrorMessage(error: unknown): string {
  if (typeof error === "string" && error.trim().length > 0) {
    return error;
  }

  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message;
  }

  return "Something went wrong while contacting the native shell service.";
}

function retainCurrentSelectionOrDefault<T extends { id: string }>(
  currentId: string | null,
  items: T[],
): string | null {
  if (currentId && items.some((item) => item.id === currentId)) {
    return currentId;
  }

  return items[0]?.id ?? null;
}

function storedSelectionKey(accountKey: string): string {
  return `${appShellSelectionStoragePrefix}.${accountKey}`;
}

function readStoredSelection(accountKey: string): AccountShellSelection {
  try {
    const rawValue = window.localStorage.getItem(
      storedSelectionKey(accountKey),
    );
    if (!rawValue) {
      return { threadId: null, spaceId: null };
    }

    const parsedValue = JSON.parse(rawValue) as Partial<AccountShellSelection>;
    return {
      threadId:
        typeof parsedValue.threadId === "string" ? parsedValue.threadId : null,
      spaceId:
        typeof parsedValue.spaceId === "string" ? parsedValue.spaceId : null,
    };
  } catch {
    return { threadId: null, spaceId: null };
  }
}

function writeStoredSelection(
  accountKey: string,
  selection: AccountShellSelection,
) {
  window.localStorage.setItem(
    storedSelectionKey(accountKey),
    JSON.stringify(selection),
  );
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
  const [roomThreads, setRoomThreads] = useState<
    ReturnType<typeof mapRoomThreadSummary>[]
  >([]);
  const [spaces, setSpaces] = useState<SpaceSummary[]>([]);
  const initialSelection = readStoredSelection(activeAccount.account_key);
  const [selectedThreadId, setSelectedThreadId] = useState<string | null>(
    initialSelection.threadId,
  );
  const [selectedSpaceId, setSelectedSpaceId] = useState<string | null>(
    initialSelection.spaceId,
  );
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

  const refreshRoomCollections = useCallback(async () => {
    const backendThreads =
      await invoke<BackendRoomThreadSummary[]>("list_room_threads");
    const mappedThreads = backendThreads.map(mapRoomThreadSummary);
    setRoomThreads(mappedThreads);
    setSelectedThreadId((currentThreadId) =>
      retainCurrentSelectionOrDefault(currentThreadId, mappedThreads),
    );

    void invoke<BackendSpaceSummary[]>("list_spaces")
      .then((backendSpaces) => {
        const mappedSpaces = backendSpaces.map(mapSpaceSummary);
        setSpaces(mappedSpaces);
        setSelectedSpaceId((currentSpaceId) =>
          retainCurrentSelectionOrDefault(currentSpaceId, mappedSpaces),
        );
      })
      .catch((error) => {
        setFeedbackMessage({
          tone: "error",
          text: getErrorMessage(error),
        });
      });
  }, []);

  const refreshShellSnapshot = useCallback(async () => {
    const accounts = await invoke<AccountSummary[]>("list_accounts");
    setKnownAccounts(accounts);
    await refreshRoomCollections();
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
    if (
      activeAccountKeyRef.current !== activeAccount.account_key ||
      selectionAccountKey !== activeAccount.account_key
    ) {
      return;
    }

    writeStoredSelection(activeAccount.account_key, {
      threadId: selectedThreadId,
      spaceId: selectedSpaceId,
    });
  }, [
    activeAccount.account_key,
    selectedSpaceId,
    selectedThreadId,
    selectionAccountKey,
  ]);

  useEffect(() => {
    if (activeAccountKeyRef.current === activeAccount.account_key) {
      return;
    }

    const nextSelection = readStoredSelection(activeAccount.account_key);

    activeAccountKeyRef.current = activeAccount.account_key;
    setSelectionAccountKey(activeAccount.account_key);
    setSelectedThreadId(nextSelection.threadId);
    setSelectedSpaceId(nextSelection.spaceId);
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
      } catch (error) {
        if (!cancelled) {
          setFeedbackMessage({
            tone: "error",
            text: getErrorMessage(error),
          });
          setRoomThreads([]);
          setSpaces([]);
        }
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
        if (!anchoredEventId) {
          void refreshRoomCollections().catch((error) => {
            if (!cancelled) {
              setFeedbackMessage({
                tone: "error",
                text: getErrorMessage(error),
              });
            }
          });
        }

        if (cancelled) {
          return;
        }
      } catch (error) {
        if (!cancelled) {
          setFeedbackMessage({
            tone: "error",
            text: getErrorMessage(error),
          });
          setSelectedRoomSummary(null);
          setSelectedTimeline(null);
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
      void refreshRoomCollections().catch((error) => {
        if (!cancelled) {
          setFeedbackMessage({
            tone: "error",
            text: getErrorMessage(error),
          });
        }
      });
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
            void refreshRoomCollections().catch((error) => {
              if (!cancelled) {
                setFeedbackMessage({
                  tone: "error",
                  text: getErrorMessage(error),
                });
              }
            });
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

          void refreshSelectedRoomAfterSync(selectedThreadId).catch((error) => {
            if (!cancelled) {
              setFeedbackMessage({
                tone: "error",
                text: getErrorMessage(error),
              });
            }
          });
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
      } catch (error) {
        if (cancelled) {
          return;
        }

        setFeedbackMessage({
          tone: "error",
          text: getErrorMessage(error),
        });
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
      const cachedRoomSnapshot = roomSnapshotsByRoomId[roomId];
      if (cachedRoomSnapshot) {
        setSelectedRoomSummary(cachedRoomSnapshot.summary);
        setSelectedTimeline(cachedRoomSnapshot.timeline);
      } else {
        setSelectedRoomSummary(null);
        setSelectedTimeline(null);
      }

      setTimelineJumpTarget(null);
      setSelectedThreadId(roomId);
      setActiveView("messages");
    },
    [roomSnapshotsByRoomId],
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
    },
    [loadSelectedRoomSnapshot],
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
      } catch (error) {
        setFeedbackMessage({
          tone: "error",
          text: getErrorMessage(error),
        });
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
    } catch (error) {
      setFeedbackMessage({
        tone: "error",
        text: getErrorMessage(error),
      });
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
          setRoomSnapshotsByRoomId((currentSnapshots) =>
            rememberRoomSnapshot(currentSnapshots, {
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
            }),
          );
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
        setRoomSnapshotsByRoomId((currentSnapshots) =>
          rememberRoomSnapshot(currentSnapshots, {
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
          }),
        );
        return mergedTimeline;
      });
    } catch (error) {
      setFeedbackMessage({
        tone: "error",
        text: getErrorMessage(error),
      });
    } finally {
      setIsLoadingOlderMessages(false);
    }
  }, [
    isLoadingOlderMessages,
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
