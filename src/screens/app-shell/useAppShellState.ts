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

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  type AccountSummary,
  type AuthenticatedShellView,
  type BackendGlobalSearchResponse,
  type BackendRoomSummary,
  type BackendRoomThreadSummary,
  type BackendRoomTimeline,
  type BackendSpaceSummary,
  type RoomTimeline,
  type RoomSummary,
  type RoomThreadSort,
  type SearchResultGroup,
  type SpaceSummary,
  filterAndSortRoomThreads,
  mapGlobalSearchResponse,
  mapRoomSummary,
  mapRoomThreadSummary,
  mapRoomTimeline,
  mapSpaceSummary,
} from './appShellAdapters';

const SHELL_SYNC_UPDATED_EVENT = 'hyperion://shell-sync-updated';

// Debounce shell refresh events so rapid sync bursts do not thrash the UI.
const shellSyncRefreshDebounceMilliseconds = 250;

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
  tone: 'success' | 'error' | 'info';
  text: string;
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
  if (typeof error === 'string' && error.trim().length > 0) {
    return error;
  }

  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message;
  }

  return 'Something went wrong while contacting the native shell service.';
}

export default function useAppShellState({
  activeAccount,
  onActiveAccountChange,
}: UseAppShellStateOptions): UseAppShellStateResult {
  const [activeView, setActiveView] = useState<AuthenticatedShellView>('messages');
  const [knownAccounts, setKnownAccounts] = useState<AccountSummary[]>([activeAccount]);
  const [roomThreads, setRoomThreads] = useState<ReturnType<typeof mapRoomThreadSummary>[]>([]);
  const [spaces, setSpaces] = useState<SpaceSummary[]>([]);
  const [selectedThreadId, setSelectedThreadId] = useState<string | null>(null);
  const [selectedSpaceId, setSelectedSpaceId] = useState<string | null>(null);
  const [selectedRoomSummary, setSelectedRoomSummary] = useState<RoomSummary | null>(null);
  const [selectedTimeline, setSelectedTimeline] = useState<RoomTimeline | null>(null);
  const [timelineJumpTarget, setTimelineJumpTarget] = useState<TimelineJumpTarget | null>(null);
  const [composerValue, setComposerValue] = useState('');
  const [isSendingMessage, setIsSendingMessage] = useState(false);
  const [isLoadingOlderMessages, setIsLoadingOlderMessages] = useState(false);
  const [threadSearchQuery, setThreadSearchQuery] = useState('');
  const [threadSort, setThreadSort] = useState<RoomThreadSort>('newest');
  const [isSortMenuOpen, setIsSortMenuOpen] = useState(false);
  const [spaceSearchQuery, setSpaceSearchQuery] = useState('');
  const [globalSearchQuery, setGlobalSearchQuery] = useState('');
  const [isGlobalSearchOpen, setIsGlobalSearchOpen] = useState(false);
  const [globalSearchResults, setGlobalSearchResults] = useState<SearchResultGroup[]>([]);
  const [isAccountCenterOpen, setIsAccountCenterOpen] = useState(false);
  const [feedbackMessage, setFeedbackMessage] = useState<FeedbackMessage | null>(null);
  const [switchingAccountKey, setSwitchingAccountKey] = useState<string | null>(null);
  const [isLoadingShell, setIsLoadingShell] = useState(true);

  const refreshRoomCollections = useCallback(async () => {
    const [backendThreads, backendSpaces] = await Promise.all([
      invoke<BackendRoomThreadSummary[]>('list_room_threads'),
      invoke<BackendSpaceSummary[]>('list_spaces'),
    ]);

    const mappedThreads = backendThreads.map(mapRoomThreadSummary);
    const mappedSpaces = backendSpaces.map(mapSpaceSummary);

    setRoomThreads(mappedThreads);
    setSpaces(mappedSpaces);
    setSelectedThreadId((currentThreadId) =>
      currentThreadId && mappedThreads.some((thread) => thread.id === currentThreadId)
        ? currentThreadId
        : mappedThreads[0]?.id ?? null,
    );
    setSelectedSpaceId((currentSpaceId) =>
      currentSpaceId && mappedSpaces.some((space) => space.id === currentSpaceId)
        ? currentSpaceId
        : mappedSpaces[0]?.id ?? null,
    );
  }, []);

  const refreshShellSnapshot = useCallback(async () => {
    const accounts = await invoke<AccountSummary[]>('list_accounts');
    setKnownAccounts(accounts);
    await refreshRoomCollections();
  }, [refreshRoomCollections]);

  const refreshSelectedRoom = useCallback(
    async (roomId: string, anchoredEventId?: string | null) => {
      const [backendSummary, backendTimeline] = await Promise.all([
        invoke<BackendRoomSummary>('get_room_summary', {
          request: { room_id: roomId },
        }),
        anchoredEventId && anchoredEventId.trim().length > 0
          ? invoke<BackendRoomTimeline>('get_room_event_context', {
              request: {
                room_id: roomId,
                event_id: anchoredEventId,
                context_limit: 8,
              },
            })
          : invoke<BackendRoomTimeline>('get_room_timeline', {
              request: { room_id: roomId, limit: 30 },
            }),
      ]);

      setSelectedRoomSummary(mapRoomSummary(backendSummary));
      setSelectedTimeline(mapRoomTimeline(backendTimeline));
    },
    [],
  );

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
            tone: 'error',
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
    if (!selectedThreadId) {
      setSelectedRoomSummary(null);
      setSelectedTimeline(null);
      setTimelineJumpTarget(null);
      setComposerValue('');
      setIsLoadingOlderMessages(false);
      return;
    }

    let cancelled = false;
    const roomId = selectedThreadId;

    async function loadSelectedRoom() {
      try {
        const anchoredEventId =
          timelineJumpTarget?.roomId === roomId &&
          timelineJumpTarget.eventId.trim().length > 0
            ? timelineJumpTarget.eventId
            : null;
        await refreshSelectedRoom(roomId, anchoredEventId);
        if (!anchoredEventId) {
          await refreshRoomCollections();
        }

        if (cancelled) {
          return;
        }
      } catch (error) {
        if (!cancelled) {
          setFeedbackMessage({
            tone: 'error',
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
    refreshRoomCollections,
    refreshSelectedRoom,
    selectedThreadId,
    timelineJumpTarget,
  ]);

  useEffect(() => {
    let cancelled = false;
    let refreshTimeoutId: number | null = null;

    const unlistenPromise = listen<ShellSyncUpdatedPayload>(
      SHELL_SYNC_UPDATED_EVENT,
      (event) => {
        if (cancelled || event.payload.account_key !== activeAccount.account_key) {
          return;
        }

        if (refreshTimeoutId !== null) {
          window.clearTimeout(refreshTimeoutId);
        }

        refreshTimeoutId = window.setTimeout(() => {
          if (cancelled) {
            return;
          }

          const roomIds = new Set(event.payload.changed_room_ids);

          void (event.payload.room_list_may_have_changed
            ? refreshRoomCollections()
            : Promise.resolve())
            .then(async () => {
              if (
                activeView === 'messages' &&
                selectedThreadId &&
                timelineJumpTarget === null &&
                roomIds.has(selectedThreadId)
              ) {
                await refreshSelectedRoom(selectedThreadId, null);
              }
            })
            .catch((error) => {
              if (!cancelled) {
                setFeedbackMessage({
                  tone: 'error',
                  text: getErrorMessage(error),
                });
              }
            });
        }, shellSyncRefreshDebounceMilliseconds);
      },
    );

    return () => {
      cancelled = true;
      if (refreshTimeoutId !== null) {
        window.clearTimeout(refreshTimeoutId);
      }
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [
    activeAccount.account_key,
    activeView,
    refreshRoomCollections,
    refreshSelectedRoom,
    selectedThreadId,
    timelineJumpTarget,
  ]);

  useEffect(() => {
    if (!isGlobalSearchOpen) {
      setGlobalSearchResults([]);
      return;
    }

    const query = globalSearchQuery.trim();
    if (query.length === 0) {
      setGlobalSearchResults([]);
      return;
    }

    let cancelled = false;
    const timeoutId = window.setTimeout(() => {
      void invoke<BackendGlobalSearchResponse>('global_search', {
        request: { query, limit_per_group: 4 },
      })
        .then((response) => {
          if (!cancelled) {
            setGlobalSearchResults(mapGlobalSearchResponse(response));
          }
        })
        .catch((error) => {
          if (!cancelled) {
            setFeedbackMessage({
              tone: 'error',
              text: getErrorMessage(error),
            });
            setGlobalSearchResults([]);
          }
        });
    }, 150);

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
  const visibleSpaces = useMemo(() => {
    const normalizedQuery = spaceSearchQuery.trim().toLowerCase();
    if (normalizedQuery.length === 0) {
      return spaces;
    }

    return spaces.filter((space) =>
      [space.name, space.description].join(' ').toLowerCase().includes(normalizedQuery),
    );
  }, [spaces, spaceSearchQuery]);
  const selectedSpace =
    visibleSpaces.find((space) => space.id === selectedSpaceId) ??
    spaces.find((space) => space.id === selectedSpaceId) ??
    null;
  const isThreadOpen = activeView === 'messages' && selectedThread !== null;
  const switchableAccounts = knownAccounts
    .filter((account) => account.account_key !== activeAccount.account_key)
    .sort((left, right) => left.user_id.localeCompare(right.user_id));

  const openRoomAtLatest = useCallback((roomId: string) => {
    setTimelineJumpTarget(null);
    setSelectedThreadId(roomId);
    setActiveView('messages');
  }, []);

  const openRoomAtEvent = useCallback((roomId: string, eventId: string) => {
    setTimelineJumpTarget({ roomId, eventId });
    setSelectedThreadId(roomId);
    setActiveView('messages');
  }, []);

  const reloadSelectedTimeline = useCallback(
    async (roomId: string) => {
      await refreshSelectedRoom(roomId, null);
    },
    [refreshSelectedRoom],
  );

  const switchAccount = useCallback(
    async (nextAccount: AccountSummary) => {
      setSwitchingAccountKey(nextAccount.account_key);

      try {
        await invoke('switch_active_account', {
          accountKey: nextAccount.account_key,
        });

        const refreshedActiveAccount =
          (await invoke<AccountSummary | null>('active_account')) ?? nextAccount;

        onActiveAccountChange(refreshedActiveAccount);
        setIsAccountCenterOpen(false);
        setFeedbackMessage(null);
        setSelectedRoomSummary(null);
        setSelectedTimeline(null);
        setGlobalSearchQuery('');
        setGlobalSearchResults([]);
        setTimelineJumpTarget(null);
        setComposerValue('');
      } catch (error) {
        setFeedbackMessage({
          tone: 'error',
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
      await invoke('send_room_message', {
        request: {
          room_id: selectedThreadId,
          body,
        },
      });

      setComposerValue('');
      setTimelineJumpTarget(null);
      await Promise.all([
        reloadSelectedTimeline(selectedThreadId),
        invoke<BackendRoomThreadSummary[]>('list_room_threads').then((backendThreads) => {
          setRoomThreads(backendThreads.map(mapRoomThreadSummary));
        }),
      ]);
    } catch (error) {
      setFeedbackMessage({
        tone: 'error',
        text: getErrorMessage(error),
      });
    } finally {
      setIsSendingMessage(false);
    }
  }, [composerValue, reloadSelectedTimeline, selectedThreadId]);

  const loadOlderMessages = useCallback(async () => {
    if (!selectedThreadId || !selectedTimeline?.nextBefore || isLoadingOlderMessages) {
      return;
    }

    setIsLoadingOlderMessages(true);

    try {
      const backendTimeline = await invoke<BackendRoomTimeline>('get_room_timeline', {
        request: {
          room_id: selectedThreadId,
          before: selectedTimeline.nextBefore,
          limit: 30,
        },
      });
      const olderTimeline = mapRoomTimeline(backendTimeline);

      setSelectedTimeline((currentTimeline) => {
        if (!currentTimeline || currentTimeline.roomId !== olderTimeline.roomId) {
          return olderTimeline;
        }

        const seenItemIds = new Set(currentTimeline.items.map((item) => item.id));
        const olderItems = olderTimeline.items.filter((item) => !seenItemIds.has(item.id));

        return {
          ...currentTimeline,
          items: [...olderItems, ...currentTimeline.items],
          nextBefore: olderTimeline.nextBefore,
        };
      });
    } catch (error) {
      setFeedbackMessage({
        tone: 'error',
        text: getErrorMessage(error),
      });
    } finally {
      setIsLoadingOlderMessages(false);
    }
  }, [isLoadingOlderMessages, selectedThreadId, selectedTimeline]);

  const openMessagesView = useCallback(() => {
    setActiveView('messages');
    setIsAccountCenterOpen(false);
    setIsSortMenuOpen(false);
    setSelectedSpaceId(null);
  }, []);

  const openSpacesView = useCallback(() => {
    setActiveView('spaces');
    setIsAccountCenterOpen(false);
    setIsSortMenuOpen(false);
    setSelectedThreadId(null);
  }, []);

  const openSettingsView = useCallback(() => {
    setActiveView('settings');
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
      setGlobalSearchQuery('');
      setGlobalSearchResults([]);

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
    isAccountCenterOpen,
    isGlobalSearchOpen,
    isLoadingOlderMessages,
    isLoadingShell,
    isSendingMessage,
    isSortMenuOpen,
    isThreadOpen,
    selectedRoomSummary,
    selectedSpace,
    selectedThread,
    selectedTimeline,
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
    toggleAccountCenter: () => setIsAccountCenterOpen((currentValue) => !currentValue),
    toggleSortMenu: () => setIsSortMenuOpen((currentValue) => !currentValue),
  };
}
