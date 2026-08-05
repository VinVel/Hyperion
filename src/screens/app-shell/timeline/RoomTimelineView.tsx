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

import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { Virtuoso, type ListRange, type VirtuosoHandle } from "react-virtuoso";
import { Link, Pencil, Reply, SmilePlus, Trash2 } from "lucide-react";
import { Button, Typography } from "../../../components/ui";
import { isTracingLevelEnabled } from "../../../utils/tracing";
import {
  mapTimelineReplyPreview,
  type BackendRoomTimelineReplyPreview,
  type RoomTimeline,
  type RoomTimelineItem,
  type RoomTimelineReplyPreview,
} from "../appShellAdapters";
import { logPaginationDiagnostic } from "../paginationDiagnostics";
import { timelineContextKey, type PaginationState } from "../pagination";
import TimelineMarkdown from "./TimelineMarkdown";
import TimelineScroller, {
  type TimelineScrollerContext,
} from "./TimelineScroller";
import {
  TimelineMedia,
  cachedPreparedRoomMedia,
  cancelQueuedMediaPreloads,
  mediaGalleryItems,
  timelineMediaPreloadCandidates,
} from "./media";
import {
  logTimelineDebug,
  logTimelineGeometry,
  logTimelineItemIdentityChanges,
  rawTimelineScrollerMetrics,
  useTimelineRowDebug,
} from "./timelineDebug";
import "./RoomTimelineView.css";

type RoomTimelineViewProps = {
  accountKey: string;
  isLoadingOlderMessages: boolean;
  paginationState: PaginationState;
  timeline: RoomTimeline | null;
  onBeginEditMessage: (eventId: string, body: string) => void;
  onBeginReplyToMessage: (eventId: string) => void;
  onLoadOlderMessages: () => void;
  onRedactMessage: (eventId: string) => void;
  onToggleReaction: (eventId: string, reactionKey: string) => void;
};

type TimelineScrollSnapshot = {
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
  wasAtBottom: boolean;
};

type StableTimelineEventTarget = {
  scroller: HTMLDivElement;
  scrollerRect: DOMRect;
  targetElement: HTMLElement;
  targetRect: DOMRect;
};

// Treat tiny offsets as bottom so dynamic measurement does not break follow state.
const bottomAnchorTolerancePixels = 8;

// Keep action anchors briefly because reaction/edit updates can arrive async.
const actionScrollAnchorLifetimeMilliseconds = 1_500;

const replyResolutionRetryDelayMilliseconds = 1_000;

// Reply jumps should show acknowledgement even when the target is already visible.
const replyNavigationHighlightMilliseconds = 1_400;

// Reply navigation uses an absolute viewport anchor when the target is outside view.
const replyNavigationAnchorRatio = 0.28;

// Bottom restores wait for Virtuoso and browser layout to publish final row sizes.
const bottomAnchorRestoreFrameCount = 2;

// Touch/pen long press opens message actions without relying on hover.
const messageActionLongPressMilliseconds = 450;

// Small movement during long press is tolerated; real scrolling cancels it.
const messageActionLongPressMoveTolerancePixels = 8;

// Render range updates happen in chunks so slow WebKitGTK frames have rows ready
// before they enter the viewport without retaining the full timeline DOM.
const timelineRenderOverscan = {
  main: 320,
  reverse: 320,
} as const;

// Only a small number of nearby attachments are warmed after scrolling rests;
// visible rows always retain priority over this background work.
const mediaPreloadAttachmentCountPerDirection = 8;

// Debouncing keeps range notifications off WebKitGTK's active scroll frames.
const mediaPreloadDelayMilliseconds = 120;

