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
  ArrowDownUp,
  Check,
  ChevronDown,
  MessageCircleMore,
  Plus,
  Search,
  SendHorizontal,
} from "lucide-react";
import { type KeyboardEvent, useLayoutEffect, useRef } from "react";
import {
  BackButton,
  Button,
  EmptyState,
  Pill,
  ToolbarField,
  Typography,
} from "../../components/ui";
import {
  type RoomSummary,
  type RoomThreadSort,
  type RoomThreadSummary,
  type RoomTimeline,
  roomThreadSortLabels,
} from "./appShellAdapters";

type AppShellMessagesViewProps = {
  composerValue: string;
  isLoadingOlderMessages: boolean;
  isSendingMessage: boolean;
  isSortMenuOpen: boolean;
  selectedRoomSummary: RoomSummary | null;
  selectedThread: RoomThreadSummary | null;
  selectedTimeline: RoomTimeline | null;
  threadSearchQuery: string;
  threadSort: RoomThreadSort;
  visibleThreads: RoomThreadSummary[];
  onCloseThread: () => void;
  onComposerChange: (value: string) => void;
  onLoadOlderMessages: () => void;
  onOpenThread: (roomId: string) => void;
  onSelectSort: (sort: RoomThreadSort) => void;
  onSendMessage: () => void;
  onThreadSearchChange: (value: string) => void;
  onToggleSortMenu: () => void;
};

