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

import { Blocks, Hash, MessageSquareMore, Search } from 'lucide-react';
import { ToolbarField, Typography } from '../../../components/ui';
import type { AuthenticatedShellView } from '../appShellAdapters';
import type { SearchResultKind, SearchResultGroup } from './types';

type AppShellSearchOverlayProps = {
  globalSearchQuery: string;
  isOpen: boolean;
  results: SearchResultGroup[];
  statusNotice: string | null;
  onClose: () => void;
  onQueryChange: (value: string) => void;
  onSelectResult: (
    threadId?: string,
    targetView?: AuthenticatedShellView,
    eventId?: string,
  ) => void;
};

export default function AppShellSearchOverlay({
  globalSearchQuery,
  isOpen,
  results,
  statusNotice,
  onClose,
  onQueryChange,
  onSelectResult,
}: AppShellSearchOverlayProps) {
  if (!isOpen) {
    return null;
  }

  return (
    <div className="app-shell-search-overlay" role="dialog" aria-modal="true">
      <button
        aria-label="Close global search"
        className="app-shell-search-scrim"
        type="button"
        onClick={onClose}
      />
      <section className="app-shell-search-dialog">
        <div className="app-shell-search-head">
          <Typography as="h2" variant="h2">
            Global Search
          </Typography>
        </div>

        <ToolbarField
          autoFocus
          icon={<Search aria-hidden="true" />}
          placeholder="Search rooms, messages, and spaces"
          value={globalSearchQuery}
          onChange={(event) => onQueryChange(event.currentTarget.value)}
        />

        {statusNotice ? (
          <Typography className="app-shell-search-status" muted variant="body">
            {statusNotice}
          </Typography>
        ) : null}

        {results.length > 0 ? (
          <div className="app-shell-search-results">
            {results.map((resultGroup) => (
              <section key={resultGroup.kind} className="app-shell-search-group">
                <Typography variant="label" className="app-shell-section-label">
                  {resultGroup.title}
                </Typography>
                <div className="app-shell-search-group-list">
                  {resultGroup.items.map((item) => (
                    <button
                      key={item.id}
                      className="app-shell-search-result"
                      type="button"
                      onClick={() =>
                        onSelectResult(item.threadId, item.targetView, item.eventId)
                      }
                    >
                      <span className="app-shell-search-result-icon">
                        <SearchResultIcon kind={resultGroup.kind} />
                      </span>
                      <span className="app-shell-search-result-copy">
                        <span className="app-shell-search-result-title">{item.title}</span>
                        <span className="app-shell-search-result-description">
                          {item.description}
                        </span>
                      </span>
                    </button>
                  ))}
                </div>
              </section>
            ))}
          </div>
        ) : globalSearchQuery.trim().length > 0 ? (
          <Typography className="app-shell-search-empty" muted variant="body">
            No results matched this query.
          </Typography>
        ) : (
          <Typography className="app-shell-search-empty" muted variant="body">
            Start typing to search
          </Typography>
        )}
      </section>
    </div>
  );
}

function SearchResultIcon({ kind }: { kind: SearchResultKind }) {
  if (kind === 'room') {
    return <Hash aria-hidden="true" />;
  }

  if (kind === 'message') {
    return <MessageSquareMore aria-hidden="true" />;
  }

  return <Blocks aria-hidden="true" />;
}
