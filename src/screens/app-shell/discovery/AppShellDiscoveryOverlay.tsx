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
import {
  Blocks,
  Check,
  ChevronDown,
  Funnel,
  Hash,
  Search,
  Send,
  UserRound,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  Button,
  ScrollArea,
  ToolbarField,
  Typography,
} from "../../../components/ui";
import {
  discoverySourceLabel,
  mapDiscoveryEntity,
  mapInviteTarget,
} from "./adapters";
import type {
  BackendDiscoveryEntity,
  BackendInviteTarget,
  DiscoveryEntity,
  DiscoveryKind,
  DiscoverySource,
  InviteTarget,
} from "./types";

const discoverySearchDebounceMilliseconds = 180;
const discoveryResultPageSize = 20;

const discoveryKindOptions: Array<{
  kind: DiscoveryKind;
  label: string;
  icon: typeof UserRound;
}> = [
  { kind: "user", label: "Users", icon: UserRound },
  { kind: "room", label: "Rooms", icon: Hash },
  { kind: "space", label: "Spaces", icon: Blocks },
];

const discoverySourceOptions: Array<{
  source: DiscoverySource;
  label: string;
}> = [
  { source: "all", label: "All" },
  { source: "matrix_rooms_info", label: "MatrixRooms.info" },
  { source: "homeserver", label: "Homeserver" },
];

type AppShellDiscoveryOverlayProps = {
  isOpen: boolean;
  onClose: () => void;
  onJoined: () => Promise<void>;
  onInviteSent: () => void;
  onError: (message: string) => void;
};

