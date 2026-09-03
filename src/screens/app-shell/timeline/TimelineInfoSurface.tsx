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
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { Button, ScrollArea, Typography } from "../../../components/ui";
import type { RoomTimelineItem } from "../appShellAdapters";
import {
  formatRawEventJson,
  timelineJsonSyntaxTheme,
  timelineInfoPresentation,
  timelineInfoViewLabels,
} from "./infoPresentation";

type TimelineInfoSurfaceProps = { item: RoomTimelineItem; onClose: () => void };

const mobileBreakpointPixels = 760;

export default function TimelineInfoSurface({
  item,
  onClose,
}: TimelineInfoSurfaceProps) {
  const [isMobile, setIsMobile] = useState(
    () => window.matchMedia(`(max-width: ${mobileBreakpointPixels}px)`).matches,
  );
  const [activeView, setActiveView] =
    useState<(typeof timelineInfoViewLabels)[number]>("Easy View");
  const [rawEventJson, setRawEventJson] = useState<string | null>(null);
  const [rawEventError, setRawEventError] = useState<string | null>(null);
  const dialogRef = useRef<HTMLElement | null>(null);
  const windowFrameContent = document.querySelector<HTMLElement>(
    ".ui-window-frame-content",
  );
  const portalHost = windowFrameContent ?? document.body;
  const presentation = timelineInfoPresentation(item, rawEventJson);

  useEffect(() => {
    if (
      activeView !== "Advanced View" ||
      rawEventJson ||
      rawEventError ||
      !item.roomId ||
      !item.id.startsWith("$")
    )
      return;
    void invoke<string>("get_room_event_raw", {
      request: { room_id: item.roomId, event_id: item.id },
    })
      .then(setRawEventJson)
      .catch(() =>
        setRawEventError("Raw event data is unavailable for this item."),
      );
  }, [activeView, item.id, item.roomId, rawEventError, rawEventJson]);

  useEffect(() => {
    dialogRef.current?.focus();
    const query = window.matchMedia(`(max-width: ${mobileBreakpointPixels}px)`);
    const handleChange = (event: MediaQueryListEvent) =>
      setIsMobile(event.matches);
    query.addEventListener("change", handleChange);
    return () => query.removeEventListener("change", handleChange);
  }, []);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const surface = (
    <div
      className={`timeline-info-surface${isMobile ? " timeline-info-surface--mobile" : ""}${windowFrameContent ? " timeline-info-surface--in-window-frame" : ""}`}
      onPointerDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <section
        aria-label="Message information"
        aria-modal={isMobile || undefined}
        className="timeline-info-panel"
        ref={dialogRef}
        role="dialog"
        tabIndex={-1}
      >
        <header className="timeline-info-header">
          <Typography as="h3" variant="h3">
            Message information
          </Typography>
          <Button
            aria-label="Close message information"
            className="timeline-info-close"
            iconOnly
            variant="ghost"
            onClick={onClose}
          >
            <X aria-hidden="true" />
          </Button>
        </header>
        <div
          aria-label="Information view"
          className="timeline-info-view-switcher"
          role="group"
        >
          {timelineInfoViewLabels.map((view) => (
            <button
              aria-pressed={activeView === view}
              className={
                activeView === view
                  ? "timeline-info-view-switcher-button timeline-info-view-switcher-button--active"
                  : "timeline-info-view-switcher-button"
              }
              key={view}
              type="button"
              onClick={() => setActiveView(view)}
            >
              {view}
            </button>
          ))}
        </div>
        {activeView === "Easy View" ? (
          <ScrollArea
            className="timeline-info-scroll"
            contentClassName="timeline-info-content"
          >
            <dl className="timeline-info-fields">
              {presentation.fields.map(([label, value]) => (
                <div key={label}>
                  <dt>{label}</dt>
                  <dd>{value}</dd>
                </div>
              ))}
            </dl>
            {presentation.threadIndicator ? (
              <Button aria-disabled="true" disabled variant="secondary">
                {presentation.threadIndicator.label}
              </Button>
            ) : null}
            <section
              aria-label="Read receipts"
              className="timeline-info-section"
            >
              <Typography variant="label">Read receipts</Typography>
              {presentation.receipts.length ? (
                presentation.receipts.map(([name, timestamp]) => (
                  <Typography key={`${name}-${timestamp}`} variant="bodySmall">
                    {name} · {timestamp}
                  </Typography>
                ))
              ) : (
                <Typography muted variant="bodySmall">
                  No receipt records available.
                </Typography>
              )}
            </section>
          </ScrollArea>
        ) : (
          <ScrollArea
            className="timeline-info-raw-scroll"
            contentClassName="timeline-info-raw"
          >
            {presentation.rawEventJson ? (
              <SyntaxHighlighter
                PreTag="div"
                language="json"
                style={timelineJsonSyntaxTheme}
                wrapLongLines
              >
                {formatRawEventJson(presentation.rawEventJson)}
              </SyntaxHighlighter>
            ) : (
              <pre>{rawEventError ?? "Loading event data…"}</pre>
            )}
          </ScrollArea>
        )}
      </section>
    </div>
  );

  return createPortal(surface, portalHost);
}
