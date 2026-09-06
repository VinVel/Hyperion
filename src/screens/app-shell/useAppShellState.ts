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
  roomTimelineFromUpdatePayload,
  timelineAnchorForRoom,
} from "./timeline/helpers";
import { useTimelineModel } from "./timeline/useTimelineModel";
import { PaginationBatch, type PaginationViewport } from "./paginationBatch";
import { logPaginationDiagnostic } from "./paginationDiagnostics";
import {
  createPaginationRequestId,
  idlePaginationState,
  paginationContextForTimeline,
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
  removeLegacyTimelineSnapshots,
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
// The guard now lasts until completion; failures make the next gesture retryable.
// Global search waits briefly so every keystroke does not call the backend.
const globalSearchDebounceMilliseconds = 150;
// Each search group is capped to keep the overlay compact.
const globalSearchLimitPerGroup = 4;
// Startup retries cover the common mobile flow where the WebView returns before
// the native Matrix client is ready.
const shellStartupRetryDelayMilliseconds = [1_000, 3_000, 7_000, 15_000];
// Low-frequency recovery guard for missed collection events. Sync correctness
// must come from backend coordinator events, not this timer.
const shellCollectionRecoveryRefreshIntervalMilliseconds = 15_000;
// Stop our typing notice shortly after the composer becomes idle.
const roomTypingIdleMilliseconds = 1_500;
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
  const {
    selectedTimeline,
    timelineModelRef,
    beginTimeline,
    closeTimeline,
    acceptTimelineSnapshot,
    updateTimelineStatus,
  } = useTimelineModel();
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
  const timelineListenerReadyRef = useRef<Promise<void>>(Promise.resolve());
  const sendMessageInFlightRef = useRef(false);
  const paginationStatesByKeyRef = useRef<Record<string, PaginationState>>({});
  const paginationBackoffsRef = useRef<Record<string, number>>({});
  const paginationBatchRef = useRef<{
    session: object;
    instanceId: string;
    batch: PaginationBatch;
  } | null>(null);
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
      const mappedTimeline = mapRoomTimeline(backendTimeline);
      return {
        summary: mapRoomSummary(backendSummary),
        timeline: mappedTimeline,
      };
    },
    [],
  );

  useEffect(() => {
    removeLegacyTimelineSnapshots();
  }, []);

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
    closeTimeline();
    setTimelineJumpTarget(null);
    setComposerValue("");
    updateComposerDrafts(() => ({}));
    setTypingUsersByRoomId({});
    paginationStatesByKeyRef.current = {};
    paginationBatchRef.current?.batch.dispose();
    paginationBatchRef.current = null;
    setPaginationStatesByKey({});
    paginationBackoffsRef.current = {};
  }, [activeAccount.account_key, closeTimeline, updateComposerDrafts]);

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
    let cancelled = false;
    let collectionRefreshTimeoutId: number | null = null;
    let timelineRefreshTimeoutId: number | null = null;
    const pendingRoomIds = new Set<string>();
    let pendingAmbiguousRoomListUpdate = false;

    async function refreshSelectedRoomAfterSync(roomId: string) {
      const session = timelineModelRef.current?.session;
      if (
        !session ||
        session.roomId !== roomId ||
        !timelineModelRef.current?.timeline
      )
        return;
      const roomSnapshot = await loadSelectedRoomSnapshot(
        roomId,
        session.focusedEventId,
      );
      if (
        cancelled ||
        !acceptTimelineSnapshot(session, roomSnapshot.timeline, "refresh")
      )
        return;
      setSelectedRoomSummary(roomSnapshot.summary);
      void refreshRoomCollections().catch(() => {});
    }

    function applyLiveTimelineUpdate(payload: ShellTimelineUpdatedPayload) {
      const session = timelineModelRef.current?.session;
      if (
        cancelled ||
        !session ||
        payload.account_key !== session.accountKey ||
        payload.room_id !== session.roomId
      )
        return;
      acceptTimelineSnapshot(
        session,
        roomTimelineFromUpdatePayload(payload),
        "update",
      );
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
    timelineListenerReadyRef.current = unlistenTimelinePromise.then(() => {});

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
    selectedThreadId,
    timelineJumpTarget,
    timelineModelRef,
    acceptTimelineSnapshot,
  ]);

  useEffect(() => {
    if (selectionAccountKey !== activeAccount.account_key) {
      return;
    }

    if (activeView !== "messages") {
      closeTimeline();
      return;
    }
    if (!selectedThreadId) {
      setSelectedRoomSummary(null);
      closeTimeline();
      setTimelineJumpTarget(null);
      setComposerValue("");
      return;
    }

    let cancelled = false;
    const roomId = selectedThreadId;
    const anchoredEventId = timelineAnchorForRoom(roomId, timelineJumpTarget);
    const session = beginTimeline({
      accountKey: activeAccount.account_key,
      roomId,
      focusedEventId: anchoredEventId,
    });

    async function loadSelectedRoom() {
      try {
        await timelineListenerReadyRef.current;
        if (cancelled) return;
        const roomSnapshot = await loadSelectedRoomSnapshot(
          roomId,
          anchoredEventId,
        );
        if (cancelled || selectedThreadIdRef.current !== roomId) {
          return;
        }

        if (!acceptTimelineSnapshot(session, roomSnapshot.timeline, "initial"))
          return;
        setSelectedRoomSummary(roomSnapshot.summary);
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
      closeTimeline();
    };
  }, [
    activeView,
    beginTimeline,
    closeTimeline,
    acceptTimelineSnapshot,
    activeAccount.account_key,
    loadSelectedRoomSnapshot,
    refreshRoomCollections,
    selectionAccountKey,
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
    selectedTimeline?.roomId === selectedThreadId &&
    selectedTimeline.timelineIdentity.accountKey === activeAccount.account_key
      ? selectedTimeline
      : null;
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
      const session = timelineModelRef.current?.session;
      if (session?.roomId === roomId && session.focusedEventId === null) return;
      closeTimeline();
      const thread = roomThreads.find((candidate) => candidate.id === roomId);
      setSelectedRoomSummary(
        thread ? fallbackRoomSummaryFromThread(thread) : null,
      );

      setTimelineJumpTarget(null);
      setComposerValue(composerDraftsByRoomId[roomId]?.body ?? "");
      setSelectedThreadId(roomId);
      setActiveView("messages");
    },
    [
      activeAccount.account_key,
      composerDraftsByRoomId,
      closeTimeline,
      timelineModelRef,
      roomThreads,
    ],
  );

  const openRoomAtEvent = useCallback(
    (roomId: string, eventId: string) => {
      closeTimeline();
      setTimelineJumpTarget({ roomId, eventId });
      setComposerValue(composerDraftsByRoomId[roomId]?.body ?? "");
      setSelectedThreadId(roomId);
      setActiveView("messages");
    },
    [closeTimeline, composerDraftsByRoomId],
  );

  const reloadSelectedTimeline = useCallback(
    async (roomId: string) => {
      const session = timelineModelRef.current?.session;
      if (!session || session.roomId !== roomId) return;
      const roomSnapshot = await loadSelectedRoomSnapshot(
        roomId,
        session.focusedEventId,
      );
      if (acceptTimelineSnapshot(session, roomSnapshot.timeline, "refresh"))
        setSelectedRoomSummary(roomSnapshot.summary);
    },
    [acceptTimelineSnapshot, loadSelectedRoomSnapshot, timelineModelRef],
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
        closeTimeline();
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
    [closeTimeline, onActiveAccountChange],
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

  const loadOlderMessages = useCallback(
    async (viewport: PaginationViewport) => {
      const model = timelineModelRef.current;
      const timeline = model?.timeline;
      const context = paginationContextForTimeline(
        activeAccount.account_key,
        timeline ?? null,
      );
      if (!model || !timeline || !context || !timeline.nextBefore) return;
      const { session } = model;
      const identity = timeline.timelineIdentity;
      const isCurrent = () =>
        timelineModelRef.current?.session === session &&
        timelineModelRef.current.timeline?.timelineIdentity.instanceId ===
          identity.instanceId;
      let owner = paginationBatchRef.current;
      if (
        !owner ||
        owner.session !== session ||
        owner.instanceId !== identity.instanceId
      ) {
        if (owner) {
          paginationBackoffsRef.current[owner.instanceId] =
            owner.batch.backoffCount;
          owner.batch.dispose();
        }
        const batch = new PaginationBatch({
          request: async () => {
            if (!isCurrent()) throw new Error("Timeline changed");
            const requestId = createPaginationRequestId();
            const response = mapRoomTimelinePaginationResponse(
              await invoke<BackendRoomTimelinePaginationResponse>(
                "paginate_room_timeline_backwards",
                {
                  request: {
                    timeline_identity: {
                      account_key: identity.accountKey,
                      room_id: identity.roomId,
                      instance_id: identity.instanceId,
                      focused_event_id: identity.focusedEventId,
                    },
                    room_id: identity.roomId,
                    before: timelineModelRef.current?.timeline?.nextBefore,
                    limit: roomTimelinePageSize,
                    request_id: requestId,
                  },
                },
              ),
            );
            if (!isCurrent()) {
              batch.dispose();
              return response;
            }
            if (
              response.requestId !== requestId ||
              response.timelineIdentity.instanceId !== identity.instanceId ||
              response.timelineIdentity.accountKey !== identity.accountKey ||
              response.timelineIdentity.roomId !== identity.roomId ||
              response.timelineIdentity.focusedEventId !==
                identity.focusedEventId ||
              !Number.isSafeInteger(response.revision) ||
              response.revision < 0
            ) {
              throw new Error("Obsolete timeline pagination response");
            }
            updateTimelineStatus(session, identity.instanceId, response);
            return response;
          },
          state: (loading) => {
            if (!isCurrent()) return;
            updatePaginationState(
              context,
              loading
                ? {
                    status: "loading",
                    requestId: createPaginationRequestId(),
                    startedAt: Date.now(),
                  }
                : idlePaginationState,
            );
          },
          error: (error) => {
            if (!isCurrent()) return;
            const message =
              error instanceof Error ? error.message : String(error);
            setGenericErrorFeedback(setFeedbackMessage, message);
          },
        });
        batch.backoffCount =
          paginationBackoffsRef.current[identity.instanceId] ?? 0;
        owner = { session, instanceId: identity.instanceId, batch };
        paginationBatchRef.current = owner;
      }
      await owner.batch.start(viewport);
    },
    [
      activeAccount.account_key,
      timelineModelRef,
      updatePaginationState,
      updateTimelineStatus,
    ],
  );

  // Dispose pending waits on every view lifecycle change, including A → B → A.
  // The Rust operation continues independently and owns its guard until settled.
  useEffect(
    () => () => {
      const owner = paginationBatchRef.current;
      if (owner) {
        paginationBackoffsRef.current[owner.instanceId] =
          owner.batch.backoffCount;
        owner.batch.dispose();
      }
      paginationBatchRef.current = null;
      paginationStatesByKeyRef.current = {};
      setPaginationStatesByKey({});
    },
    [
      activeAccount.account_key,
      selectedThreadId,
      activeView,
      selectedTimeline?.timelineIdentity.instanceId,
    ],
  );

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
    closeThread: () => {
      closeTimeline();
      setSelectedThreadId(null);
    },
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
