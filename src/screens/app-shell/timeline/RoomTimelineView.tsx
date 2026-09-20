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

import { createTimelineScrollSeek } from "./scrollSeek";
import { waitForRenderedTimeline } from "./paginationRendering";
import type { PaginationViewport } from "../paginationBatch";
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
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import {
  Info,
  Link,
  LoaderCircle,
  Pencil,
  Reply,
  SmilePlus,
  Trash2,
} from "lucide-react";
import { Button, Typography } from "../../../components/ui";
import { isTracingLevelEnabled } from "../../../utils/tracing";
import {
  mapTimelineReplyPreview,
  type BackendRoomTimelineReplyPreview,
  type RoomTimeline,
  type RoomTimelineItem,
  type RoomTimelineReplyPreview,
} from "../appShellAdapters";
import { paginationCanLoadAtTimelineStart } from "../pagination";
import FormattedTimelineBody from "./FormattedTimelineBody";
import TimelineMarkdown from "./TimelineMarkdown";
import TimelineInfoSurface from "./TimelineInfoSurface";
import {
  messageContextActions,
  timelineMessagePresentation,
} from "./presentation";
import TimelineScroller from "./TimelineScroller";
import { type TimelineScrollerContext } from "./TimelineScroller";
import {
  logTimelineDebug,
  logTimelineGeometry,
  logTimelineItemIdentityChanges,
  rawTimelineScrollerMetrics,
  useTimelineRowDebug,
} from "./timelineDebug";
import "./RoomTimelineView.css";
import { initialViewportLocation } from "./viewport";
import { readTimelineBookmark } from "./bookmarks";
import TimelineViewport from "./TimelineViewport";

