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

import { useCallback, useRef, useState } from "react";
import type { RoomTimeline } from "../appShellAdapters";
import {
  applyTimelineSnapshot,
  applyTimelinePaginationStatus,
  createTimelineModel,
  type TimelineModel,
  type TimelineSession,
  type SnapshotSource,
} from "./model";

export function useTimelineModel() {
  const timelineModelRef = useRef<TimelineModel | null>(null);
  const [selectedTimeline, setSelectedTimeline] = useState<RoomTimeline | null>(
    null,
  );

  const commitModel = useCallback((next: TimelineModel) => {
    timelineModelRef.current = next;
    setSelectedTimeline(next.timeline);
  }, []);
  const beginTimeline = useCallback(
    (selection: TimelineSession) => {
      const next = createTimelineModel(selection);
      commitModel(next);
      return next.session;
    },
    [commitModel],
  );
  const closeTimeline = useCallback(() => {
    timelineModelRef.current = null;
    setSelectedTimeline(null);
  }, []);
  const acceptTimelineSnapshot = useCallback(
    (
      session: TimelineSession,
      snapshot: RoomTimeline,
      source: SnapshotSource,
    ) => {
      const current = timelineModelRef.current;
      if (!current || current.session !== session) return false;
      const next = applyTimelineSnapshot(current, session, snapshot, source);
      if (next !== current) commitModel(next);
      return next !== current;
    },
    [commitModel],
  );
  const updateTimelineStatus = useCallback(
    (
      session: TimelineSession,
      instanceId: string,
      status: Parameters<typeof applyTimelinePaginationStatus>[3],
    ) => {
      const current = timelineModelRef.current;
      if (!current) return;
      const next = applyTimelinePaginationStatus(
        current,
        session,
        instanceId,
        status,
      );
      if (next !== current) commitModel(next);
    },
    [commitModel],
  );
  return {
    selectedTimeline,
    timelineModelRef,
    beginTimeline,
    closeTimeline,
    acceptTimelineSnapshot,
    updateTimelineStatus,
  };
}
