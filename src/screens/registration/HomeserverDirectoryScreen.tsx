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

import { Search } from "lucide-react";
import { BackButton, Button, FeedbackMessage, Pill, Typography } from "../../components/ui";
import type { FeedbackMessage as RegistrationFeedbackMessage, HomeserverDirectoryEntry } from "./registrationShared";
import { flowLabel, homeserverCopy, homeserverHost, homeserverTitle } from "./registrationShared";
import "./HomeserverDirectoryScreen.css";

type HomeserverDirectoryScreenProps = {
  feedback: RegistrationFeedbackMessage | null;
  isLoadingHomeservers: boolean;
  isRefreshingHomeservers: boolean;
  searchQuery: string;
  visibleHomeservers: HomeserverDirectoryEntry[];
  onBack: () => void;
  onOpenHomeserver: (homeserver: HomeserverDirectoryEntry) => void;
  onRefreshHomeservers: () => void;
  onSearchQueryChange: (query: string) => void;
};

export function HomeserverDirectoryScreen({
  feedback,
  isLoadingHomeservers,
  isRefreshingHomeservers,
  searchQuery,
  visibleHomeservers,
  onBack,
  onOpenHomeserver,
  onRefreshHomeservers,
  onSearchQueryChange,
}: HomeserverDirectoryScreenProps) {
  return (
    <section className="registration-screen--directory" aria-labelledby="registration-directory-title">
      <div className="registration-heading-row">
        <BackButton onClick={onBack} />
        <Typography variant="h1" id="registration-directory-title">
          Homeservers with open registration
        </Typography>
      </div>
      <Typography variant="body" muted className="registration-screen-copy">
        Choose a homeserver first. Open the details screen only when you want to
        inspect one more closely.
      </Typography>

      <div className="registration-toolbar">
        <label className="registration-search">
          <span className="registration-search-icon">
            <Search aria-hidden="true" />
          </span>
          <input
            className="ui-field__control registration-search-control"
            name="homeserver-search"
            onChange={(event) => onSearchQueryChange(event.currentTarget.value)}
            placeholder="Search homeservers, domains, or software"
            spellCheck={false}
            type="search"
            value={searchQuery}
          />
        </label>

        <Button
          variant="secondary"
          disabled={isLoadingHomeservers || isRefreshingHomeservers}
          onClick={onRefreshHomeservers}
        >
          {isRefreshingHomeservers ? "Refreshing..." : "Refresh"}
        </Button>
      </div>

      <Typography variant="meta" muted className="registration-toolbar-meta">
        {isLoadingHomeservers
          ? "Loading published registration metadata..."
          : `${visibleHomeservers.length} ${visibleHomeservers.length === 1 ? "homeserver" : "homeservers"} available`}
      </Typography>

      {feedback ? (
        <FeedbackMessage tone={feedback.tone}>
          {feedback.text}
        </FeedbackMessage>
      ) : null}

      {isLoadingHomeservers ? (
        <Typography variant="body" muted className="registration-empty-state">
          Pulling the latest public homeserver directory from the native layer.
        </Typography>
      ) : visibleHomeservers.length > 0 ? (
        <div className="registration-directory-grid" role="list">
          {visibleHomeservers.map((homeserver) => (
            <Button
              key={homeserver.server_id}
              variant="secondary"
              className="registration-server-card"
              onClick={() => onOpenHomeserver(homeserver)}
              role="listitem"
            >
              <div className="registration-server-card-head">
                <div className="registration-server-card-copy">
                  <span className="registration-server-title">
                    {homeserverTitle(homeserver)}
                  </span>
                  <span className="registration-server-subtitle">
                    {homeserverHost(homeserver)}
                  </span>
                </div>

                <Pill tone="secondary">
                  {flowLabel(homeserver.registration_flow)}
                </Pill>
              </div>

              <p className="registration-server-description">
                {homeserverCopy(homeserver)}
              </p>

              <div className="registration-server-meta">
                {homeserver.is_official ? (
                  <Pill tone="primary">Official</Pill>
                ) : null}
                {homeserver.software ? (
                  <Pill>
                    {homeserver.version
                      ? `${homeserver.software} ${homeserver.version}`
                      : homeserver.software}
                  </Pill>
                ) : null}
                {homeserver.longstanding ? (
                  <Pill>Established</Pill>
                ) : null}
              </div>
            </Button>
          ))}
        </div>
      ) : (
        <Typography variant="body" muted className="registration-empty-state">
          No homeservers matched your search. Try a broader query or refresh the
          directory.
        </Typography>
      )}
    </section>
  );
}