export default function AppShellMessagesView({
  composerValue,
  isLoadingOlderMessages,
  isSendingMessage,
  isSortMenuOpen,
  selectedRoomSummary,
  selectedThread,
  selectedTimeline,
  threadSearchQuery,
  threadSort,
  visibleThreads,
  onCloseThread,
  onComposerChange,
  onLoadOlderMessages,
  onOpenThread,
  onSelectSort,
  onSendMessage,
  onThreadSearchChange,
  onToggleSortMenu,
}: AppShellMessagesViewProps) {
  const timelineRef = useRef<HTMLDivElement | null>(null);
  const previousTimelineRoomIdRef = useRef<string | null>(null);
  const canSendMessages = selectedRoomSummary?.canSendMessages === true;
  const composerIsEmpty = composerValue.trim().length === 0;
  const composerPlaceholder =
    selectedRoomSummary?.canSendMessages === false
      ? "You cannot send messages in this room"
      : "Send a message";
  const messageCount = selectedTimeline?.items.length ?? 0;

  function handleComposerKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key !== "Enter") {
      return;
    }

    event.preventDefault();
    onSendMessage();
  }

  function renderThreadList() {
    if (visibleThreads.length === 0) {
      return (
        <Typography muted variant="body">
          No conversations are available for this account yet.
        </Typography>
      );
    }

    return visibleThreads.map((thread) => (
      <button
        key={thread.id}
        className={`app-shell-thread-row${
          selectedThread?.id === thread.id
            ? " app-shell-thread-row--active"
            : ""
        }`}
        type="button"
        onClick={() => onOpenThread(thread.id)}
      >
        <span className="app-shell-thread-avatar">{thread.avatarLabel}</span>
        <span className="app-shell-thread-copy">
          <span className="app-shell-thread-title-row">
            <span className="app-shell-thread-title">{thread.title}</span>
            {thread.unreadCount > 0 ? (
              <span className="app-shell-thread-unread">
                {thread.unreadCount}
              </span>
            ) : null}
          </span>
          <span className="app-shell-thread-meta">
            {thread.participantLabel} · {thread.lastActivityLabel}
          </span>
        </span>
      </button>
    ));
  }

  function renderTimelineItems() {
    if (!selectedTimeline?.items.length) {
      return (
        <div className="app-shell-timeline-item">
          <Typography variant="label">No messages yet</Typography>
          <Typography variant="body">
            No text messages are available in this room yet.
          </Typography>
        </div>
      );
    }

    return selectedTimeline.items.map((item) => (
      <div
        key={item.id}
        className={`app-shell-timeline-item${
          item.isOwnMessage ? " app-shell-timeline-item--own" : ""
        }${
          selectedTimeline.focusedEventId === item.id
            ? " app-shell-timeline-item--highlighted"
            : ""
        }`}
      >
        <Typography variant="label">
          {item.senderDisplayName}
          {item.timeLabel ? ` · ${item.timeLabel}` : ""}
        </Typography>
        <Typography variant="body">
          {item.body}
          {item.isEdited ? " (edited)" : ""}
        </Typography>
      </div>
    ));
  }

  useLayoutEffect(() => {
    const timelineElement = timelineRef.current;
    if (!timelineElement || !selectedTimeline) {
      previousTimelineRoomIdRef.current = selectedTimeline?.roomId ?? null;
      return;
    }

    const roomChanged =
      previousTimelineRoomIdRef.current !== selectedTimeline.roomId;
    previousTimelineRoomIdRef.current = selectedTimeline.roomId;
    if (!roomChanged || selectedTimeline.focusedEventId) {
      return;
    }

    timelineElement.scrollTop = timelineElement.scrollHeight;
  }, [
    selectedTimeline?.focusedEventId,
    selectedTimeline?.items.length,
    selectedTimeline?.roomId,
  ]);

  return (
    <>
      <aside className="app-shell-sidebar" aria-label="Message thread list">
        <div className="app-shell-sidebar-head">
          <div className="app-shell-heading-row">
            <Typography as="h1" variant="h2">
              Conversations
            </Typography>
            <Button
              iconOnly
              aria-label="Start a new chat"
              className="app-shell-square-action"
              variant="secondary"
            >
              <Plus aria-hidden="true" />
            </Button>
          </div>
        </div>

        <div className="app-shell-toolbar">
          <ToolbarField
            icon={<Search aria-hidden="true" />}
            placeholder="Search conversations"
            value={threadSearchQuery}
            onChange={(event) =>
              onThreadSearchChange(event.currentTarget.value)
            }
          />

          <div className="app-shell-sort-menu">
            <button
              aria-expanded={isSortMenuOpen}
              className="app-shell-select"
              type="button"
              onClick={onToggleSortMenu}
            >
              <span className="app-shell-select-copy">
                <ArrowDownUp aria-hidden="true" />
                <span>{roomThreadSortLabels[threadSort]}</span>
              </span>
              <ChevronDown aria-hidden="true" />
            </button>

            {isSortMenuOpen ? (
              <div className="app-shell-sort-menu-popover">
                {Object.entries(roomThreadSortLabels).map(
                  ([sortKey, sortLabel]) => (
                    <button
                      key={sortKey}
                      className={`app-shell-sort-option${
                        threadSort === sortKey
                          ? " app-shell-sort-option--active"
                          : ""
                      }`}
                      type="button"
                      onClick={() => onSelectSort(sortKey as RoomThreadSort)}
                    >
                      <span>{sortLabel}</span>
                      {threadSort === sortKey ? (
                        <Check aria-hidden="true" />
                      ) : null}
                    </button>
                  ),
                )}
              </div>
            ) : null}
          </div>
        </div>

        <div className="app-shell-thread-list">{renderThreadList()}</div>
      </aside>

      <section className="app-shell-main-pane" aria-label="Conversation view">
        {selectedThread ? (
          <div className="app-shell-room">
            <header className="app-shell-room-head">
              <div className="app-shell-room-title-row">
                <BackButton
                  className="app-shell-mobile-back-button"
                  onClick={onCloseThread}
                />

                <Typography as="h2" variant="h2">
                  {selectedRoomSummary?.title ?? selectedThread.title}
                </Typography>
                <Typography variant="bodySmall" muted>
                  {selectedRoomSummary?.participantLabel ??
                    selectedThread.participantLabel}
                  {" · "}
                  {selectedRoomSummary?.homeserverLabel ??
                    selectedThread.homeserverLabel}
                </Typography>
                {selectedRoomSummary?.topic ? (
                  <Typography variant="bodySmall" muted>
                    {selectedRoomSummary.topic}
                  </Typography>
                ) : null}
              </div>
              <Pill tone="secondary">{messageCount} messages</Pill>
            </header>

            <div ref={timelineRef} className="app-shell-room-timeline">
              {selectedTimeline?.nextBefore ? (
                <div className="app-shell-room-timeline-controls">
                  <Button
                    disabled={isLoadingOlderMessages}
                    variant="secondary"
                    onClick={onLoadOlderMessages}
                  >
                    {isLoadingOlderMessages
                      ? "Loading older messages..."
                      : "Load older messages"}
                  </Button>
                </div>
              ) : null}

              {renderTimelineItems()}
            </div>

            <div className="app-shell-room-composer">
              <div className="app-shell-composer-row">
                <div className="app-shell-composer">
                  <input
                    className="app-shell-composer-input"
                    disabled={!canSendMessages || isSendingMessage}
                    placeholder={composerPlaceholder}
                    type="text"
                    value={composerValue}
                    onChange={(event) =>
                      onComposerChange(event.currentTarget.value)
                    }
                    onKeyDown={handleComposerKeyDown}
                  />
                </div>
                <Button
                  aria-label="Send message"
                  className="app-shell-composer-send"
                  disabled={
                    isSendingMessage || !canSendMessages || composerIsEmpty
                  }
                  iconOnly
                  variant="primary"
                  onClick={onSendMessage}
                >
                  <SendHorizontal aria-hidden="true" />
                </Button>
              </div>
            </div>
          </div>
        ) : (
          <EmptyState
            copy="Choose a room from the left to open its conversation."
            graphic={<MessageCircleMore aria-hidden="true" />}
            title="Pick a chat"
          />
        )}
      </section>
    </>
  );
}
