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

import type { RoomTimeline, TimelineIdentity } from "../appShellAdapters";
import { timelineInitialItemIndex } from "../appShellAdapters";
import { reuseUnchangedTimelineItems } from "./helpers";

export type TimelineSession = Omit<TimelineIdentity, "instanceId">;
export type SnapshotSource = "initial" | "update" | "refresh";
export type TimelineModel = {
  session: TimelineSession;
  timeline: RoomTimeline | null;
  pendingUpdates: ReadonlyMap<string, RoomTimeline>;
};

export function createTimelineModel(selection: TimelineSession): TimelineModel {
  return {
    session: { ...selection },
    timeline: null,
    pendingUpdates: new Map(),
  };
}

export function applyTimelineSnapshot(
  model: TimelineModel,
  session: TimelineSession,
  snapshot: RoomTimeline,
  source: SnapshotSource,
): TimelineModel {
  if (model.session !== session || !snapshotMatchesSession(snapshot, session)) {
    return model;
  }
  const current = model.timeline;
  if (!current && source !== "initial") {
    if (source !== "update") return model;
    const pending = model.pendingUpdates.get(
      snapshot.timelineIdentity.instanceId,
    );
    if (pending && pending.revision >= snapshot.revision) return model;
    const pendingUpdates = new Map(model.pendingUpdates);
    pendingUpdates.set(snapshot.timelineIdentity.instanceId, snapshot);
    return { ...model, pendingUpdates };
  }
  if (!current) {
    const pending = model.pendingUpdates.get(
      snapshot.timelineIdentity.instanceId,
    );
    const opened = {
      ...model,
      timeline: snapshot,
      pendingUpdates: new Map<string, RoomTimeline>(),
    };
    // The opening reply binds identity; only a matching subscription can then
    // supersede its rows. A late response never merges an older window into it.
    return pending
      ? applyTimelineSnapshot(opened, session, pending, "update")
      : opened;
  }
  if (
    current.timelineIdentity.instanceId !==
      snapshot.timelineIdentity.instanceId ||
    snapshot.revision <= current.revision
  ) {
    return model;
  }
  const reusedItems = reuseUnchangedTimelineItems(
    current.items,
    snapshot.items,
  );
  const unchanged =
    reusedItems.length === current.items.length &&
    reusedItems.every((item, index) => item === current.items[index]);
  const items = unchanged ? current.items : reusedItems;
  // Preserve the existing Virtuoso prepend bookkeeping while all rows now
  // come from complete SDK snapshots. Structural restoration is handled in 5D.
  const prependCount = purePrependCount(current, snapshot);
  return {
    ...model,
    timeline: {
      ...snapshot,
      items,
      nextBefore: current.nextBefore,
      firstItemIndex:
        (current.firstItemIndex ??
          snapshot.firstItemIndex ??
          timelineInitialItemIndex) - prependCount,
    },
  };
}

function snapshotMatchesSession(
  snapshot: RoomTimeline,
  session: TimelineSession,
): boolean {
  const identity = snapshot.timelineIdentity;
  return (
    identity.accountKey === session.accountKey &&
    identity.roomId === session.roomId &&
    identity.focusedEventId === session.focusedEventId &&
    snapshot.roomId === identity.roomId &&
    snapshot.focusedEventId === identity.focusedEventId &&
    identity.instanceId.length > 0 &&
    Number.isSafeInteger(snapshot.revision) &&
    snapshot.revision >= 0
  );
}

function purePrependCount(
  current: RoomTimeline,
  snapshot: RoomTimeline,
): number {
  const added = snapshot.items.length - current.items.length;
  if (!current.items.length || added <= 0) return 0;
  return current.items.every(
    (item, index) => snapshot.items[index + added]?.id === item.id,
  )
    ? added
    : 0;
}

export function applyTimelinePaginationStatus(
  model: TimelineModel,
  session: TimelineSession,
  instanceId: string,
  status: { roomId: string; nextBefore: string | null; tokenChanged: boolean },
): TimelineModel {
  const current = model.timeline;
  if (
    model.session !== session ||
    !current ||
    current.timelineIdentity.instanceId !== instanceId ||
    status.roomId !== current.roomId ||
    current.nextBefore === status.nextBefore
  )
    return model;
  // Completion supplies only cursor/status compatibility data. Its items never
  // become displayed rows, even if subscription delivery has not happened yet.
  return { ...model, timeline: { ...current, nextBefore: status.nextBefore } };
}