function RoomTimelineView({
  accountKey,
  isLoadingOlderMessages,
  paginationState,
  timeline,
  onBeginEditMessage,
  onBeginReplyToMessage,
  onLoadOlderMessages,
  onRedactMessage,
  onToggleReaction,
}: RoomTimelineViewProps) {
  const timelineRootRef = useRef<HTMLDivElement | null>(null);
  const virtuosoRef = useRef<VirtuosoHandle | null>(null);
  const pendingActionScrollSnapshotRef = useRef<TimelineScrollSnapshot | null>(
    null,
  );
  const actionScrollAnchorTimeoutRef = useRef<number | null>(null);
  const navigationHighlightTimeoutRef = useRef<number | null>(null);
  const navigationSequenceRef = useRef(0);
  const previousFocusedEventIdRef = useRef<string | null>(null);
  const isAtBottomRef = useRef(true);
  const isUserScrollInteractionActiveRef = useRef(false);
  const isVirtuosoScrollingRef = useRef(false);
  const resolvingReplyKeysRef = useRef<Set<string>>(new Set());
  const retryingReplyKeysRef = useRef<Set<string>>(new Set());
  const previousTimelineItemsRef = useRef<RoomTimelineItem[]>([]);
  const timelineItemsRef = useRef<RoomTimelineItem[]>([]);
  const visibleTimelineRangeRef = useRef<ListRange | null>(null);
  const preloadedMediaHandlesRef = useRef<Set<string>>(new Set());
  const mediaPreloadTimeoutRef = useRef<number | null>(null);
  const pendingPaginationScrollSnapshotRef =
    useRef<TimelineScrollSnapshot | null>(null);
  const pendingPaginationRequestCountRef = useRef(0);
  const [resolvedReplyPreviews, setResolvedReplyPreviews] = useState<
    Record<string, RoomTimelineReplyPreview>
  >({});
  const [navigationHighlightedEventId, setNavigationHighlightedEventId] =
    useState<string | null>(null);
  const [activeActionEventId, setActiveActionEventId] = useState<string | null>(
    null,
  );
  const timelineItems = timeline?.items ?? [];
  timelineItemsRef.current = timelineItems;
  const getMediaGalleryItems = useCallback(
    () => mediaGalleryItems(timelineItemsRef.current),
    [],
  );
  const preloadTimelineMedia = useCallback(() => {
    const visibleRange = visibleTimelineRangeRef.current;
    if (!visibleRange || timelineItemsRef.current.length === 0) {
      return;
    }

    cancelQueuedMediaPreloads(accountKey);
    const preloadCandidates = timelineMediaPreloadCandidates(
      timelineItemsRef.current,
      visibleRange.startIndex,
      visibleRange.endIndex,
      mediaPreloadAttachmentCountPerDirection,
    );
    for (const { mediaHandle } of preloadCandidates) {
      if (preloadedMediaHandlesRef.current.has(mediaHandle)) {
        continue;
      }

      preloadedMediaHandlesRef.current.add(mediaHandle);
      void cachedPreparedRoomMedia(accountKey, mediaHandle, {
        priority: "preload",
      }).catch(() => {
        preloadedMediaHandlesRef.current.delete(mediaHandle);
      });
    }
  }, [accountKey]);
  const scheduleTimelineMediaPreload = useCallback(() => {
    if (mediaPreloadTimeoutRef.current !== null) {
      window.clearTimeout(mediaPreloadTimeoutRef.current);
    }

    mediaPreloadTimeoutRef.current = window.setTimeout(() => {
      mediaPreloadTimeoutRef.current = null;
      if (!isVirtuosoScrollingRef.current) {
        preloadTimelineMedia();
      }
    }, mediaPreloadDelayMilliseconds);
  }, [preloadTimelineMedia]);
  const handleTimelineRangeChanged = useCallback(
    (visibleRange: ListRange) => {
      visibleTimelineRangeRef.current = visibleRange;
      scheduleTimelineMediaPreload();
    },
    [scheduleTimelineMediaPreload],
  );
  const roomId = timeline?.roomId ?? null;
  const focusedEventId = timeline?.focusedEventId ?? null;
  const timelineContext = timelineContextKey(focusedEventId);
  const [oldestBoundaryIsVisible, setOldestBoundaryIsVisible] = useState(false);
  const timelineTraceIsEnabled = isTracingLevelEnabled("trace");

  function runTimelineAction(action: () => void) {
    const snapshot = captureTimelineScroll(timelineRootRef.current);
    pendingActionScrollSnapshotRef.current = snapshot;
    if (actionScrollAnchorTimeoutRef.current !== null) {
      window.clearTimeout(actionScrollAnchorTimeoutRef.current);
    }
    actionScrollAnchorTimeoutRef.current = window.setTimeout(() => {
      pendingActionScrollSnapshotRef.current = null;
      actionScrollAnchorTimeoutRef.current = null;
    }, actionScrollAnchorLifetimeMilliseconds);
    action();
    restoreTimelineScroll(timelineRootRef.current, snapshot);
  }

  useLayoutEffect(() => {
    if (timelineTraceIsEnabled) {
      logTimelineItemIdentityChanges(
        previousTimelineItemsRef.current,
        timelineItems,
      );
      logTimelineGeometry(
        "layout-effect-before-bottom-restore",
        timelineRootRef.current,
        timelineItems,
        bottomAnchorTolerancePixels,
      );
    }

    if (pendingActionScrollSnapshotRef.current) {
      restoreTimelineScroll(
        timelineRootRef.current,
        pendingActionScrollSnapshotRef.current,
      );
      previousTimelineItemsRef.current = timelineItems;
      return;
    }

    if (pendingPaginationScrollSnapshotRef.current) {
      restoreTimelineScroll(
        timelineRootRef.current,
        pendingPaginationScrollSnapshotRef.current,
      );
      logPaginationDiagnostic("pagination.ui.anchor.restore", {
        accountKey,
        roomId,
        timelineContext,
        success: Boolean(timelineScroller(timelineRootRef.current)),
        requestCount: pendingPaginationRequestCountRef.current,
      });
      pendingPaginationScrollSnapshotRef.current = null;
      previousTimelineItemsRef.current = timelineItems;
      return;
    }

    if (
      !focusedEventId &&
      (isAtBottomRef.current ||
        timelineScrollerIsAtBottom(timelineRootRef.current))
    ) {
      scrollToTimelineBottom(
        timelineRootRef.current,
        virtuosoRef.current,
        timelineTraceIsEnabled,
      );
    }

    previousTimelineItemsRef.current = timelineItems;
  }, [
    accountKey,
    focusedEventId,
    roomId,
    timelineContext,
    timelineTraceIsEnabled,
    timelineItems,
  ]);

  useEffect(() => {
    preloadedMediaHandlesRef.current.clear();
    return () => {
      if (mediaPreloadTimeoutRef.current !== null) {
        window.clearTimeout(mediaPreloadTimeoutRef.current);
        mediaPreloadTimeoutRef.current = null;
      }
      cancelQueuedMediaPreloads(accountKey);
    };
  }, [accountKey, roomId]);

  useEffect(() => {
    return () => {
      if (actionScrollAnchorTimeoutRef.current !== null) {
        window.clearTimeout(actionScrollAnchorTimeoutRef.current);
      }
      if (navigationHighlightTimeoutRef.current !== null) {
        window.clearTimeout(navigationHighlightTimeoutRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (!activeActionEventId) {
      return;
    }

    function handleDocumentPointerDown(event: PointerEvent) {
      if (
        event.target instanceof Element &&
        event.target.closest(".room-timeline-row--actions-open")
      ) {
        return;
      }

      setActiveActionEventId(null);
    }

    function handleDocumentKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setActiveActionEventId(null);
      }
    }

    document.addEventListener("pointerdown", handleDocumentPointerDown, true);
    document.addEventListener("keydown", handleDocumentKeyDown, true);
    return () => {
      document.removeEventListener(
        "pointerdown",
        handleDocumentPointerDown,
        true,
      );
      document.removeEventListener("keydown", handleDocumentKeyDown, true);
    };
  }, [activeActionEventId]);

  const followTimelineOutput = useCallback(
    (wasAtBottom: boolean) => {
      const userControlsScroll = isUserScrollInteractionActiveRef.current;
      const bottomAnchorIsStable =
        wasAtBottom ||
        isAtBottomRef.current ||
        timelineScrollerIsAtBottom(timelineRootRef.current);
      if (timelineTraceIsEnabled) {
        logTimelineDebug("follow-output", {
          wasAtBottom,
          isAtBottomRef: isAtBottomRef.current,
          bottomAnchorIsStable,
          focusedEventId,
          userControlsScroll,
          rawMetrics: rawTimelineScrollerMetrics(
            timelineRootRef.current,
            bottomAnchorTolerancePixels,
          ),
        });
      }
      if (!bottomAnchorIsStable || userControlsScroll || focusedEventId) {
        return false;
      }

      return "auto";
    },
    [focusedEventId, timelineTraceIsEnabled],
  );

  const scrollerContext = useMemo<TimelineScrollerContext>(
    () => ({
      onScrollInteractionEnd: () => {
        if (!isVirtuosoScrollingRef.current) {
          isUserScrollInteractionActiveRef.current = false;
        }
      },
      onScrollInteractionStart: () => {
        isUserScrollInteractionActiveRef.current = true;
      },
    }),
    [],
  );
  const handleLoadOlderMessages = useCallback(() => {
    pendingPaginationScrollSnapshotRef.current = captureTimelineScroll(
      timelineRootRef.current,
    );
    pendingPaginationRequestCountRef.current += 1;
    onLoadOlderMessages();
  }, [onLoadOlderMessages]);

  const virtuosoComponents = useMemo(
    () => ({
      Header: () => (
        <TimelineHeader
          accountKey={accountKey}
          boundaryVisible={oldestBoundaryIsVisible}
          currentStatus={paginationState.status}
          isLoadingOlderMessages={isLoadingOlderMessages}
          roomId={roomId}
          timelineContext={timelineContext}
          onLoadOlderMessages={handleLoadOlderMessages}
        />
      ),
      Scroller: TimelineScroller,
    }),
    [
      accountKey,
      isLoadingOlderMessages,
      oldestBoundaryIsVisible,
      handleLoadOlderMessages,
      paginationState.status,
      roomId,
      timelineContext,
    ],
  );

  function handleVirtuosoScrollingChange(isScrolling: boolean) {
    isVirtuosoScrollingRef.current = isScrolling;
    if (timelineTraceIsEnabled) {
      logTimelineDebug("virtuoso-scrolling-change", {
        isScrolling,
        rawMetrics: rawTimelineScrollerMetrics(
          timelineRootRef.current,
          bottomAnchorTolerancePixels,
        ),
      });
    }
    if (!isScrolling) {
      isUserScrollInteractionActiveRef.current = false;
      scheduleTimelineMediaPreload();
    }
  }

  function scrollToTimelineEvent(eventId: string) {
    const targetIndex = timelineItems.findIndex((item) => item.id === eventId);
    if (targetIndex < 0) {
      return;
    }

    const navigationSequence = navigationSequenceRef.current + 1;
    navigationSequenceRef.current = navigationSequence;
    void navigateToTimelineEvent(eventId, targetIndex, navigationSequence);
  }

  async function navigateToTimelineEvent(
    eventId: string,
    targetIndex: number,
    navigationSequence: number,
  ) {
    const visibleTarget = timelineEventTarget(eventId);
    if (visibleTarget && timelineEventNavigationIsSatisfied(visibleTarget)) {
      highlightNavigatedTimelineEvent(eventId);
      return;
    }

    if (visibleTarget) {
      scrollMountedTimelineEventToAnchor(visibleTarget);
      await waitForAnimationFrame();
      if (navigationIsCurrent(navigationSequence)) {
        highlightNavigatedTimelineEvent(eventId);
      }
      return;
    }

    const scroller = timelineScroller(timelineRootRef.current);
    const offset = scroller
      ? Math.round(scroller.clientHeight * replyNavigationAnchorRatio) * -1
      : 0;
    virtuosoRef.current?.scrollToIndex({
      index: targetIndex,
      align: "start",
      behavior: "auto",
      offset,
    });
    await waitForAnimationFrame();
    if (navigationIsCurrent(navigationSequence)) {
      highlightNavigatedTimelineEvent(eventId);
    }
  }

  function navigationIsCurrent(navigationSequence: number): boolean {
    return navigationSequenceRef.current === navigationSequence;
  }

  function highlightNavigatedTimelineEvent(eventId: string) {
    setNavigationHighlightedEventId(eventId);
    if (navigationHighlightTimeoutRef.current !== null) {
      window.clearTimeout(navigationHighlightTimeoutRef.current);
    }

    navigationHighlightTimeoutRef.current = window.setTimeout(() => {
      setNavigationHighlightedEventId((currentEventId) =>
        currentEventId === eventId ? null : currentEventId,
      );
      navigationHighlightTimeoutRef.current = null;
    }, replyNavigationHighlightMilliseconds);
  }

  function timelineEventTarget(
    eventId: string,
  ): StableTimelineEventTarget | null {
    const rootElement = timelineRootRef.current;
    const scroller = timelineScroller(rootElement);
    const targetElement = rootElement?.querySelector<HTMLElement>(
      `[data-event-id="${CSS.escape(eventId)}"]`,
    );
    if (!scroller || !targetElement) {
      return null;
    }

    return {
      scroller,
      scrollerRect: scroller.getBoundingClientRect(),
      targetElement,
      targetRect: targetElement.getBoundingClientRect(),
    };
  }

  function timelineEventNavigationIsSatisfied(
    target: StableTimelineEventTarget,
  ): boolean {
    return (
      target.targetRect.top >= target.scrollerRect.top &&
      target.targetRect.bottom <= target.scrollerRect.bottom
    );
  }

  function replyPreviewForItem(
    item: RoomTimelineItem,
  ): RoomTimelineReplyPreview | null {
    if (!roomId || !item.replyPreview) {
      return item.replyPreview;
    }

    return (
      resolvedReplyPreviews[
        replyResolutionKey(roomId, item.replyPreview.eventId)
      ] ?? item.replyPreview
    );
  }

  function resolveReplyPreview(roomId: string, eventId: string) {
    const key = replyResolutionKey(roomId, eventId);
    if (resolvingReplyKeysRef.current.has(key)) {
      return;
    }

    resolvingReplyKeysRef.current.add(key);
    void invoke<BackendRoomTimelineReplyPreview>("resolve_room_reply_preview", {
      request: {
        room_id: roomId,
        event_id: eventId,
      },
    })
      .then((replyPreview) => {
        retryingReplyKeysRef.current.delete(key);
        setResolvedReplyPreviews((currentPreviews) => ({
          ...currentPreviews,
          [key]: mapTimelineReplyPreview(replyPreview),
        }));
      })
      .catch(() => {
        if (!retryingReplyKeysRef.current.has(key)) {
          retryingReplyKeysRef.current.add(key);
          window.setTimeout(
            () => resolveReplyPreview(roomId, eventId),
            replyResolutionRetryDelayMilliseconds,
          );
          return;
        }

        setResolvedReplyPreviews((currentPreviews) => ({
          ...currentPreviews,
          [key]: failedReplyPreview(eventId),
        }));
      })
      .finally(() => {
        resolvingReplyKeysRef.current.delete(key);
      });
  }

  useEffect(() => {
    if (
      !focusedEventId ||
      previousFocusedEventIdRef.current === focusedEventId
    ) {
      return;
    }

    previousFocusedEventIdRef.current = focusedEventId;
    const focusedIndex = timelineItems.findIndex(
      (item) => item.id === focusedEventId,
    );
    if (focusedIndex < 0) {
      return;
    }

    virtuosoRef.current?.scrollToIndex({
      index: focusedIndex,
      align: "center",
      behavior: "auto",
    });
  }, [focusedEventId, timelineItems]);

  useEffect(() => {
    if (!roomId) {
      return;
    }

    for (const item of timelineItems) {
      const replyPreview = item.replyPreview;
      if (!replyPreview || replyPreview.state !== "loading") {
        continue;
      }

      const key = replyResolutionKey(roomId, replyPreview.eventId);
      const currentPreview = resolvedReplyPreviews[key];
      if (currentPreview && currentPreview.state !== "loading") {
        continue;
      }

      resolveReplyPreview(roomId, replyPreview.eventId);
    }
  }, [roomId, resolvedReplyPreviews, timelineItems]);

  if (!timelineItems.length) {
    return (
      <div className="room-timeline-empty">
        <Typography variant="label">No messages yet</Typography>
        <Typography variant="body">
          No text messages are available in this room yet.
        </Typography>
      </div>
    );
  }

  return (
    <div className="room-timeline-host" ref={timelineRootRef}>
      <Virtuoso
        key={timeline?.roomId ?? "room-timeline"}
        ref={virtuosoRef}
        alignToBottom
        className="room-timeline"
        components={virtuosoComponents}
        atBottomThreshold={bottomAnchorTolerancePixels}
        computeItemKey={(_index, item) => item.id}
        context={scrollerContext}
        data={timelineItems}
        atBottomStateChange={(isAtBottom) => {
          isAtBottomRef.current = isAtBottom;
          if (timelineTraceIsEnabled) {
            logTimelineDebug("at-bottom-state-change", {
              isAtBottom,
              rawMetrics: rawTimelineScrollerMetrics(
                timelineRootRef.current,
                bottomAnchorTolerancePixels,
              ),
            });
            logTimelineGeometry(
              "at-bottom-state-change",
              timelineRootRef.current,
              timelineItems,
              bottomAnchorTolerancePixels,
            );
          }
        }}
        atTopStateChange={(isAtTop) => {
          setOldestBoundaryIsVisible(isAtTop);
        }}
        followOutput={followTimelineOutput}
        initialTopMostItemIndex={{
          index: timelineItems.length - 1,
          align: "end",
        }}
        isScrolling={handleVirtuosoScrollingChange}
        overscan={timelineRenderOverscan}
        rangeChanged={handleTimelineRangeChanged}
        itemContent={(_index, item) => (
          <TimelineMessageRow
            item={item}
            accountKey={accountKey}
            actionsAreOpen={activeActionEventId === item.id}
            traceEnabled={timelineTraceIsEnabled}
            isFocused={
              focusedEventId === item.id ||
              navigationHighlightedEventId === item.id
            }
            onBeginEditMessage={onBeginEditMessage}
            onBeginReplyToMessage={onBeginReplyToMessage}
            onCloseMessageActions={() => setActiveActionEventId(null)}
            onRedactMessage={onRedactMessage}
            onRunTimelineAction={runTimelineAction}
            onScrollToTimelineEvent={scrollToTimelineEvent}
            onOpenMessageActions={setActiveActionEventId}
            onToggleReaction={onToggleReaction}
            getMediaGalleryItems={getMediaGalleryItems}
            replyPreview={replyPreviewForItem(item)}
          />
        )}
      />
    </div>
  );
}

type TimelineHeaderProps = {
  accountKey: string;
  boundaryVisible: boolean;
  currentStatus: PaginationState["status"];
  isLoadingOlderMessages: boolean;
  roomId: string | null;
  timelineContext: string;
  onLoadOlderMessages: () => void;
};

function TimelineHeader({
  accountKey,
  boundaryVisible,
  currentStatus,
  isLoadingOlderMessages,
  roomId,
  timelineContext,
  onLoadOlderMessages,
}: TimelineHeaderProps) {
  useEffect(() => {
    logPaginationDiagnostic("pagination.ui.button.render", {
      accountKey,
      roomId,
      timelineContext,
      currentStatus,
      loading: isLoadingOlderMessages,
      buttonVisible: boundaryVisible,
      boundaryVisible,
    });
  }, [
    accountKey,
    boundaryVisible,
    currentStatus,
    isLoadingOlderMessages,
    roomId,
    timelineContext,
  ]);

  function handleClick() {
    logPaginationDiagnostic("pagination.ui.raw_click", {
      accountKey,
      roomId,
      timelineContext,
      currentStatus,
      loading: isLoadingOlderMessages,
      buttonVisible: boundaryVisible,
      boundaryVisible,
    });
    onLoadOlderMessages();
  }

  return (
    <div className="room-timeline-controls">
      <Button
        aria-disabled={isLoadingOlderMessages}
        variant="secondary"
        onClick={handleClick}
      >
        {isLoadingOlderMessages
          ? "Loading older messages..."
          : "Load older messages"}
      </Button>
    </div>
  );
}

type TimelineMessageRowProps = {
  accountKey: string;
  actionsAreOpen: boolean;
  traceEnabled: boolean;
  getMediaGalleryItems: () => ReturnType<typeof mediaGalleryItems>;
  isFocused: boolean;
  item: RoomTimelineItem;
  onBeginEditMessage: (eventId: string, body: string) => void;
  onBeginReplyToMessage: (eventId: string) => void;
  onCloseMessageActions: () => void;
  onOpenMessageActions: (eventId: string) => void;
  onRedactMessage: (eventId: string) => void;
  onRunTimelineAction: (action: () => void) => void;
  onScrollToTimelineEvent: (eventId: string) => void;
  onToggleReaction: (eventId: string, reactionKey: string) => void;
  replyPreview: RoomTimelineReplyPreview | null;
};

const TimelineMessageRow = memo(function TimelineMessageRow({
  accountKey,
  actionsAreOpen,
  traceEnabled,
  getMediaGalleryItems,
  isFocused,
  item,
  onBeginEditMessage,
  onBeginReplyToMessage,
  onCloseMessageActions,
  onOpenMessageActions,
  onRedactMessage,
  onRunTimelineAction,
  onScrollToTimelineEvent,
  onToggleReaction,
  replyPreview,
}: TimelineMessageRowProps) {
  useTimelineRowDebug(traceEnabled, item);
  const longPressTimeoutRef = useRef<number | null>(null);
  const longPressStartRef = useRef<{
    pointerId: number;
    x: number;
    y: number;
  } | null>(null);
  const showsSender = shouldShowSender(item);
  const reactions = item.reactions ?? [];
  const rowClasses = [
    "room-timeline-row",
    `room-timeline-row--${item.groupPosition}`,
    actionsAreOpen ? "room-timeline-row--actions-open" : "",
    item.isOwnMessage ? "room-timeline-row--own" : "",
    isFocused ? "room-timeline-row--focused" : "",
  ]
    .filter(Boolean)
    .join(" ");

  useEffect(() => {
    return () => {
      clearMessageActionLongPress(longPressTimeoutRef, longPressStartRef);
    };
  }, []);

  function runMessageAction(action: () => void) {
    onCloseMessageActions();
    onRunTimelineAction(action);
  }

  function openMessageActions() {
    onOpenMessageActions(item.id);
  }

  function handleContextMenu(event: ReactMouseEvent<HTMLElement>) {
    event.preventDefault();
    clearMessageActionLongPress(longPressTimeoutRef, longPressStartRef);
    openMessageActions();
  }

  function handlePointerDown(event: ReactPointerEvent<HTMLElement>) {
    if (event.pointerType !== "touch" && event.pointerType !== "pen") {
      return;
    }

    if (event.target instanceof Element && event.target.closest("button")) {
      return;
    }

    clearMessageActionLongPress(longPressTimeoutRef, longPressStartRef);
    longPressStartRef.current = {
      pointerId: event.pointerId,
      x: event.clientX,
      y: event.clientY,
    };
    longPressTimeoutRef.current = window.setTimeout(() => {
      longPressTimeoutRef.current = null;
      longPressStartRef.current = null;
      openMessageActions();
    }, messageActionLongPressMilliseconds);
  }

  function handlePointerMove(event: ReactPointerEvent<HTMLElement>) {
    const longPressStart = longPressStartRef.current;
    if (!longPressStart || longPressStart.pointerId !== event.pointerId) {
      return;
    }

    const movedDistance = Math.hypot(
      event.clientX - longPressStart.x,
      event.clientY - longPressStart.y,
    );
    if (movedDistance > messageActionLongPressMoveTolerancePixels) {
      clearMessageActionLongPress(longPressTimeoutRef, longPressStartRef);
    }
  }

  return (
    <article
      className={rowClasses}
      data-event-id={item.id}
      onContextMenu={handleContextMenu}
      onPointerCancel={() =>
        clearMessageActionLongPress(longPressTimeoutRef, longPressStartRef)
      }
      onPointerDown={handlePointerDown}
      onPointerLeave={() =>
        clearMessageActionLongPress(longPressTimeoutRef, longPressStartRef)
      }
      onPointerMove={handlePointerMove}
      onPointerUp={() =>
        clearMessageActionLongPress(longPressTimeoutRef, longPressStartRef)
      }
    >
      <div
        className={`room-timeline-avatar-slot${
          showsSender ? "" : " room-timeline-avatar-slot--empty"
        }`}
      >
        {showsSender ? (
          <span className="room-timeline-avatar">
            {timelineAvatarLabel(item)}
          </span>
        ) : null}
      </div>

      <div className="room-timeline-message">
        <div className="room-timeline-actions">
          {item.canReply ? (
            <Button
              aria-label="Reply"
              iconOnly
              variant="ghost"
              onMouseDown={preventActionButtonFocus}
              onClick={() =>
                runMessageAction(() => onBeginReplyToMessage(item.id))
              }
            >
              <Reply aria-hidden="true" />
            </Button>
          ) : null}
          {item.canReact ? (
            <Button
              aria-label="React"
              iconOnly
              variant="ghost"
              onMouseDown={preventActionButtonFocus}
              onClick={() =>
                runMessageAction(() => onToggleReaction(item.id, "👍"))
              }
            >
              <SmilePlus aria-hidden="true" />
            </Button>
          ) : null}
          {item.canEdit ? (
            <Button
              aria-label="Edit"
              iconOnly
              variant="ghost"
              onMouseDown={preventActionButtonFocus}
              onClick={() =>
                runMessageAction(() =>
                  onBeginEditMessage(item.id, item.body ?? ""),
                )
              }
            >
              <Pencil aria-hidden="true" />
            </Button>
          ) : null}
          {item.canRedact ? (
            <Button
              aria-label="Delete"
              iconOnly
              variant="ghost"
              onMouseDown={preventActionButtonFocus}
              onClick={() => runMessageAction(() => onRedactMessage(item.id))}
            >
              <Trash2 aria-hidden="true" />
            </Button>
          ) : null}
          {item.permalink ? (
            <Button
              aria-label="Copy message link"
              iconOnly
              variant="ghost"
              onMouseDown={preventActionButtonFocus}
              onClick={() =>
                runMessageAction(() => {
                  void navigator.clipboard.writeText(item.permalink);
                })
              }
            >
              <Link aria-hidden="true" />
            </Button>
          ) : null}
        </div>

        {showsSender ? (
          <header className="room-timeline-message-head">
            <span className="room-timeline-sender">
              {item.senderDisplayName}
            </span>
            {item.timeLabel ? (
              <span className="room-timeline-time">{item.timeLabel}</span>
            ) : null}
          </header>
        ) : null}

        {replyPreview ? (
          <button
            className="room-timeline-reply-preview"
            disabled={replyPreview.state !== "resolved"}
            type="button"
            onClick={() => onScrollToTimelineEvent(replyPreview.eventId)}
          >
            <span className="room-timeline-reply-author">
              {replyPreview.senderDisplayName}
            </span>
            <span className="room-timeline-reply-body">
              {replyPreviewLabel(replyPreview)}
            </span>
          </button>
        ) : null}

        <TimelineMedia
          cacheScope={accountKey}
          getGalleryItems={getMediaGalleryItems}
          item={item}
        />

        {item.body || item.isEdited ? (
          <div className="room-timeline-body-row">
            {item.body ? (
              <TimelineMarkdown
                className={`room-timeline-body${
                  item.contentKind === "unableToDecrypt"
                    ? " room-timeline-body--system"
                    : ""
                }`}
                markdown={item.body}
              />
            ) : null}
            {item.isEdited ? (
              <span className="room-timeline-edited">edited</span>
            ) : null}
          </div>
        ) : null}
        {reactions.length > 0 ? (
          <div className="room-timeline-reactions" aria-label="Reactions">
            {reactions.map((reaction) => (
              <button
                key={reaction.key}
                className={`room-timeline-reaction${
                  reaction.reactedByMe ? " room-timeline-reaction--own" : ""
                }`}
                type="button"
                onMouseDown={preventActionButtonFocus}
                onClick={() =>
                  onRunTimelineAction(() =>
                    onToggleReaction(item.id, reaction.key),
                  )
                }
              >
                <span>{reaction.key}</span>
                <span>{reaction.count}</span>
              </button>
            ))}
          </div>
        ) : null}
      </div>
    </article>
  );
});

function captureTimelineScroll(
  root: HTMLDivElement | null,
): TimelineScrollSnapshot | null {
  const scroller = timelineScroller(root);
  if (!scroller) {
    return null;
  }

  const distanceFromBottom =
    scroller.scrollHeight - scroller.clientHeight - scroller.scrollTop;
  return {
    scrollTop: scroller.scrollTop,
    scrollHeight: scroller.scrollHeight,
    clientHeight: scroller.clientHeight,
    wasAtBottom: distanceFromBottom <= bottomAnchorTolerancePixels,
  };
}

function restoreTimelineScroll(
  root: HTMLDivElement | null,
  snapshot: TimelineScrollSnapshot | null,
): void {
  if (!snapshot) {
    return;
  }

  const scrollSnapshot = snapshot;

  function restore() {
    const scroller = timelineScroller(root);
    if (!scroller) {
      return;
    }

    if (scrollSnapshot.wasAtBottom) {
      scroller.scrollTop = timelineMaximumScrollTop(scroller);
      return;
    }

    const maxScrollTop = timelineMaximumScrollTop(scroller);
    const prependedHeightDelta = Math.max(
      0,
      scroller.scrollHeight - scrollSnapshot.scrollHeight,
    );
    scroller.scrollTop = Math.min(
      scrollSnapshot.scrollTop + prependedHeightDelta,
      maxScrollTop,
    );
  }

  requestAnimationFrame(() => {
    restore();
    requestAnimationFrame(restore);
  });
}

function scrollToTimelineBottom(
  root: HTMLDivElement | null,
  virtuoso: VirtuosoHandle | null,
  traceEnabled = false,
): void {
  if (traceEnabled) {
    logTimelineDebug("scroll-to-bottom-before", {
      rawMetrics: rawTimelineScrollerMetrics(root, bottomAnchorTolerancePixels),
    });
  }

  virtuoso?.scrollToIndex({
    index: "LAST",
    align: "end",
    behavior: "auto",
  });
  runAfterAnimationFrames(bottomAnchorRestoreFrameCount, () => {
    const scroller = timelineScroller(root);
    if (!scroller) {
      return;
    }

    scroller.scrollTop = timelineMaximumScrollTop(scroller);
    if (traceEnabled) {
      logTimelineDebug("scroll-to-bottom-after", {
        rawMetrics: rawTimelineScrollerMetrics(
          root,
          bottomAnchorTolerancePixels,
        ),
      });
    }
  });
}

function scrollMountedTimelineEventToAnchor(
  target: StableTimelineEventTarget,
): void {
  const currentTargetTop = target.targetRect.top - target.scrollerRect.top;
  const desiredTargetTop =
    target.scroller.clientHeight * replyNavigationAnchorRatio;
  const requestedScrollTop =
    target.scroller.scrollTop + currentTargetTop - desiredTargetTop;
  const maximumScrollTop = Math.max(
    0,
    target.scroller.scrollHeight - target.scroller.clientHeight,
  );

  target.scroller.scrollTo({
    top: Math.min(Math.max(0, requestedScrollTop), maximumScrollTop),
    behavior: "auto",
  });
}

function timelineScrollerIsAtBottom(root: HTMLDivElement | null): boolean {
  const scroller = timelineScroller(root);
  if (!scroller) {
    return false;
  }

  const distanceFromBottom =
    scroller.scrollHeight - scroller.clientHeight - scroller.scrollTop;
  return distanceFromBottom <= bottomAnchorTolerancePixels;
}

function timelineScroller(root: HTMLDivElement | null): HTMLDivElement | null {
  return root?.querySelector<HTMLDivElement>(".room-timeline-scroller") ?? null;
}

function timelineMaximumScrollTop(scroller: HTMLDivElement): number {
  return Math.max(0, scroller.scrollHeight - scroller.clientHeight);
}

function preventActionButtonFocus(
  event: ReactMouseEvent<HTMLButtonElement>,
): void {
  event.preventDefault();
}

function clearMessageActionLongPress(
  timeoutRef: RefObject<number | null>,
  startRef: RefObject<{
    pointerId: number;
    x: number;
    y: number;
  } | null>,
): void {
  if (timeoutRef.current !== null) {
    window.clearTimeout(timeoutRef.current);
    timeoutRef.current = null;
  }

  startRef.current = null;
}

function replyResolutionKey(roomId: string, eventId: string): string {
  return `${roomId}::${eventId}`;
}

function waitForAnimationFrame(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => resolve());
  });
}

