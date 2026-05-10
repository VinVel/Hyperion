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

import { ArrowDownUp, Check, ChevronDown } from "lucide-react";
import { Typography } from "../../components/ui";
import {
  type RoomThreadKindFilter,
  type RoomThreadSort,
  type RoomThreadSummary,
  roomThreadKindFilterLabels,
  roomThreadSortLabels,
} from "./conversationTypes";
import "./Conversations.css";

type ConversationSidebarProps = {
  isSortMenuOpen: boolean;
  selectedThread: RoomThreadSummary | null;
  threadKindFilter: RoomThreadKindFilter;
  threadSort: RoomThreadSort;
  visibleThreads: RoomThreadSummary[];
  onOpenThread: (roomId: string) => void;
  onSelectSort: (sort: RoomThreadSort) => void;
  onThreadKindFilterChange: (value: RoomThreadKindFilter) => void;
  onToggleSortMenu: () => void;
};

export default function ConversationSidebar({
  isSortMenuOpen,
  selectedThread,
  threadKindFilter,
  threadSort,
  visibleThreads,
  onOpenThread,
  onSelectSort,
  onThreadKindFilterChange,
  onToggleSortMenu,
}: ConversationSidebarProps) {
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
        className={`conversations-thread-row${
          selectedThread?.id === thread.id
            ? " conversations-thread-row--active"
            : ""
        }`}
        type="button"
        onClick={() => onOpenThread(thread.id)}
      >
        <span className="conversations-thread-avatar">
          {thread.avatarLabel}
        </span>
        <span className="conversations-thread-copy">
          <span className="conversations-thread-title-row">
            <span className="conversations-thread-title">{thread.title}</span>
            {thread.unreadCount > 0 ? (
              <span className="conversations-thread-unread">
                {thread.unreadCount}
              </span>
            ) : null}
          </span>
          <span className="conversations-thread-meta">
            {thread.participantLabel} · {thread.lastActivityLabel}
          </span>
        </span>
      </button>
    ));
  }

  return (
    <aside className="conversations-sidebar" aria-label="Message thread list">
      <div className="conversations-sidebar-head">
        <div className="conversations-heading-row">
          <Typography as="h1" variant="h2">
            Conversations
          </Typography>
        </div>
      </div>

      <div
        aria-label="Conversation type"
        className="conversations-thread-kind-switch"
        role="group"
      >
        {Object.entries(roomThreadKindFilterLabels).map(
          ([filterKey, filterLabel]) => (
            <button
              key={filterKey}
              aria-pressed={threadKindFilter === filterKey}
              className={`conversations-thread-kind-option${
                threadKindFilter === filterKey
                  ? " conversations-thread-kind-option--active"
                  : ""
              }`}
              type="button"
              onClick={() =>
                onThreadKindFilterChange(filterKey as RoomThreadKindFilter)
              }
            >
              {filterLabel}
            </button>
          ),
        )}
      </div>

      <div className="conversations-toolbar">
        <div className="conversations-sort-menu">
          <button
            aria-expanded={isSortMenuOpen}
            className="conversations-select"
            type="button"
            onClick={onToggleSortMenu}
          >
            <span className="conversations-select-copy">
              <ArrowDownUp aria-hidden="true" />
              <span>{roomThreadSortLabels[threadSort]}</span>
            </span>
            <ChevronDown aria-hidden="true" />
          </button>

          {isSortMenuOpen ? (
            <div className="conversations-sort-menu-popover">
              {Object.entries(roomThreadSortLabels).map(
                ([sortKey, sortLabel]) => (
                  <button
                    key={sortKey}
                    className={`conversations-sort-option${
                      threadSort === sortKey
                        ? " conversations-sort-option--active"
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

      <div className="conversations-thread-list">{renderThreadList()}</div>
    </aside>
  );
}