export default function AppShellDiscoveryOverlay({
  isOpen,
  onClose,
  onJoined,
  onInviteSent,
  onError,
}: AppShellDiscoveryOverlayProps) {
  const [kind, setKind] = useState<DiscoveryKind>("room");
  const [source, setSource] = useState<DiscoverySource>("all");
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<DiscoveryEntity[]>([]);
  const [selectedEntity, setSelectedEntity] = useState<DiscoveryEntity | null>(
    null,
  );
  const [inviteTargets, setInviteTargets] = useState<InviteTarget[]>([]);
  const [selectedInviteRoomId, setSelectedInviteRoomId] = useState("");
  const [isSearching, setIsSearching] = useState(false);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [isJoining, setIsJoining] = useState(false);
  const [isInviting, setIsInviting] = useState(false);
  const [isSourceMenuOpen, setIsSourceMenuOpen] = useState(false);
  const [isInviteTargetMenuOpen, setIsInviteTargetMenuOpen] = useState(false);
  const [isMobileDetailOpen, setIsMobileDetailOpen] = useState(false);
  const resultCacheRef = useRef(new Map<string, DiscoveryResultCacheEntry>());

  const trimmedQuery = query.trim();
  const isShowingRecommendations = trimmedQuery.length === 0;
  const resultSectionTitle = isShowingRecommendations
    ? "Recommendations"
    : "Search results";
  const resultSectionEmptyLabel = discoveryEmptyLabel(
    isSearching,
    isShowingRecommendations,
  );
  const selectedInviteTarget = useMemo(
    () =>
      inviteTargets.find((target) => target.roomId === selectedInviteRoomId) ??
      null,
    [inviteTargets, selectedInviteRoomId],
  );

  useEffect(() => {
    if (!isOpen) {
      resetDiscoveryState();
      return;
    }

    let cancelled = false;
    const searchCriteria = {
      kind,
      query: trimmedQuery,
      source,
    };
    const cacheKey = discoverySearchCacheKey(searchCriteria);
    const cachedResult = resultCacheRef.current.get(cacheKey);

    setSelectedEntity(null);
    setInviteTargets([]);
    setSelectedInviteRoomId("");
    setIsMobileDetailOpen(false);
    setIsLoadingMore(false);

    if (cachedResult) {
      setResults(cachedResult.items);
      setSelectedEntity(cachedResult.items[0] ?? null);
      setIsSearching(false);
      return;
    }

    setResults([]);
    setIsSearching(true);

    async function searchDiscovery() {
      setIsSearching(true);

      try {
        const mappedResults = await loadDiscoveryResults(searchCriteria, 0);
        if (cancelled) {
          return;
        }

        setResults(mappedResults);
        resultCacheRef.current.set(cacheKey, {
          items: mappedResults,
          nextOffset: discoveryResultPageSize,
        });
        setSelectedEntity((currentEntity) =>
          retainSelectedDiscoveryEntity(currentEntity, mappedResults),
        );
        setIsMobileDetailOpen(false);
      } catch {
        if (!cancelled) {
          setResults([]);
          setSelectedEntity(null);
          onError("Discovery search failed.");
        }
      } finally {
        if (!cancelled) {
          setIsSearching(false);
        }
      }
    }

    const timeoutId = window.setTimeout(() => {
      void searchDiscovery();
    }, discoverySearchDebounceMilliseconds);

    return () => {
      cancelled = true;
      window.clearTimeout(timeoutId);
    };
  }, [isOpen, kind, onError, source, trimmedQuery]);

  useEffect(() => {
    if (!isOpen || !selectedEntity) {
      setInviteTargets([]);
      setSelectedInviteRoomId("");
      return;
    }

    let cancelled = false;
    const entity = selectedEntity;

    async function loadInviteTargets() {
      if (entity.kind !== "user") {
        setInviteTargets([]);
        setSelectedInviteRoomId("");
        return;
      }

      try {
        const backendTargets = await invoke<BackendInviteTarget[]>(
          "list_invite_targets",
          {
            request: { user_id: entity.id },
          },
        );
        if (cancelled) {
          return;
        }

        const mappedTargets = backendTargets.map(mapInviteTarget);
        setInviteTargets(mappedTargets);
        setSelectedInviteRoomId((currentRoomId) =>
          retainSelectedInviteRoomId(currentRoomId, mappedTargets),
        );
      } catch {
        if (!cancelled) {
          setInviteTargets([]);
          setSelectedInviteRoomId("");
        }
      }
    }

    void loadInviteTargets();

    return () => {
      cancelled = true;
    };
  }, [isOpen, selectedEntity]);

  if (!isOpen) {
    return null;
  }

  async function loadDiscoveryResults(
    searchCriteria: DiscoverySearchCriteria,
    offset: number,
  ) {
    const backendResults = await invoke<BackendDiscoveryEntity[]>(
      "search_discovery_entities",
      {
        request: {
          kind: searchCriteria.kind,
          query: searchCriteria.query,
          source: searchCriteria.source,
          limit: discoveryResultPageSize,
          offset,
        },
      },
    );

    return backendResults.map(mapDiscoveryEntity);
  }

  async function loadMoreDiscoveryResults() {
    setIsLoadingMore(true);

    try {
      const searchCriteria = { kind, query: trimmedQuery, source };
      const cacheKey = discoverySearchCacheKey(searchCriteria);
      const cachedResult = resultCacheRef.current.get(cacheKey);
      const nextOffset = cachedResult?.nextOffset ?? discoveryResultPageSize;
      const mappedResults = await loadDiscoveryResults(
        searchCriteria,
        nextOffset,
      );
      setResults((currentResults) => {
        const mergedResults = mergeDiscoveryResults(
          currentResults,
          mappedResults,
        );
        resultCacheRef.current.set(cacheKey, {
          items: mergedResults,
          nextOffset: nextOffset + discoveryResultPageSize,
        });
        return mergedResults;
      });
    } catch {
      onError("Could not load more discovery results.");
    } finally {
      setIsLoadingMore(false);
    }
  }

  async function joinSelectedEntity() {
    if (!selectedEntity || selectedEntity.kind === "user") {
      return;
    }

    setIsJoining(true);
    const entity = selectedEntity;

    try {
      await invoke("join_discovery_room", {
        request: {
          room_id_or_alias: entity.alias || entity.id,
          via: entity.via,
        },
      });
      onClose();
      void onJoined();
    } catch (error) {
      onError(discoveryErrorMessage(error, "Could not join this room."));
    } finally {
      setIsJoining(false);
    }
  }

  async function inviteSelectedUser() {
    if (
      !selectedEntity ||
      selectedEntity.kind !== "user" ||
      !selectedInviteRoomId
    ) {
      return;
    }

    setIsInviting(true);

    try {
      await invoke("invite_user_to_room", {
        request: {
          user_id: selectedEntity.id,
          room_id: selectedInviteRoomId,
        },
      });
      onInviteSent();
    } catch {
      onError("Could not send the invite.");
    } finally {
      setIsInviting(false);
    }
  }

  function selectDiscoveryKind(nextKind: DiscoveryKind) {
    setKind(nextKind);
    setSelectedEntity(null);
    setIsMobileDetailOpen(false);
  }

  function selectDiscoveryEntity(entity: DiscoveryEntity) {
    setSelectedEntity(entity);
    setIsMobileDetailOpen(true);
  }

  function resetDiscoveryState() {
    setResults([]);
    setSelectedEntity(null);
    setInviteTargets([]);
    setSelectedInviteRoomId("");
    setIsSearching(false);
    setIsLoadingMore(false);
    setIsSourceMenuOpen(false);
    setIsInviteTargetMenuOpen(false);
    setIsMobileDetailOpen(false);
  }

  return (
    <div
      className="ui-overlay app-shell-discovery-overlay"
      role="dialog"
      aria-modal="true"
    >
      <button
        aria-label="Close discovery"
        className="ui-overlay-scrim ui-overlay-scrim--blurred app-shell-discovery-scrim"
        type="button"
        onClick={onClose}
      />
      <section className="app-shell-discovery-panel">
        <div className="app-shell-discovery-head">
          <Typography as="h2" variant="h2">
            Discover
          </Typography>
          <Button
            aria-label="Close discovery"
            className="app-shell-discovery-close-button"
            iconOnly
            variant="ghost"
            onClick={onClose}
          >
            <X aria-hidden="true" />
          </Button>
        </div>

        <div className="app-shell-discovery-controls">
          <div
            aria-label="Discovery type"
            className="app-shell-discovery-kind-switch"
            role="group"
          >
            {discoveryKindOptions.map((option) => {
              const Icon = option.icon;
              return (
                <button
                  key={option.kind}
                  aria-label={option.label}
                  aria-pressed={kind === option.kind}
                  className={`app-shell-discovery-kind-option${
                    kind === option.kind
                      ? " app-shell-discovery-kind-option--active"
                      : ""
                  }`}
                  type="button"
                  onClick={() => selectDiscoveryKind(option.kind)}
                >
                  <Icon aria-hidden="true" />
                </button>
              );
            })}
          </div>

          <div className="app-shell-discovery-source-menu">
            <Button
              aria-expanded={isSourceMenuOpen}
              aria-label="Filter discovery source"
              className="app-shell-discovery-filter-button"
              iconOnly
              variant="ghost"
              onClick={() =>
                setIsSourceMenuOpen((currentValue) => !currentValue)
              }
            >
              <Funnel aria-hidden="true" />
            </Button>

            {isSourceMenuOpen ? (
              <div className="app-shell-discovery-menu" role="menu">
                {discoverySourceOptions.map((option) => (
                  <button
                    key={option.source}
                    className={`app-shell-discovery-menu-option${
                      source === option.source
                        ? " app-shell-discovery-menu-option--active"
                        : ""
                    }`}
                    role="menuitemradio"
                    aria-checked={source === option.source}
                    type="button"
                    onClick={() => {
                      setSource(option.source);
                      setIsSourceMenuOpen(false);
                    }}
                  >
                    <span>{option.label}</span>
                    {source === option.source ? (
                      <Check aria-hidden="true" />
                    ) : null}
                  </button>
                ))}
              </div>
            ) : null}
          </div>
        </div>

        <ToolbarField
          autoFocus
          icon={<Search aria-hidden="true" />}
          placeholder={discoveryPlaceholder(kind)}
          value={query}
          onChange={(event) => setQuery(event.currentTarget.value)}
        />

        <div
          className={`app-shell-discovery-body${
            isMobileDetailOpen ? " app-shell-discovery-body--detail-open" : ""
          }`}
        >
          <DiscoveryResultSection
            emptyLabel={resultSectionEmptyLabel}
            isLoadingMore={isLoadingMore}
            items={results}
            selectedId={selectedEntity?.id ?? ""}
            title={resultSectionTitle}
            onLoadMore={() => void loadMoreDiscoveryResults()}
            onSelect={selectDiscoveryEntity}
          />

          <DiscoveryDetail
            inviteTargets={inviteTargets}
            isInviteTargetMenuOpen={isInviteTargetMenuOpen}
            isInviting={isInviting}
            isJoining={isJoining}
            selectedEntity={selectedEntity}
            selectedInviteRoomId={selectedInviteRoomId}
            selectedInviteTarget={selectedInviteTarget}
            onBack={() => setIsMobileDetailOpen(false)}
            onInvite={inviteSelectedUser}
            onInviteTargetChange={(roomId) => {
              setSelectedInviteRoomId(roomId);
              setIsInviteTargetMenuOpen(false);
            }}
            onJoin={joinSelectedEntity}
            onToggleInviteTargetMenu={() =>
              setIsInviteTargetMenuOpen((currentValue) => !currentValue)
            }
          />
        </div>
      </section>
    </div>
  );
}

