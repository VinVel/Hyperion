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

import type { PaginationViewport } from "./paginationBatch";
import { MessageSquareMore, SendHorizontal } from "lucide-react";
import { type KeyboardEvent } from "react";
import {
  BackButton,
  Button,
  EmptyState,
  Pill,
  Typography,
} from "../../components/ui";
import {
  ConversationSidebar,
  type RoomThreadKindFilter,
  type RoomThreadSort,
  type RoomThreadSummary,
} from "../conversations";
import { type RoomSummary, type RoomTimeline } from "./appShellAdapters";
import { RoomTimelineView } from "./timeline";

type AppShellMessagesViewProps = {
  activeComposerMode: "message" | "edit" | "reply";
  composerValue: string;
  isLoadingOlderMessages: boolean;
  isSendingMessage: boolean;
  isSortMenuOpen: boolean;
  selectedRoomSummary: RoomSummary | null;
  selectedThread: RoomThreadSummary | null;
  selectedTimeline: RoomTimeline | null;
  selectedTypingUsers: string[];
  threadKindFilter: RoomThreadKindFilter;
  threadSort: RoomThreadSort;
  visibleThreads: RoomThreadSummary[];
  onBeginEditMessage: (eventId: string, body: string) => void;
  onBeginReplyToMessage: (eventId: string) => void;
  onCancelComposerMode: () => void;
  onCloseThread: () => void;
  onComposerChange: (value: string) => void;
  onLoadOlderMessages: (viewport: PaginationViewport) => Promise<void>;
  onOpenThread: (roomId: string) => void;
  onRedactMessage: (eventId: string) => void;
  onSelectSort: (sort: RoomThreadSort) => void;
  onSendMessage: () => void;
  onThreadKindFilterChange: (value: RoomThreadKindFilter) => void;
  onToggleReaction: (eventId: string, reactionKey: string) => void;
  onToggleSortMenu: () => void;
};

export default function AppShellMessagesView({
  activeComposerMode,
  composerValue,
  isLoadingOlderMessages,
  isSendingMessage,
  isSortMenuOpen,
  selectedRoomSummary,
  selectedThread,
  selectedTimeline,
  selectedTypingUsers,
  threadKindFilter,
  threadSort,
  visibleThreads,
  onBeginEditMessage,
  onBeginReplyToMessage,
  onCancelComposerMode,
  onCloseThread,
  onComposerChange,
  onLoadOlderMessages,
  onOpenThread,
  onRedactMessage,
  onSelectSort,
  onSendMessage,
  onThreadKindFilterChange,
  onToggleReaction,
  onToggleSortMenu,
}: AppShellMessagesViewProps) {
  const canSendMessages = selectedRoomSummary?.canSendMessages === true;
  const composerIsEmpty = composerValue.trim().length === 0;
  let composerPlaceholder = "Send a message";
  if (selectedRoomSummary?.canSendMessages === false) {
    composerPlaceholder = "You cannot send messages in this room";
  } else if (activeComposerMode === "edit") {
    composerPlaceholder = "Save edited message";
  } else if (activeComposerMode === "reply") {
    composerPlaceholder = "Write a reply";
  }
  const messageCount = selectedTimeline?.items?.length ?? 0;
  const typingLabel = typingIndicatorLabel(selectedTypingUsers ?? []);

  function handleComposerKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key !== "Enter") {
      return;
    }

    event.preventDefault();
    onSendMessage();
  }

  return (
    <>
      <ConversationSidebar
        isSortMenuOpen={isSortMenuOpen}
        selectedThread={selectedThread}
        threadKindFilter={threadKindFilter}
        threadSort={threadSort}
        visibleThreads={visibleThreads}
        onOpenThread={onOpenThread}
        onSelectSort={onSelectSort}
        onThreadKindFilterChange={onThreadKindFilterChange}
        onToggleSortMenu={onToggleSortMenu}
      />

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

            <RoomTimelineView
              isLoadingOlderMessages={isLoadingOlderMessages}
              timeline={selectedTimeline}
              onBeginEditMessage={onBeginEditMessage}
              onBeginReplyToMessage={onBeginReplyToMessage}
              onLoadOlderMessages={onLoadOlderMessages}
              onRedactMessage={onRedactMessage}
              onToggleReaction={onToggleReaction}
            />

            <div className="app-shell-room-composer">
              {typingLabel ? (
                <div className="app-shell-typing-indicator" aria-live="polite">
                  <span className="app-shell-typing-dots" aria-hidden="true">
                    <span />
                    <span />
                    <span />
                  </span>
                  <Typography variant="bodySmall" muted>
                    {typingLabel}
                  </Typography>
                </div>
              ) : null}
              {activeComposerMode !== "message" ? (
                <div className="app-shell-composer-context">
                  <Typography variant="bodySmall" muted>
                    {activeComposerMode === "edit"
                      ? "Editing message"
                      : "Replying to message"}
                  </Typography>
                  <Button variant="ghost" onClick={onCancelComposerMode}>
                    Cancel
                  </Button>
                </div>
              ) : null}
              <div className="app-shell-composer-row">
                <div className="app-shell-composer">
                  <input
                    className="app-shell-composer-input"
                    disabled={!canSendMessages}
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
            graphic={<MessageSquareMore aria-hidden="true" />}
            title="Pick a chat"
          />
        )}
      </section>
    </>
  );
}

function typingIndicatorLabel(users: readonly string[] = []): string | null {
  if (users.length === 0) {
    return null;
  }

  if (users.length === 1) {
    return `${users[0]} is typing`;
  }

  if (users.length === 2) {
    return `${users[0]} and ${users[1]} are typing`;
  }

  return `${users[0]} and ${users.length - 1} others are typing`;
}