function runAfterAnimationFrames(
  frameCount: number,
  callback: () => void,
): void {
  if (frameCount <= 0) {
    callback();
    return;
  }

  requestAnimationFrame(() => {
    runAfterAnimationFrames(frameCount - 1, callback);
  });
}

function failedReplyPreview(eventId: string): RoomTimelineReplyPreview {
  return {
    eventId,
    state: "failedToLoad",
    senderId: "",
    senderDisplayName: "Reply",
    body: "",
    isRedacted: false,
  };
}

function replyPreviewLabel(replyPreview: RoomTimelineReplyPreview): string {
  if (replyPreview.state === "loading") {
    return "Loading replied message...";
  }

  if (replyPreview.state === "deletedRedacted" || replyPreview.isRedacted) {
    return "Original message deleted";
  }

  if (replyPreview.state === "inaccessible") {
    return "Message not accessible";
  }

  if (replyPreview.state === "failedToLoad") {
    return "Failed to load replied message";
  }

  if (replyPreview.state === "invalidRelation") {
    return "Invalid reply";
  }

  return replyPreview.body;
}

function shouldShowSender(item: RoomTimelineItem): boolean {
  return item.groupPosition === "standalone" || item.groupPosition === "start";
}

function timelineAvatarLabel(item: RoomTimelineItem): string {
  const displayName = item.senderDisplayName?.trim() || item.senderId || "";
  return displayName.slice(0, 2).toUpperCase();
}

export default memo(RoomTimelineView);