type DiscoverySearchCriteria = {
  kind: DiscoveryKind;
  query: string;
  source: DiscoverySource;
};

type DiscoveryResultCacheEntry = {
  items: DiscoveryEntity[];
  nextOffset: number;
};

type DiscoveryResultSectionProps = {
  emptyLabel: string;
  isLoadingMore: boolean;
  items: DiscoveryEntity[];
  selectedId: string;
  title: string;
  onLoadMore: () => void;
  onSelect: (entity: DiscoveryEntity) => void;
};

function DiscoveryResultSection({
  emptyLabel,
  isLoadingMore,
  items,
  selectedId,
  title,
  onLoadMore,
  onSelect,
}: DiscoveryResultSectionProps) {
  return (
    <ScrollArea
      className="app-shell-discovery-results"
      contentClassName="app-shell-discovery-results-content"
    >
      <Typography className="app-shell-section-label" variant="label">
        {title}
      </Typography>
      {items.length > 0 ? (
        <div className="app-shell-discovery-result-list">
          {items.map((item) => (
            <button
              key={`${item.source}:${item.id}`}
              className={`app-shell-discovery-result${
                selectedId === item.id
                  ? " app-shell-discovery-result--active"
                  : ""
              }`}
              type="button"
              onClick={() => onSelect(item)}
            >
              <span className="app-shell-discovery-result-icon">
                <DiscoveryIcon kind={item.kind} />
              </span>
              <span className="app-shell-discovery-result-copy">
                <span className="app-shell-discovery-result-title">
                  {item.title}
                </span>
                <span className="app-shell-discovery-result-description">
                  {entityMetadata(item)}
                </span>
              </span>
            </button>
          ))}
          <Button
            className="app-shell-discovery-more-button"
            disabled={isLoadingMore}
            onClick={onLoadMore}
          >
            {isLoadingMore ? "Loading" : "Show more"}
          </Button>
        </div>
      ) : (
        <Typography muted variant="body">
          {emptyLabel}
        </Typography>
      )}
    </ScrollArea>
  );
}

