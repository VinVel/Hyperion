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
  type BackendRoomTimelinePaginationResponse,
  type BackendRoomSummary,
  type BackendRoomTimeline,
  type BackendSpaceSummary,
  type RoomTimeline,
  type RoomSummary,
  type SpaceSummary,
  mapRoomSummary,
  mapRoomTimeline,
  mapRoomTimelinePaginationResponse,
  mapSpaceSummary,
} from "./appShellAdapters";
import {
  type BackendRoomThreadSummary,
  type RoomThreadKindFilter,
  type RoomThreadSort,
  filterAndSortRoomThreads,
  mapRoomThreadSummary,
} from "../conversations";
import {
  type BackendGlobalSearchResponse,
  type SearchResultGroup,
  globalSearchStatusNotice,
  mapGlobalSearchResponse,
} from "./search";
import { outboundMessageContentFromMarkdown } from "./outboundMarkdown";
import {
  durableRoomTimeline,
  emptyRoomTimeline,
  mergeTimelineRefresh,
  normalizeRoomTimeline,
  prependTimelinePage,
  roomTimelineFromUpdatePayload,
  timelineAnchorForRoom,
} from "./timeline/helpers";
import { logPaginationDiagnostic } from "./paginationDiagnostics";
import {
  createPaginationRequestId,
  idlePaginationState,
  paginationBackoffDelayMilliseconds,
  paginationCanAutomaticallyContinue,
  paginationContextForTimeline,
  paginationErrorIsRateLimited,
  paginationIsLoading,
  paginationStateKey,
  type PaginationContext,
  type PaginationState,
} from "./pagination";
import {
  FeedbackMessage,
  RoomThread,
  ShellTimelineUpdatedPayload,
  SelectedRoomSnapshot,
  TimelineJumpTarget,
  RoomComposerDraft,
  UseAppShellStateOptions,
  UseAppShellStateResult,
  ShellSyncUpdatedPayload,
  ShellTypingUpdatedPayload,
} from "./types";
import {
  readCachedJson,
  cachedRoomSnapshotKey,
  writeCachedJson,
  cachedRoomThreadsKey,
  cachedSpacesKey,
} from "./cache";

const SHELL_SYNC_UPDATED_EVENT = "hyperion://shell-sync-updated";
const SHELL_TIMELINE_UPDATED_EVENT = "hyperion://shell-timeline-updated";
const SHELL_TYPING_UPDATED_EVENT = "hyperion://shell-typing-updated";

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
// Pagination click loading is guarded so a broken request cannot strand the UI.
const paginationLoadingTimeoutMilliseconds = 3_000;
// Global search waits briefly so every keystroke does not call the backend.
const globalSearchDebounceMilliseconds = 150;
// Each search group is capped to keep the overlay compact.
const globalSearchLimitPerGroup = 4;
// Keep recently opened room views in memory so switching rooms is an immediate
// render operation while the backend refresh catches up.
const maximumInMemoryRoomSnapshots = 24;
export const cachedRoomThreadsStoragePrefix = "hyperion.appShell.roomThreads";
export const cachedSpacesStoragePrefix = "hyperion.appShell.spaces";
export const cachedRoomSnapshotsStoragePrefix =
  "hyperion.appShell.roomSnapshots";
