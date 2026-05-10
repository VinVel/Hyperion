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

import { Pill, Typography } from "../../components/ui";
import { type SpaceSummary } from "./appShellAdapters";

type AppShellSpacesViewProps = {
  selectedSpace: SpaceSummary | null;
  visibleSpaces: SpaceSummary[];
  onSelectSpace: (spaceId: string) => void;
};

export default function AppShellSpacesView({
  selectedSpace,
  visibleSpaces,
  onSelectSpace,
}: AppShellSpacesViewProps) {
  return (
    <section
      className="app-shell-main-pane app-shell-main-pane--full"
      aria-label="Spaces browser"
    >
      <div className="app-shell-spaces-layout">
        <header className="app-shell-spaces-head">
          <div className="app-shell-heading-row">
            <Typography as="h2" variant="h2">
              Spaces
            </Typography>
          </div>
        </header>

        <div className="app-shell-space-grid">
          {visibleSpaces.length === 0 ? (
            <Typography muted variant="body">
              No joined spaces are available for this account yet.
            </Typography>
          ) : (
            visibleSpaces.map((space) => (
              <button
                key={space.id}
                className={`app-shell-space-card${
                  selectedSpace?.id === space.id
                    ? " app-shell-space-card--active"
                    : ""
                }`}
                type="button"
                onClick={() => onSelectSpace(space.id)}
              >
                <span className="app-shell-space-icon">
                  {space.accentLabel}
                </span>
                <span className="app-shell-space-copy">
                  <span className="app-shell-space-title-row">
                    <span className="app-shell-space-title">{space.name}</span>
                    {space.isOfficial ? (
                      <Pill tone="primary">Official</Pill>
                    ) : null}
                  </span>
                  <span className="app-shell-space-description">
                    {space.description}
                  </span>
                  <span className="app-shell-space-meta">
                    {space.memberLabel} · {space.activityLabel}
                  </span>
                </span>
              </button>
            ))
          )}
        </div>
      </div>
    </section>
  );
}