type DiscoveryDetailProps = {
  inviteTargets: InviteTarget[];
  isInviteTargetMenuOpen: boolean;
  isInviting: boolean;
  isJoining: boolean;
  selectedEntity: DiscoveryEntity | null;
  selectedInviteRoomId: string;
  selectedInviteTarget: InviteTarget | null;
  onBack: () => void;
  onInvite: () => void;
  onInviteTargetChange: (roomId: string) => void;
  onJoin: () => void;
  onToggleInviteTargetMenu: () => void;
};

function DiscoveryDetail({
  inviteTargets,
  isInviteTargetMenuOpen,
  isInviting,
  isJoining,
  selectedEntity,
  selectedInviteRoomId,
  selectedInviteTarget,
  onBack,
  onInvite,
  onInviteTargetChange,
  onJoin,
  onToggleInviteTargetMenu,
}: DiscoveryDetailProps) {
  if (!selectedEntity) {
    return (
      <div className="app-shell-discovery-detail">
        <Typography muted variant="body">
          Select a result to view details.
        </Typography>
      </div>
    );
  }

  const detailHead = (
    <>
      <Button
        aria-label="Close discovery detail"
        className="app-shell-discovery-mobile-back"
        iconOnly
        variant="ghost"
        onClick={onBack}
      >
        <X aria-hidden="true" />
      </Button>

      <div className="app-shell-discovery-detail-head">
        <span className="app-shell-discovery-detail-icon">
          <DiscoveryIcon kind={selectedEntity.kind} />
        </span>
        <span className="app-shell-discovery-detail-copy">
          <Typography as="h3" variant="h3">
            {selectedEntity.title}
          </Typography>
          <Typography muted variant="body">
            {entityMetadata(selectedEntity)}
          </Typography>
        </span>
      </div>
    </>
  );

  if (selectedEntity.kind !== "user") {
    return (
      <div className="app-shell-discovery-detail">
        {detailHead}

        {selectedEntity.description ? (
          <ScrollArea
            className="app-shell-discovery-detail-description"
            contentClassName="app-shell-discovery-detail-description-content"
          >
            <Typography variant="body">{selectedEntity.description}</Typography>
          </ScrollArea>
        ) : null}

        <Button
          disabled={selectedEntity.alreadyJoined || isJoining}
          variant="primary"
          onClick={onJoin}
        >
          <span>{joinButtonLabel(selectedEntity, isJoining)}</span>
        </Button>
      </div>
    );
  }

  return (
    <div className="app-shell-discovery-detail">
      {detailHead}

      {selectedEntity.description ? (
        <Typography variant="body">{selectedEntity.description}</Typography>
      ) : null}

      <div className="app-shell-discovery-invite">
        <div className="app-shell-discovery-select-menu">
          <button
            aria-expanded={isInviteTargetMenuOpen}
            className="app-shell-discovery-select-trigger"
            disabled={inviteTargets.length === 0}
            type="button"
            onClick={onToggleInviteTargetMenu}
          >
            <span className="app-shell-discovery-select-copy">
              <span className="app-shell-discovery-select-label">
                Invite to
              </span>
              <span className="app-shell-discovery-select-title">
                {selectedInviteTarget?.title ?? "No invite targets"}
              </span>
            </span>
            <ChevronDown aria-hidden="true" />
          </button>

          {isInviteTargetMenuOpen ? (
            <div className="app-shell-discovery-menu app-shell-discovery-menu--invite">
              <ScrollArea
                className="app-shell-discovery-invite-target-list"
                contentClassName="app-shell-discovery-menu-invite-content"
              >
                {inviteTargets.map((target) => (
                  <button
                    key={target.roomId}
                    className={`app-shell-discovery-menu-option${
                      selectedInviteRoomId === target.roomId
                        ? " app-shell-discovery-menu-option--active"
                        : ""
                    }`}
                    type="button"
                    onClick={() => onInviteTargetChange(target.roomId)}
                  >
                    <span>{target.title}</span>
                    {selectedInviteRoomId === target.roomId ? (
                      <Check aria-hidden="true" />
                    ) : null}
                  </button>
                ))}
              </ScrollArea>
            </div>
          ) : null}
        </div>
        <Button
          disabled={isInviting || inviteTargets.length === 0}
          variant="primary"
          onClick={onInvite}
        >
          <Send aria-hidden="true" />
          <span>{isInviting ? "Inviting" : "Invite"}</span>
        </Button>
      </div>
    </div>
  );
}