// Startup retries cover the common mobile flow where the WebView returns before
// the native Matrix client is ready.
const shellStartupRetryDelayMilliseconds = [1_000, 3_000, 7_000, 15_000];
// Low-frequency recovery guard for missed collection events. Sync correctness
// must come from backend coordinator events, not this timer.
const shellCollectionRecoveryRefreshIntervalMilliseconds = 15_000;
// Stop our typing notice shortly after the composer becomes idle.
const roomTypingIdleMilliseconds = 1_500;
export function accountScopedStorageKey(
  prefix: string,
  accountKey: string,
): string {
  return `${prefix}.${accountKey}`;
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

function readCachedRoomSnapshot(
  accountKey: string,
  roomId: string,
): SelectedRoomSnapshot | null {
  const cachedSnapshot = readCachedJson<SelectedRoomSnapshot | null>(
    cachedRoomSnapshotKey(accountKey, roomId),
    null,
  );
  if (!cachedSnapshot) {
    logPaginationDiagnostic("pagination.restart_restore_check", {
      accountKey,
      roomId,
      cachedCountOnRoomLoad: 0,
      localStorageCountOnRoomLoad: 0,
      backendReturnedCountOnRoomLoad: "pending",
    });
    return null;
  }

  const normalizedSnapshot = {
    ...cachedSnapshot,
    timeline: durableRoomTimeline(
      normalizeRoomTimeline(cachedSnapshot.timeline),
    ),
  };
  logPaginationDiagnostic("pagination.restart_restore_check", {
    accountKey,
    roomId,
    cachedCountOnRoomLoad: "backend_pending",
    localStorageCountOnRoomLoad: normalizedSnapshot.timeline.items.length,
    backendReturnedCountOnRoomLoad: "pending",
  });
  return normalizedSnapshot;
}

function writeCachedRoomSnapshot(
  accountKey: string,
  roomSnapshot: SelectedRoomSnapshot,
) {
  const normalizedRoomSnapshot = {
    ...roomSnapshot,
    timeline: durableRoomTimeline(normalizeRoomTimeline(roomSnapshot.timeline)),
  };

  writeCachedJson(
    cachedRoomSnapshotKey(accountKey, normalizedRoomSnapshot.timeline.roomId),
    normalizedRoomSnapshot,
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

function emptyComposerDraft(): RoomComposerDraft {
  return {
    body: "",
    editEventId: null,
    replyEventId: null,
  };
}

function rememberRoomSnapshot(
  currentSnapshots: Record<string, SelectedRoomSnapshot>,
  roomSnapshot: SelectedRoomSnapshot,
): Record<string, SelectedRoomSnapshot> {
  const normalizedRoomSnapshot = {
    ...roomSnapshot,
    timeline: normalizeRoomTimeline(roomSnapshot.timeline),
  };
  const nextSnapshots = {
    ...currentSnapshots,
    [normalizedRoomSnapshot.timeline.roomId]: normalizedRoomSnapshot,
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
  const composerValueRef = useRef(composerValue);
  composerValueRef.current = composerValue;
  const [composerDraftsByRoomId, setComposerDraftsByRoomId] = useState<
    Record<string, RoomComposerDraft>
  >({});
  const [typingUsersByRoomId, setTypingUsersByRoomId] = useState<
    Record<string, string[]>
  >({});
  const isSendingMessage = false;
  const [paginationStatesByKey, setPaginationStatesByKey] = useState<
    Record<string, PaginationState>
  >({});
  const [threadKindFilter, setThreadKindFilter] =
    useState<RoomThreadKindFilter>("direct");
  const [threadSort, setThreadSort] = useState<RoomThreadSort>("newest");
  const [isSortMenuOpen, setIsSortMenuOpen] = useState(false);
  const [globalSearchQuery, setGlobalSearchQuery] = useState("");
  const [isGlobalSearchOpen, setIsGlobalSearchOpen] = useState(false);
  const [isDiscoveryOpen, setIsDiscoveryOpen] = useState(false);
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
  const sendMessageInFlightRef = useRef(false);
  const paginationStatesByKeyRef = useRef<Record<string, PaginationState>>({});
  const loadOlderMessagesRef = useRef<(() => Promise<void>) | null>(null);
  const paginationRetryCountsRef = useRef<Record<string, number>>({});
  const paginationRetryTimeoutsRef = useRef<Record<string, number>>({});
  // A pagination response must be the only timeline model update while
  // Virtuoso applies its prepend index shift. SDK refreshes are retained and
  // merged immediately after that atomic presentation update.
  const paginationPresentationRoomIdRef = useRef<string | null>(null);
  const deferredTimelineRefreshRef = useRef<SelectedRoomSnapshot | null>(null);
  const composerDraftsByRoomIdRef = useRef<Record<string, RoomComposerDraft>>(
    {},
  );

  const updateComposerDrafts = useCallback(
    (
      updateDrafts: (
        currentDrafts: Record<string, RoomComposerDraft>,
      ) => Record<string, RoomComposerDraft>,
    ) => {
      const nextDrafts = updateDrafts(composerDraftsByRoomIdRef.current);
      composerDraftsByRoomIdRef.current = nextDrafts;
      setComposerDraftsByRoomId(nextDrafts);
    },
    [],
  );

  const updatePaginationState = useCallback(
    (context: PaginationContext, nextState: PaginationState) => {
      const stateKey = paginationStateKey(context);
      const previousState =
        paginationStatesByKeyRef.current[stateKey] ?? idlePaginationState;
      const nextStates = {
        ...paginationStatesByKeyRef.current,
        [stateKey]: nextState,
      };
      paginationStatesByKeyRef.current = nextStates;
      setPaginationStatesByKey(nextStates);
      logPaginationDiagnostic("pagination.ui.state_update", {
        accountKey: context.accountKey,
        roomId: context.roomId,
        timelineContext: context.timelineContext,
        previousStatus: previousState.status,
        nextStatus: nextState.status,
        loading: paginationIsLoading(nextState),
        requestId:
          nextState.status === "loading" ? nextState.requestId : undefined,
      });
    },
    [],
  );

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
    }, shellCollectionRecoveryRefreshIntervalMilliseconds);

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
      const mappedTimeline = normalizeRoomTimeline(
        mapRoomTimeline(backendTimeline),
      );
      logPaginationDiagnostic("pagination.restart_restore_check", {
        accountKey: activeAccount.account_key,
        roomId,
        cachedCountOnRoomLoad: "backend_reported_separately",
        localStorageCountOnRoomLoad:
          readCachedRoomSnapshot(activeAccount.account_key, roomId)?.timeline
            .items.length ?? 0,
        backendReturnedCountOnRoomLoad: mappedTimeline.items.length,
      });

      return {
        summary: mapRoomSummary(backendSummary),
        timeline: mappedTimeline,
      };
    },
    [activeAccount.account_key],
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
    updateComposerDrafts(() => ({}));
    setTypingUsersByRoomId({});
    paginationStatesByKeyRef.current = {};
    paginationRetryCountsRef.current = {};
    for (const timeoutId of Object.values(paginationRetryTimeoutsRef.current)) {
      window.clearTimeout(timeoutId);
    }
    paginationRetryTimeoutsRef.current = {};
    setPaginationStatesByKey({});
  }, [activeAccount.account_key, updateComposerDrafts]);

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
      if (paginationPresentationRoomIdRef.current === roomId) {
        deferredTimelineRefreshRef.current = roomSnapshot;
        return;
      }
      setSelectedTimeline((currentTimeline) => {
        const mergedTimeline = mergeTimelineRefresh(
          currentTimeline,
          roomSnapshot.timeline,
        );
        const mergedRoomSnapshot = {
          summary: roomSnapshot.summary,
          timeline: mergedTimeline,
        };
        setRoomSnapshotsByRoomId((currentSnapshots) =>
          rememberRoomSnapshot(currentSnapshots, mergedRoomSnapshot),
        );
        writeCachedRoomSnapshot(activeAccount.account_key, mergedRoomSnapshot);
        return mergedTimeline;
      });
      void refreshRoomCollections().catch(() => {});
    }

    function applyLiveTimelineUpdate(payload: ShellTimelineUpdatedPayload) {
      if (
        cancelled ||
        payload.account_key !== activeAccount.account_key ||
        activeView !== "messages" ||
        timelineJumpTarget !== null ||
        selectedThreadIdRef.current !== payload.room_id
      ) {
        return;
      }

      const refreshedTimeline = roomTimelineFromUpdatePayload(payload);
      if (paginationPresentationRoomIdRef.current === payload.room_id) {
        const matchingThread = roomThreads.find(
          (thread) => thread.id === payload.room_id,
        );
        const summary =
          selectedRoomSummary?.id === payload.room_id
            ? selectedRoomSummary
            : matchingThread
              ? fallbackRoomSummaryFromThread(matchingThread)
              : null;
        if (summary) {
          deferredTimelineRefreshRef.current = {
            summary,
            timeline: refreshedTimeline,
          };
          return;
        }
      }
      setSelectedTimeline((currentTimeline) => {
        const mergedTimeline = mergeTimelineRefresh(
          currentTimeline,
          refreshedTimeline,
        );
        const selectedRoomSummaryForPayload =
          selectedRoomSummary?.id === payload.room_id
            ? selectedRoomSummary
            : null;
        const selectedThreadForPayload =
          roomThreads.find((thread) => thread.id === payload.room_id) ?? null;
        if (selectedRoomSummaryForPayload || selectedThreadForPayload) {
          let summary: RoomSummary;
          if (selectedRoomSummaryForPayload) {
            summary = selectedRoomSummaryForPayload;
          } else if (selectedThreadForPayload) {
            summary = fallbackRoomSummaryFromThread(selectedThreadForPayload);
          } else {
            return mergedTimeline;
          }
          const roomSnapshot = {
            summary,
            timeline: mergedTimeline,
          };
          setRoomSnapshotsByRoomId((currentSnapshots) =>
            rememberRoomSnapshot(currentSnapshots, roomSnapshot),
          );
          writeCachedRoomSnapshot(activeAccount.account_key, roomSnapshot);
        }
        return mergedTimeline;
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
    const unlistenTimelinePromise = listen<ShellTimelineUpdatedPayload>(
      SHELL_TIMELINE_UPDATED_EVENT,
      (event) => {
        applyLiveTimelineUpdate(event.payload);
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
      void unlistenTimelinePromise.then((unlisten) => unlisten());
    };
  }, [
    activeAccount.account_key,
    activeView,
    loadSelectedRoomSnapshot,
    refreshRoomCollections,
    roomThreads,
    selectedRoomSummary,
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

  useEffect(() => {
    let cancelled = false;

    const unlistenPromise = listen<ShellTypingUpdatedPayload>(
      SHELL_TYPING_UPDATED_EVENT,
      (event) => {
        if (
          cancelled ||
          event.payload.account_key !== activeAccount.account_key
        ) {
          return;
        }

        const remoteTypingUsers = event.payload.users.filter(
          (userId) => userId !== activeAccount.user_id,
        );
        setTypingUsersByRoomId((currentTypingUsers) => ({
          ...currentTypingUsers,
          [event.payload.room_id]: remoteTypingUsers,
        }));
      },
    );

    return () => {
      cancelled = true;
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [activeAccount.account_key, activeAccount.user_id]);

  const visibleThreads = useMemo(
    () => filterAndSortRoomThreads(roomThreads, threadSort, threadKindFilter),
    [roomThreads, threadKindFilter, threadSort],
  );
  const selectedThread =
    visibleThreads.find((thread) => thread.id === selectedThreadId) ??
    roomThreads.find((thread) => thread.id === selectedThreadId) ??
    null;
  const selectedRoomSummaryForSelectedThread =
    selectedRoomSummary?.id === selectedThreadId ? selectedRoomSummary : null;
  const selectedTimelineForSelectedThread =
    selectedTimeline?.roomId === selectedThreadId ? selectedTimeline : null;
  const selectedPaginationContext = paginationContextForTimeline(
    activeAccount.account_key,
    selectedTimelineForSelectedThread,
  );
  const selectedPaginationState = selectedPaginationContext
    ? (paginationStatesByKey[paginationStateKey(selectedPaginationContext)] ??
      idlePaginationState)
    : idlePaginationState;
  const isLoadingOlderMessages = paginationIsLoading(selectedPaginationState);
  const selectedTypingUsers = selectedThreadId
    ? (typingUsersByRoomId[selectedThreadId] ?? [])
    : [];
  const selectedSpace =
    spaces.find((space) => space.id === selectedSpaceId) ?? null;
  const selectedComposerDraft = selectedThreadId
    ? (composerDraftsByRoomId[selectedThreadId] ?? emptyComposerDraft())
    : emptyComposerDraft();
  let activeComposerMode: UseAppShellStateResult["activeComposerMode"] =
    "message";
  if (selectedComposerDraft.editEventId) {
    activeComposerMode = "edit";
  } else if (selectedComposerDraft.replyEventId) {
    activeComposerMode = "reply";
  }
  const isThreadOpen = activeView === "messages" && selectedThread !== null;
  const switchableAccounts = knownAccounts
    .filter((account) => account.account_key !== activeAccount.account_key)
    .sort((left, right) => left.user_id.localeCompare(right.user_id));

  const refreshRoomThreadsAfterSend = useCallback(async () => {
    const backendThreads =
      await invoke<BackendRoomThreadSummary[]>("list_room_threads");
    setRoomThreads(backendThreads.map(mapRoomThreadSummary));
  }, []);

  const setRoomTyping = useCallback(
    async (isTyping: boolean) => {
      if (!selectedThreadId) {
        return;
      }

      await invoke("set_room_typing", {
        request: {
          room_id: selectedThreadId,
          is_typing: isTyping,
        },
      });
    },
    [selectedThreadId],
  );

  useEffect(() => {
    if (!selectedThreadId) {
      return;
    }

    const composerHasContent = composerValue.trim().length > 0;
    if (!composerHasContent) {
      void setRoomTyping(false).catch(() => {});
      return;
    }

    void setRoomTyping(true).catch(() => {});
    const timeoutId = window.setTimeout(() => {
      void setRoomTyping(false).catch(() => {});
    }, roomTypingIdleMilliseconds);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [composerValue, selectedThreadId, setRoomTyping]);

  const updateComposerValue = useCallback(
    (value: string) => {
      setComposerValue(value);
      if (!selectedThreadId) {
        return;
      }

      updateComposerDrafts((currentDrafts) => ({
        ...currentDrafts,
        [selectedThreadId]: {
          body: value,
          editEventId: currentDrafts[selectedThreadId]?.editEventId ?? null,
          replyEventId: currentDrafts[selectedThreadId]?.replyEventId ?? null,
        },
      }));
    },
    [selectedThreadId, updateComposerDrafts],
  );

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
      setComposerValue(composerDraftsByRoomId[roomId]?.body ?? "");
      setSelectedThreadId(roomId);
      setActiveView("messages");
    },
    [
      activeAccount.account_key,
      composerDraftsByRoomId,
      roomSnapshotsByRoomId,
      roomThreads,
    ],
  );

  const openRoomAtEvent = useCallback(
    (roomId: string, eventId: string) => {
      setTimelineJumpTarget({ roomId, eventId });
      setComposerValue(composerDraftsByRoomId[roomId]?.body ?? "");
      setSelectedThreadId(roomId);
      setActiveView("messages");
    },
    [composerDraftsByRoomId],
  );

  const reloadSelectedTimeline = useCallback(
    async (roomId: string) => {
      const roomSnapshot = await loadSelectedRoomSnapshot(roomId, null);
      if (selectedThreadIdRef.current !== roomId) {
        return;
      }

      setSelectedRoomSummary(roomSnapshot.summary);
      setSelectedTimeline((currentTimeline) => {
        const mergedTimeline = mergeTimelineRefresh(
          currentTimeline,
          roomSnapshot.timeline,
        );
        const mergedRoomSnapshot = {
          summary: roomSnapshot.summary,
          timeline: mergedTimeline,
        };
        setRoomSnapshotsByRoomId((currentSnapshots) =>
          rememberRoomSnapshot(currentSnapshots, mergedRoomSnapshot),
        );
        writeCachedRoomSnapshot(activeAccount.account_key, mergedRoomSnapshot);
        return mergedTimeline;
      });
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
    if (sendMessageInFlightRef.current) {
      return;
    }

    if (!selectedThreadId) {
      return;
    }

    const currentDraft =
      composerDraftsByRoomIdRef.current[selectedThreadId] ??
      emptyComposerDraft();
    const submittedComposerValue = currentDraft.body;
    const outboundContent = outboundMessageContentFromMarkdown(
      submittedComposerValue,
    );
    if (outboundContent.body.length === 0) {
      return;
    }

    sendMessageInFlightRef.current = true;
    setComposerValue("");
    updateComposerDrafts((currentDrafts) => ({
      ...currentDrafts,
      [selectedThreadId]: emptyComposerDraft(),
    }));
    setTimelineJumpTarget(null);
    sendMessageInFlightRef.current = false;

    try {
      if (currentDraft.editEventId) {
        await invoke("edit_room_message", {
          request: {
            room_id: selectedThreadId,
            event_id: currentDraft.editEventId,
            ...outboundContent,
          },
        });
      } else if (currentDraft.replyEventId) {
        await invoke("reply_to_room_message", {
          request: {
            room_id: selectedThreadId,
            event_id: currentDraft.replyEventId,
            ...outboundContent,
          },
        });
      } else {
        await invoke("send_room_message", {
          request: {
            room_id: selectedThreadId,
            ...outboundContent,
          },
        });
      }

      void refreshRoomThreadsAfterSend().catch(() => {});
    } catch {
      setGenericErrorFeedback(setFeedbackMessage, "Message could not be sent.");
    }
  }, [refreshRoomThreadsAfterSend, selectedThreadId, updateComposerDrafts]);

  const beginEditMessage = useCallback(
    (eventId: string, body: string) => {
      if (!selectedThreadId) {
        return;
      }

      setComposerValue(body);
      updateComposerDrafts((currentDrafts) => ({
        ...currentDrafts,
        [selectedThreadId]: {
          body,
          editEventId: eventId,
          replyEventId: null,
        },
      }));
    },
    [selectedThreadId, updateComposerDrafts],
  );

  const beginReplyToMessage = useCallback(
    (eventId: string) => {
      if (!selectedThreadId) {
        return;
      }

      updateComposerDrafts((currentDrafts) => ({
        ...currentDrafts,
        [selectedThreadId]: {
          body:
            currentDrafts[selectedThreadId]?.body ?? composerValueRef.current,
          editEventId: null,
          replyEventId: eventId,
        },
      }));
    },
    [selectedThreadId, updateComposerDrafts],
  );

  const cancelComposerMode = useCallback(() => {
    if (!selectedThreadId) {
      return;
    }

    const currentDraft =
      composerDraftsByRoomIdRef.current[selectedThreadId] ??
      emptyComposerDraft();
    const nextBody = currentDraft.editEventId
      ? ""
      : currentDraft.body || composerValue;
    setComposerValue(nextBody);
    updateComposerDrafts((currentDrafts) => ({
      ...currentDrafts,
      [selectedThreadId]: {
        body: nextBody,
        editEventId: null,
        replyEventId: null,
      },
    }));
  }, [composerValue, selectedThreadId, updateComposerDrafts]);

  const redactMessage = useCallback(
    async (eventId: string) => {
      if (!selectedThreadId) {
        return;
      }

      try {
        await invoke("redact_room_message", {
          request: {
            room_id: selectedThreadId,
            event_id: eventId,
            reason: null,
          },
        });
        await reloadSelectedTimeline(selectedThreadId);
      } catch {
        setGenericErrorFeedback(
          setFeedbackMessage,
          "Message could not be deleted.",
        );
      }
    },
    [reloadSelectedTimeline, selectedThreadId],
  );

  const toggleReaction = useCallback(
    async (eventId: string, reactionKey: string) => {
      if (!selectedThreadId) {
        return;
      }

      try {
        await invoke("toggle_room_reaction", {
          request: {
            room_id: selectedThreadId,
            event_id: eventId,
            reaction_key: reactionKey,
          },
        });
      } catch {
        setGenericErrorFeedback(
          setFeedbackMessage,
          "Reaction could not be updated.",
        );
      }
    },
    [selectedThreadId],
  );

  const loadOlderMessages = useCallback(async () => {
    const paginationContext = paginationContextForTimeline(
      activeAccount.account_key,
      selectedTimeline,
    );
    if (!selectedThreadId || !selectedTimeline || !paginationContext) {
      logPaginationDiagnostic("pagination.ui.command.error", {
        accountKey: activeAccount.account_key,
        roomId: selectedThreadId,
        timelineContext: "unknown",
        reason: !selectedThreadId
          ? "missing_selected_room"
          : "missing_timeline",
      });
      return;
    }

    const stateKey = paginationStateKey(paginationContext);
    const currentPaginationState =
      paginationStatesByKeyRef.current[stateKey] ?? idlePaginationState;
    if (paginationIsLoading(currentPaginationState)) {
      logPaginationDiagnostic("pagination.ui.click_ignored", {
        accountKey: paginationContext.accountKey,
        roomId: paginationContext.roomId,
        timelineContext: paginationContext.timelineContext,
        currentStatus: currentPaginationState.status,
        loading: true,
        requestId:
          currentPaginationState.status === "loading"
            ? currentPaginationState.requestId
            : undefined,
        reason: "loading",
      });
      return;
    }

    const requestId = createPaginationRequestId();
    updatePaginationState(paginationContext, {
      status: "loading",
      requestId,
      startedAt: Date.now(),
    });
    paginationPresentationRoomIdRef.current = paginationContext.roomId;

    let timeoutDidFire = false;
    const timeoutId = window.setTimeout(() => {
      timeoutDidFire = true;
      logPaginationDiagnostic("pagination.ui.timeout_reset", {
        accountKey: paginationContext.accountKey,
        roomId: paginationContext.roomId,
        timelineContext: paginationContext.timelineContext,
        currentStatus: "loading",
        nextStatus: "idle",
        requestId,
        loading: false,
      });
      updatePaginationState(paginationContext, idlePaginationState);
    }, paginationLoadingTimeoutMilliseconds);

    try {
      logPaginationDiagnostic("pagination.ui.command.invoke", {
        accountKey: paginationContext.accountKey,
        roomId: paginationContext.roomId,
        timelineContext: paginationContext.timelineContext,
        currentStatus: "loading",
        requestId,
        loading: true,
      });
      const backendResponse =
        await invoke<BackendRoomTimelinePaginationResponse>(
          "paginate_room_timeline_backwards",
          {
            request: {
              room_id: selectedThreadId,
              before: selectedTimeline.nextBefore,
              limit: roomTimelinePageSize,
              request_id: requestId,
              known_event_ids: selectedTimeline.items.map((item) => item.id),
            },
          },
        );
      const paginationResponse =
        mapRoomTimelinePaginationResponse(backendResponse);

      window.clearTimeout(timeoutId);
      logPaginationDiagnostic("pagination.ui.command.success", {
        accountKey: paginationContext.accountKey,
        roomId: paginationContext.roomId,
        timelineContext: paginationContext.timelineContext,
        requestId,
        receivedItemCount: paginationResponse.items.length,
        backendDuplicateCount: paginationResponse.duplicateCount,
        backendNewItemCount: paginationResponse.newItemCount,
        continuationAttemptCount: paginationResponse.continuationAttemptCount,
        timeoutDidFire,
      });

      if (paginationResponse.items.length === 0) {
        logPaginationDiagnostic("pagination.ui.empty_result", {
          accountKey: paginationContext.accountKey,
          roomId: paginationContext.roomId,
          timelineContext: paginationContext.timelineContext,
          requestId,
          receivedItemCount: 0,
          reason: paginationResponse.reason,
        });
        if (paginationResponse.tokenChanged) {
          setSelectedTimeline((currentTimeline) => {
            if (
              !currentTimeline ||
              currentTimeline.roomId !== paginationResponse.roomId
            ) {
              return currentTimeline;
            }

            const cursorAdvancedTimeline = {
              ...currentTimeline,
              nextBefore: paginationResponse.nextBefore,
            };
            const roomSnapshot = {
              summary: selectedRoomSummaryForSelectedThread ?? {
                id: cursorAdvancedTimeline.roomId,
                title: selectedThread?.title ?? "",
                participantLabel: selectedThread?.participantLabel ?? "",
                homeserverLabel: selectedThread?.homeserverLabel ?? "",
                topic: "",
                isDirect: selectedThread?.isDirect ?? false,
                canSendMessages: true,
              },
              timeline: cursorAdvancedTimeline,
            };
            setRoomSnapshotsByRoomId((currentSnapshots) =>
              rememberRoomSnapshot(currentSnapshots, roomSnapshot),
            );
            writeCachedRoomSnapshot(activeAccount.account_key, roomSnapshot);
            logPaginationDiagnostic("pagination.ui.merge.done", {
              accountKey: paginationContext.accountKey,
              roomId: paginationContext.roomId,
              timelineContext: paginationContext.timelineContext,
              requestId,
              receivedItemCount: 0,
              mergedItemCount: 0,
              duplicateCount: paginationResponse.duplicateCount,
              nextBeforeAdvanced: true,
            });
            return cursorAdvancedTimeline;
          });
        }
        const retryCount = paginationRetryCountsRef.current[stateKey] ?? 0;
        const cursorAdvanceCount = retryCount + 1;
        if (
          paginationCanAutomaticallyContinue(
            paginationResponse,
            cursorAdvanceCount,
          )
        ) {
          const retryDelayMilliseconds =
            paginationBackoffDelayMilliseconds(retryCount);
          paginationRetryCountsRef.current[stateKey] = cursorAdvanceCount;
          updatePaginationState(paginationContext, {
            status: "cooldown",
            retryAt: Date.now() + retryDelayMilliseconds,
            retryCount,
          });
          paginationRetryTimeoutsRef.current[stateKey] = window.setTimeout(
            () => {
              delete paginationRetryTimeoutsRef.current[stateKey];
              if (selectedThreadIdRef.current !== paginationContext.roomId) {
                return;
              }
              updatePaginationState(paginationContext, idlePaginationState);
              void loadOlderMessagesRef.current?.();
            },
            retryDelayMilliseconds,
          );
        } else {
          paginationRetryCountsRef.current[stateKey] = 0;
          updatePaginationState(paginationContext, idlePaginationState);
        }
        return;
      }

      setSelectedTimeline((currentTimeline) => {
        logPaginationDiagnostic("pagination.ui.merge.start", {
          accountKey: paginationContext.accountKey,
          roomId: paginationContext.roomId,
          timelineContext: paginationContext.timelineContext,
          requestId,
          receivedItemCount: paginationResponse.items.length,
        });
        if (
          !currentTimeline ||
          currentTimeline.roomId !== paginationResponse.roomId
        ) {
          const replacementTimeline = {
            roomId: paginationResponse.roomId,
            items: paginationResponse.items,
            nextBefore: paginationResponse.nextBefore,
            focusedEventId: selectedTimeline.focusedEventId,
            redactedEventIds: selectedTimeline.redactedEventIds,
          };
          const roomSnapshot = {
            summary: selectedRoomSummaryForSelectedThread ?? {
              id: replacementTimeline.roomId,
              title: selectedThread?.title ?? "",
              participantLabel: selectedThread?.participantLabel ?? "",
              homeserverLabel: selectedThread?.homeserverLabel ?? "",
              topic: "",
              isDirect: selectedThread?.isDirect ?? false,
              canSendMessages: true,
            },
            timeline: replacementTimeline,
          };
          setRoomSnapshotsByRoomId((currentSnapshots) =>
            rememberRoomSnapshot(currentSnapshots, roomSnapshot),
          );
          writeCachedRoomSnapshot(activeAccount.account_key, roomSnapshot);
          logPaginationDiagnostic("pagination.ui.merge.done", {
            accountKey: paginationContext.accountKey,
            roomId: paginationContext.roomId,
            timelineContext: paginationContext.timelineContext,
            requestId,
            mergedItemCount: replacementTimeline.items.length,
            duplicateCount: 0,
          });
          return replacementTimeline;
        }

        const prependResult = prependTimelinePage(
          currentTimeline,
          paginationResponse.items,
          paginationResponse.nextBefore,
          paginationResponse.tokenChanged,
        );
        if (prependResult.insertedCount === 0) {
          logPaginationDiagnostic("pagination.ui.duplicate_only", {
            accountKey: paginationContext.accountKey,
            roomId: paginationContext.roomId,
            timelineContext: paginationContext.timelineContext,
            requestId,
            receivedItemCount: paginationResponse.items.length,
            duplicateCount: prependResult.duplicateCount,
          });
        }

        const mergedTimeline = prependResult.timeline;
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
        logPaginationDiagnostic("pagination.ui.merge.done", {
          accountKey: paginationContext.accountKey,
          roomId: paginationContext.roomId,
          timelineContext: paginationContext.timelineContext,
          requestId,
          receivedItemCount: paginationResponse.items.length,
          mergedItemCount: prependResult.insertedCount,
          duplicateCount: prependResult.duplicateCount,
        });
        return mergedTimeline;
      });
      const retryCount = paginationRetryCountsRef.current[stateKey] ?? 0;
      const cursorAdvanceCount = retryCount + 1;
      if (
        paginationCanAutomaticallyContinue(
          paginationResponse,
          cursorAdvanceCount,
        )
      ) {
        const retryDelayMilliseconds =
          paginationBackoffDelayMilliseconds(retryCount);
        paginationRetryCountsRef.current[stateKey] = cursorAdvanceCount;
        updatePaginationState(paginationContext, {
          status: "cooldown",
          retryAt: Date.now() + retryDelayMilliseconds,
          retryCount,
        });
        const priorTimeoutId = paginationRetryTimeoutsRef.current[stateKey];
        if (priorTimeoutId !== undefined) {
          window.clearTimeout(priorTimeoutId);
        }
        paginationRetryTimeoutsRef.current[stateKey] = window.setTimeout(() => {
          delete paginationRetryTimeoutsRef.current[stateKey];
          if (selectedThreadIdRef.current !== paginationContext.roomId) {
            return;
          }
          updatePaginationState(paginationContext, idlePaginationState);
          void loadOlderMessagesRef.current?.();
        }, retryDelayMilliseconds);
      } else {
        paginationRetryCountsRef.current[stateKey] = 0;
        updatePaginationState(paginationContext, idlePaginationState);
      }
    } catch (error) {
      window.clearTimeout(timeoutId);
      const message = error instanceof Error ? error.message : String(error);
      logPaginationDiagnostic("pagination.ui.command.error", {
        accountKey: paginationContext.accountKey,
        roomId: paginationContext.roomId,
        timelineContext: paginationContext.timelineContext,
        currentStatus: "loading",
        nextStatus: "error",
        requestId,
        message,
      });
      updatePaginationState(paginationContext, { status: "error", message });
      setGenericErrorFeedback(
        setFeedbackMessage,
        paginationErrorIsRateLimited(error)
          ? "Older messages are temporarily rate limited. Please wait and try again."
          : "Could not load older messages.",
      );
    } finally {
      const paginationPresentationIsCurrent =
        paginationPresentationRoomIdRef.current === paginationContext.roomId;
      if (paginationPresentationIsCurrent) {
        paginationPresentationRoomIdRef.current = null;
        const deferredRefresh = deferredTimelineRefreshRef.current;
        deferredTimelineRefreshRef.current = null;
        const deferredRefreshStillApplies =
          deferredRefresh &&
          selectedThreadIdRef.current === paginationContext.roomId;
        if (deferredRefreshStillApplies) {
          setSelectedRoomSummary(deferredRefresh.summary);
          setSelectedTimeline((currentTimeline) => {
            const mergedTimeline = mergeTimelineRefresh(
              currentTimeline,
              deferredRefresh.timeline,
            );
            const roomSnapshot = {
              summary: deferredRefresh.summary,
              timeline: mergedTimeline,
            };
            setRoomSnapshotsByRoomId((currentSnapshots) =>
              rememberRoomSnapshot(currentSnapshots, roomSnapshot),
            );
            writeCachedRoomSnapshot(activeAccount.account_key, roomSnapshot);
            return mergedTimeline;
          });
        }
      }
    }
  }, [
    activeAccount.account_key,
    selectedRoomSummaryForSelectedThread,
    selectedThread,
    selectedThreadId,
    selectedTimeline,
    updatePaginationState,
  ]);

  loadOlderMessagesRef.current = loadOlderMessages;

  const openMessagesView = useCallback(() => {
    setActiveView("messages");
    setIsAccountCenterOpen(false);
    setIsDiscoveryOpen(false);
    setIsSortMenuOpen(false);
    setSelectedSpaceId(null);
  }, []);

  const openSpacesView = useCallback(() => {
    setActiveView("spaces");
    setIsAccountCenterOpen(false);
    setIsDiscoveryOpen(false);
    setIsSortMenuOpen(false);
    setSelectedThreadId(null);
  }, []);

  const openSettingsView = useCallback(() => {
    setActiveView("settings");
    setIsAccountCenterOpen(false);
    setIsDiscoveryOpen(false);
    setIsSortMenuOpen(false);
  }, []);

  const handleGlobalSearchResult = useCallback(
    (
      threadId?: string,
      targetView?: AuthenticatedShellView,
      eventId?: string,
    ) => {
      setIsGlobalSearchOpen(false);
      setIsDiscoveryOpen(false);
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

  const closeDiscovery = useCallback(() => {
    setIsDiscoveryOpen(false);
  }, []);

  const handleDiscoveryError = useCallback((message: string) => {
    setGenericErrorFeedback(setFeedbackMessage, message);
  }, []);

  const handleDiscoveryInviteSent = useCallback(() => {
    setFeedbackMessage({
      tone: "success",
      text: "Invite sent.",
    });
  }, []);

  const handleDiscoveryJoined = useCallback(async () => {
    await refreshRoomCollections();
    setFeedbackMessage({
      tone: "success",
      text: "Joined room.",
    });
  }, [refreshRoomCollections]);

  const openGlobalSearch = useCallback(() => {
    setIsGlobalSearchOpen(true);
    setIsDiscoveryOpen(false);
    setIsAccountCenterOpen(false);
  }, []);

  const openDiscovery = useCallback(() => {
    setIsDiscoveryOpen(true);
    setIsGlobalSearchOpen(false);
    setIsAccountCenterOpen(false);
    setIsSortMenuOpen(false);
  }, []);

  return {
    activeView,
    composerValue,
    feedbackMessage,
    globalSearchQuery,
    globalSearchResults,
    globalSearchStatusNotice: globalSearchNotice,
    isAccountCenterOpen,
    isDiscoveryOpen,
    isGlobalSearchOpen,
    isLoadingOlderMessages,
    paginationState: selectedPaginationState,
    isLoadingShell,
    isSendingMessage,
    isSortMenuOpen,
    isThreadOpen,
    activeComposerMode,
    selectedTypingUsers,
    selectedRoomSummary: selectedRoomSummaryForSelectedThread,
    selectedSpace,
    selectedThread,
    selectedTimeline: selectedTimelineForSelectedThread,
    switchableAccounts,
    switchingAccountKey,
    threadKindFilter,
    threadSort,
    visibleSpaces: spaces,
    visibleThreads,
    closeGlobalSearch: () => setIsGlobalSearchOpen(false),
    closeDiscovery,
    closeThread: () => setSelectedThreadId(null),
    beginEditMessage,
    beginReplyToMessage,
    cancelComposerMode,
    handleGlobalSearchResult,
    handleDiscoveryError,
    handleDiscoveryInviteSent,
    handleDiscoveryJoined,
    loadOlderMessages,
    openGlobalSearch,
    openDiscovery,
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
    redactMessage,
    setRoomTyping,
    setComposerValue: updateComposerValue,
    setGlobalSearchQuery,
    setThreadKindFilter,
    switchAccount,
    toggleAccountCenter: () =>
      setIsAccountCenterOpen((currentValue) => !currentValue),
    toggleSortMenu: () => setIsSortMenuOpen((currentValue) => !currentValue),
    toggleReaction,
  };
}