type RoomTimelineViewProps = {
  isLoadingOlderMessages: boolean;
  timeline: RoomTimeline | null;
  onBeginEditMessage: (eventId: string, body: string) => void;
  onBeginReplyToMessage: (eventId: string) => void;
  onJumpToLatest?: () => void;
  onLoadOlderMessages: (viewport: PaginationViewport) => Promise<void>;
  onRedactMessage: (eventId: string) => void;
  onToggleReaction: (eventId: string, reactionKey: string) => void;
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
// TimelineViewport now preserves them through measured SDK snapshot updates.

const replyResolutionRetryDelayMilliseconds = 1_000;

// Reply jumps should show acknowledgement even when the target is already visible.
const replyNavigationHighlightMilliseconds = 1_400;

// Reply navigation uses an absolute viewport anchor when the target is outside view.
const replyNavigationAnchorRatio = 0.28;

// Use the pre-update reading intent; newly measured rows can temporarily make
// Virtuoso report that a reader who was following is no longer at the bottom.
const followLiveBottom = () => "auto" as const;

// Bottom restores wait for Virtuoso and browser layout to publish final row sizes.
// followOutput starts live-bottom following; the viewport tracks its measured size.

// Touch/pen long press opens message actions without relying on hover.
const messageActionLongPressMilliseconds = 450;

// Small movement during long press is tolerated; real scrolling cancels it.
const messageActionLongPressMoveTolerancePixels = 8;

// Render range updates happen in chunks so slow WebKitGTK frames have rows ready
// before they enter the viewport without retaining the full timeline DOM.
const timelineRenderOverscan = {
  main: 120,
  reverse: 120,
} as const;

// Virtuoso needs headroom to retain the visual anchor while pages prepend.
const timelineInitialItemIndex = 100_000;

function TimelineEmptyPlaceholder() {
  return (
    <div className="room-timeline-empty">
      <Typography variant="label">No messages yet</Typography>
      <Typography variant="body">
        No text messages are available in this room yet.
      </Typography>
    </div>
  );
}

function RoomTimelineView({
  isLoadingOlderMessages,
  timeline,
  onBeginEditMessage,
  onBeginReplyToMessage,
  onJumpToLatest,
  onLoadOlderMessages,
  onRedactMessage,
  onToggleReaction,
}: RoomTimelineViewProps) {
  const [initialBookmark] = useState(
    () =>
      timeline?.readingPosition?.bookmark ??
      (timeline ? readTimelineBookmark(timeline.timelineIdentity) : null),
  );
  // Virtuoso's resize-follow paths treat a callback as enabled even when it
  // returns false. Historical reading must supply the literal disabled value.
  const [followsLiveBottom, setFollowsLiveBottom] = useState(
    !timeline?.focusedEventId &&
      (!initialBookmark || initialBookmark.wasAtBottom),
  );
  const scrollSeek = useMemo(createTimelineScrollSeek, []);
  useLayoutEffect(() => {
    scrollSeek.reset();
  }, [
    scrollSeek,
    timeline?.firstItemIndex,
    timeline?.timelineIdentity.instanceId,
  ]);
  const timelineRootRef = useRef<HTMLDivElement | null>(null);
  const virtuosoRef = useRef<VirtuosoHandle | null>(null);
  const viewportRef = useRef<TimelineViewport | null>(null);
  const navigationHighlightTimeoutRef = useRef<number | null>(null);
  const navigationSequenceRef = useRef(0);
  const resolvingReplyKeysRef = useRef<Set<string>>(new Set());
  const retryingReplyKeysRef = useRef<Set<string>>(new Set());
  const previousTimelineItemsRef = useRef<RoomTimelineItem[]>([]);
  const [resolvedReplyPreviews, setResolvedReplyPreviews] = useState<
    Record<string, RoomTimelineReplyPreview>
  >({});
  const [navigationHighlightedEventId, setNavigationHighlightedEventId] =
    useState<string | null>(null);
  const [activeActionEventId, setActiveActionEventId] = useState<string | null>(
    null,
  );
  const [activeInfoEventId, setActiveInfoEventId] = useState<string | null>(
    null,
  );
  const renderedTimelineRef = useRef(timeline);
  const paginationGestureRef = useRef<AbortController | null>(null);
  useLayoutEffect(() => {
    renderedTimelineRef.current = timeline;
  }, [timeline]);
  useEffect(
    () => () => {
      paginationGestureRef.current?.abort();
      paginationGestureRef.current = null;
    },
    [timeline?.timelineIdentity.instanceId],
  );
  const timelineItems = timeline?.items ?? [];
  const roomId = timeline?.roomId ?? null;
  const focusedEventId = timeline?.focusedEventId ?? null;
  const activeInfoItem = activeInfoEventId
    ? (timelineItems.find((item) => item.id === activeInfoEventId) ?? null)
    : null;
  const timelineTraceIsEnabled = isTracingLevelEnabled("trace");

  function runTimelineAction(action: () => void) {
    action();
  }

  useLayoutEffect(() => {
    if (timelineTraceIsEnabled) {
      logTimelineItemIdentityChanges(
        previousTimelineItemsRef.current,
        timelineItems,
      );
      logTimelineGeometry(
        "presentation-update",
        timelineRootRef.current,
        timelineItems,
        bottomAnchorTolerancePixels,
      );
    }
    previousTimelineItemsRef.current = timelineItems;
  }, [timelineItems, timelineTraceIsEnabled]);

  useEffect(() => {
    return () => {
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

  useEffect(() => {
    if (!activeInfoEventId) return;
    function handleOutsideInfoClick(event: PointerEvent) {
      if (
        event.target instanceof Element &&
        event.target.closest(".timeline-info-panel")
      )
        return;
      setActiveInfoEventId(null);
    }
    document.addEventListener("pointerdown", handleOutsideInfoClick, true);
    return () =>
      document.removeEventListener("pointerdown", handleOutsideInfoClick, true);
  }, [activeInfoEventId]);

  const virtuosoComponents = useMemo(
    () => ({
      Scroller: TimelineScroller,
      EmptyPlaceholder: TimelineEmptyPlaceholder,
      ScrollSeekPlaceholder: TimelineScrollSeekPlaceholder,
    }),
    [],
  );

  const handleTimelineStartReached = useCallback(
    (hasTopScrollIntent: boolean) => {
      const scroller = timelineScroller(timelineRootRef.current);
      if (
        !paginationCanLoadAtTimelineStart(
          isLoadingOlderMessages,
          timeline?.nextBefore ?? null,
          scroller !== null && scroller.scrollTop <= 0,
          hasTopScrollIntent,
        )
      ) {
        return;
      }
      if (!scroller || paginationGestureRef.current) return;
      const controller = new AbortController();
      paginationGestureRef.current = controller;
      const instanceId =
        renderedTimelineRef.current?.timelineIdentity.instanceId;
      function leaveOldestEdge() {
        if (scroller && scroller.scrollTop > 0) controller.abort();
      }
      scroller.addEventListener("scroll", leaveOldestEdge, {
        passive: true,
        signal: controller.signal,
      });
      void onLoadOlderMessages({
        items: renderedTimelineRef.current?.items.map((item) => item.id) ?? [],
        signal: controller.signal,
        settle: (revision, signal) =>
          waitForRenderedTimeline(
            () => renderedTimelineRef.current,
            instanceId,
            revision,
            signal,
            () => !viewportRef.current?.isSettling,
          ),
      }).finally(() => {
        scroller.removeEventListener("scroll", leaveOldestEdge);
        if (paginationGestureRef.current === controller)
          paginationGestureRef.current = null;
      });
    },
    [
      isLoadingOlderMessages,
      onJumpToLatest,
      onLoadOlderMessages,
      timeline?.nextBefore,
      timelineItems.length,
    ],
  );

  const scrollerContext = useMemo<TimelineScrollerContext>(
    () => ({
      onTopScrollIntent: () => handleTimelineStartReached(true),
    }),
    [handleTimelineStartReached],
  );

  function handleVirtuosoScrollingChange(isScrolling: boolean) {
    if (timelineTraceIsEnabled) {
      logTimelineDebug("virtuoso-scrolling-change", {
        isScrolling,
        rawMetrics: rawTimelineScrollerMetrics(
          timelineRootRef.current,
          bottomAnchorTolerancePixels,
        ),
      });
    }
  }

  function scrollToTimelineEvent(eventId: string) {
    viewportRef.current?.releaseFollow();
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

  if (!timeline) return <TimelineEmptyPlaceholder />;

  return (
    <TimelineViewport
      ref={viewportRef}
      onRestorationFailure={() => paginationGestureRef.current?.abort()}
      onFollowingChange={setFollowsLiveBottom}
      onJumpToLatest={onJumpToLatest}
      onRestoringChange={scrollSeek.suspend}
      timeline={timeline}
      rootRef={timelineRootRef}
      virtuosoRef={virtuosoRef}
    >
      <div className="room-timeline-host" ref={timelineRootRef}>
        {isLoadingOlderMessages ? (
          <div
            aria-label="Loading older messages"
            className="room-timeline-pagination-loader"
            role="status"
          >
            <LoaderCircle aria-hidden="true" />
          </div>
        ) : null}
        <Virtuoso
          key={timeline?.roomId ?? "room-timeline"}
          ref={virtuosoRef}
          className="room-timeline"
          components={virtuosoComponents}
          context={scrollerContext}
          atBottomThreshold={bottomAnchorTolerancePixels}
          computeItemKey={(_index, item) => item.id}
          data={timelineItems}
          firstItemIndex={timeline?.firstItemIndex ?? timelineInitialItemIndex}
          atBottomStateChange={(isAtBottom) => {
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
          followOutput={followsLiveBottom ? followLiveBottom : false}
          initialTopMostItemIndex={initialViewportLocation(
            timeline,
            initialBookmark,
          )}
          isScrolling={handleVirtuosoScrollingChange}
          overscan={timelineRenderOverscan}
          scrollSeekConfiguration={scrollSeek.configuration}
          itemContent={(_index, item) => (
            <TimelineMessageRow
              item={item}
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
              onOpenMessageInfo={setActiveInfoEventId}
              onToggleReaction={onToggleReaction}
              replyPreview={replyPreviewForItem(item)}
            />
          )}
        />
        {activeInfoItem ? (
          <TimelineInfoSurface
            item={activeInfoItem}
            onClose={() => setActiveInfoEventId(null)}
          />
        ) : null}
      </div>
    </TimelineViewport>
  );
}

function TimelineScrollSeekPlaceholder() {
  return (
    <div className="room-timeline-scroll-seek-placeholder" aria-hidden="true" />
  );
}

type TimelineMessageRowProps = {
  actionsAreOpen: boolean;
  traceEnabled: boolean;
  isFocused: boolean;
  item: RoomTimelineItem;
  onBeginEditMessage: (eventId: string, body: string) => void;
  onBeginReplyToMessage: (eventId: string) => void;
  onCloseMessageActions: () => void;
  onOpenMessageActions: (eventId: string) => void;
  onOpenMessageInfo: (eventId: string) => void;
  onRedactMessage: (eventId: string) => void;
  onRunTimelineAction: (action: () => void) => void;
  onScrollToTimelineEvent: (eventId: string) => void;
  onToggleReaction: (eventId: string, reactionKey: string) => void;
  replyPreview: RoomTimelineReplyPreview | null;
};

const TimelineMessageRow = memo(function TimelineMessageRow({
  actionsAreOpen,
  traceEnabled,
  isFocused,
  item,
  onBeginEditMessage,
  onBeginReplyToMessage,
  onCloseMessageActions,
  onOpenMessageActions,
  onOpenMessageInfo,
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
  const presentation = timelineMessagePresentation(item, replyPreview);
  const contextActions = messageContextActions(item);
  const reactions = item.reactions ?? [];
  const rowClasses = [
    "room-timeline-row",
    `room-timeline-row--${item.groupPosition}`,
    actionsAreOpen ? "room-timeline-row--actions-open" : "",
    item.isOwnMessage ? "room-timeline-row--own" : "",
    presentation.isTombstone ? "room-timeline-row--tombstone" : "",
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
          {contextActions.includes("reply") ? (
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
          {contextActions.includes("react") ? (
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
          {contextActions.includes("edit") ? (
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
          {contextActions.includes("redact") ? (
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
          {contextActions.includes("copyLink") ? (
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
          <Button
            aria-label="Message information"
            iconOnly
            variant="ghost"
            onMouseDown={preventActionButtonFocus}
            onClick={() => runMessageAction(() => onOpenMessageInfo(item.id))}
          >
            <Info aria-hidden="true" />
          </Button>
        </div>

        {showsSender ? (
          <header className="room-timeline-message-head">
            <span className="room-timeline-sender">
              {presentation.senderDisplayName}
            </span>
            {presentation.timeLabel ? (
              <span className="room-timeline-time">
                {presentation.timeLabel}
              </span>
            ) : null}
          </header>
        ) : null}

        {presentation.replyCard ? (
          <button
            className="room-timeline-reply-preview"
            disabled={!presentation.replyCard.canNavigate}
            type="button"
            onClick={() => {
              if (presentation.replyCard?.canNavigate && replyPreview) {
                onScrollToTimelineEvent(replyPreview.eventId);
              }
            }}
          >
            <span className="room-timeline-reply-author">
              {presentation.replyCard.author}
            </span>
            <span className="room-timeline-reply-body">
              {presentation.replyCard.label}
            </span>
          </button>
        ) : null}

        <div className="room-timeline-body-row">
          {presentation.isPlaceholder ? (
            <TimelineMarkdown
              className="room-timeline-body room-timeline-body--system"
              markdown={presentation.body}
            />
          ) : (
            <FormattedTimelineBody
              body={presentation.body}
              className="room-timeline-body"
              richText={item.richText}
            />
          )}
          {presentation.isEdited ? (
            <span className="room-timeline-edited">edited</span>
          ) : null}
        </div>
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

function timelineScroller(root: HTMLDivElement | null): HTMLDivElement | null {
  return root?.querySelector<HTMLDivElement>(".room-timeline-scroller") ?? null;
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

function shouldShowSender(item: RoomTimelineItem): boolean {
  return item.groupPosition === "standalone" || item.groupPosition === "start";
}

function timelineAvatarLabel(item: RoomTimelineItem): string {
  const displayName = item.senderDisplayName?.trim() || item.senderId || "";
  return displayName.slice(0, 2).toUpperCase();
}

function RoomTimelineInstance(props: RoomTimelineViewProps) {
  return (
    <RoomTimelineView
      key={props.timeline?.timelineIdentity.instanceId ?? "closed"}
      {...props}
    />
  );
}

export default memo(RoomTimelineInstance);