function DiscoveryIcon({ kind }: { kind: DiscoveryKind }) {
  if (kind === "user") {
    return <UserRound aria-hidden="true" />;
  }

  if (kind === "space") {
    return <Blocks aria-hidden="true" />;
  }

  return <Hash aria-hidden="true" />;
}

function discoveryPlaceholder(kind: DiscoveryKind): string {
  if (kind === "user") {
    return "Search Matrix users";
  }

  if (kind === "space") {
    return "Search public spaces";
  }

  return "Search public rooms";
}

function entityMetadata(entity: DiscoveryEntity): string {
  const parts = [
    entity.alias,
    entity.memberCount === null ? "" : `${entity.memberCount} members`,
    discoverySourceLabel(entity.source),
  ].filter(Boolean);

  return parts.join(" · ");
}

function joinButtonLabel(entity: DiscoveryEntity, isJoining: boolean): string {
  if (entity.alreadyJoined) {
    return "Joined";
  }

  if (isJoining) {
    return "Joining";
  }

  return "Join";
}

function retainSelectedDiscoveryEntity(
  currentEntity: DiscoveryEntity | null,
  mappedResults: DiscoveryEntity[],
): DiscoveryEntity | null {
  if (!currentEntity) {
    return mappedResults[0] ?? null;
  }

  return (
    mappedResults.find((result) => result.id === currentEntity.id) ??
    mappedResults[0] ??
    null
  );
}

function retainSelectedInviteRoomId(
  currentRoomId: string,
  targets: InviteTarget[],
): string {
  if (targets.some((target) => target.roomId === currentRoomId)) {
    return currentRoomId;
  }

  return targets[0]?.roomId ?? "";
}

function mergeDiscoveryResults(
  currentResults: DiscoveryEntity[],
  nextResults: DiscoveryEntity[],
): DiscoveryEntity[] {
  const seenResultKeys = new Set(
    currentResults.map((result) => discoveryResultKey(result)),
  );
  const uniqueNextResults = nextResults.filter(
    (result) => !seenResultKeys.has(discoveryResultKey(result)),
  );

  return [...currentResults, ...uniqueNextResults];
}

function discoveryEmptyLabel(
  isSearching: boolean,
  isShowingRecommendations: boolean,
): string {
  if (isSearching) {
    return "Searching...";
  }

  if (isShowingRecommendations) {
    return "No recommendations yet.";
  }

  return "No matches found.";
}

function discoveryResultKey(result: DiscoveryEntity): string {
  return result.alias || result.id;
}

function discoverySearchCacheKey(criteria: DiscoverySearchCriteria): string {
  return `${criteria.kind}:${criteria.source}:${criteria.query}`;
}

function discoveryErrorMessage(error: unknown, fallback: string): string {
  if (typeof error === "string" && error.trim().length > 0) {
    return error;
  }

  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message;
  }

  return fallback;
}
