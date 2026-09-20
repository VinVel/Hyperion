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

import { StrictMode, useState } from "react";
import { createRoot } from "react-dom/client";
import { ThemeProvider } from "../../../components/context";
import { ToastProvider } from "../../../components/ui";
import RoomTimelineView from "./RoomTimelineView";
import {
  applyTimelineSnapshot,
  createTimelineModel,
  type TimelineModel,
} from "./model";
import type { RoomTimeline, RoomTimelineItem } from "../appShellAdapters";
import { captureVisibleAnchors, measureAnchor } from "./viewport";
import { saveTimelineBookmark } from "./bookmarks";
import { PaginationBatch } from "../paginationBatch";
import type { PaginationViewport } from "../paginationBatch";
import "./viewportFixture.css";

const selection = {
  accountKey: "fixture",
  roomId: "!fixture",
  focusedEventId: null,
};
function item(index: number): RoomTimelineItem {
  return {
    id: "$fixture-" + index,
    transactionId: null,
    senderId: "@fixture",
    roomId: selection.roomId,
    senderDisplayName: index % 2 ? "Alice" : "Bob",
    senderAvatarUrl: "",
    body:
      index % 7 === 0
        ? "```typescript\nconst example = 42;\nconsole.log(example);\n```"
        : index % 5 === 0
          ? "Mixed height text. ".repeat(45)
          : "Message " + index,
    formattedBody: "",
    formattedBodyFormat: null,
    richText: null,
    contentKind: "text",
    timestampUnixMs: Date.UTC(2026, 8, 1) + index * 3600000,
    timeLabel: index % 24 === 0 ? "Next day · 00:00" : "12:30",
    isEdited: false,
    isRedacted: false,
    isOwnMessage: index % 2 === 0,
    sendState: "sent",
    decryptionState: "unencrypted",
    groupPosition: "standalone",
    permalink: "",
    canEdit: false,
    canRedact: false,
    canReply: true,
    canReact: true,
    reactions: [],
    receipts: [],
    thread: null,
    threadReplyTo: null,
    replyPreview:
      index % 9 === 0
        ? {
            eventId: "$fixture-10",
            state: "resolved",
            senderId: "@fixture",
            senderDisplayName: "Alice",
            body: "An earlier message with a reply preview",
            isRedacted: false,
          }
        : null,
  };
}
function initial(): TimelineModel {
  const model = createTimelineModel(selection);
  const timeline: RoomTimeline = {
    timelineIdentity: { ...selection, instanceId: "fixture-instance" },
    revision: 1,
    roomId: selection.roomId,
    items: Array.from({ length: 100 }, (_, i) => item(i + 100)),
    firstItemIndex: 100000,
    nextBefore: "more",
    focusedEventId: null,
    redactedEventIds: [],
  };
  return applyTimelineSnapshot(model, model.session, timeline, "initial");
}
let model = initial();
let publish: (value: TimelineModel) => void = () => {};
let setMounted: (value: boolean) => void = () => {};
let requests = 0;
let oldest = 100;
let paginationDelay = 350;
let emptyPages = false;
let holdPagination = false;
let releasePagination: (() => void) | null = null;
const batch = new PaginationBatch({
  request: async () => {
    requests++;
    if (holdPagination)
      await new Promise<void>((resolve) => {
        releasePagination = resolve;
      });
    else await new Promise((resolve) => setTimeout(resolve, paginationDelay));
    if (!emptyPages) {
      oldest -= 10;
      update([
        ...Array.from({ length: 10 }, (_, i) => item(oldest + i)),
        ...model.timeline!.items,
      ]);
    }
    return { reachedStart: false, revision: model.timeline!.revision };
  },
  state: () => {},
  error: (message) => {
    throw Error(String(message));
  },
});
function update(items: RoomTimelineItem[]) {
  model = applyTimelineSnapshot(
    model,
    model.session,
    { ...model.timeline!, items, revision: model.timeline!.revision + 1 },
    "update",
  );
  publish(model);
}

const frame = () =>
  new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
async function settle() {
  // Let Virtuoso's own delayed size retry finish before establishing a baseline.
  await new Promise((resolve) => setTimeout(resolve, 250));
  for (let i = 0; i < 120; i++) {
    const viewport = document.querySelector<HTMLDivElement>(
      ".room-timeline-scroller",
    );
    if (
      !viewport ||
      (!document.querySelector(".room-timeline-scroll-seek-placeholder") &&
        captureVisibleAnchors(viewport).length)
    )
      break;
    await frame();
  }
}

function scroller() {
  return document.querySelector<HTMLDivElement>(".room-timeline-scroller")!;
}

async function measured(name: string, mutate: () => void) {
  document.documentElement.dataset.fixturePhase = name;
  const anchor = captureVisibleAnchors(scroller())[0];
  if (!anchor)
    return {
      name,
      pass: false,
      drift: null,
      peak: 0,
      missing: true,
      eventId: "",
    };
  mutate();
  let peak = 0;
  let missing = false;
  for (let i = 0; i < 60; i++) {
    await frame();
    await new Promise((resolve) => setTimeout(resolve, 0));
    const delta = measureAnchor(
      document.querySelector<HTMLDivElement>(".room-timeline-frozen") ??
        scroller(),
      anchor,
    );
    if (delta === null) missing = true;
    else peak = Math.max(peak, Math.abs(delta));
  }
  const drift = measureAnchor(scroller(), anchor);
  return {
    name,
    eventId: anchor.eventId,
    drift,
    peak,
    missing,
    pass: drift !== null && Math.abs(drift) <= 1 && peak <= 1 && !missing,
  };
}

async function run() {
  // Initial bottom positioning may retry while mixed row sizes are discovered.
  await wait(1500);
  scroller().dispatchEvent(
    new KeyboardEvent("keydown", { key: "PageUp", bubbles: true }),
  );
  scroller().scrollTop = Math.floor(scroller().scrollHeight / 3);
  await settle();
  const results = [];
  results.push(
    await measured("mixed-height prepend", () => {
      oldest -= 10;
      update([
        ...Array.from({ length: 10 }, (_, i) => item(oldest + i)),
        ...model.timeline!.items,
      ]);
    }),
  );
  results.push(
    await measured("historical append", () =>
      update([...model.timeline!.items, item(500)]),
    ),
  );
  results.push(
    await measured("structural replacement above reader", () =>
      update(model.timeline!.items.slice(3)),
    ),
  );
  const visible = captureVisibleAnchors(scroller());
  const removed = visible[0];
  const survivor = visible[1];
  update(model.timeline!.items.filter((row) => row.id !== removed.eventId));
  await settle();
  const drift = measureAnchor(scroller(), survivor);
  results.push({
    name: "removed anchor keeps next visible row",
    drift,
    pass: drift !== null && Math.abs(drift) <= 1,
  });
  results.push(
    await measured("delayed height change above anchor", () => {
      const anchorId = captureVisibleAnchors(scroller())[0].eventId;
      const index = model.timeline!.items.findIndex(
        (row) => row.id === anchorId,
      );
      setTimeout(
        () =>
          update(
            model.timeline!.items.map((row, i) =>
              i === index - 1
                ? { ...row, body: "Delayed height. ".repeat(80) }
                : row,
            ),
          ),
        50,
      );
    }),
  );
  const saved = captureVisibleAnchors(scroller())[0];
  setMounted(false);
  await settle();
  setMounted(true);
  let restorationPeak = 0;
  let restorationMissing = false;
  for (let i = 0; i < 60; i++) {
    await frame();
    await wait(0);
    const viewport = scroller();
    if (!viewport || getComputedStyle(viewport).visibility === "hidden")
      continue;
    const delta = measureAnchor(viewport, saved);
    if (delta === null) restorationMissing = true;
    else restorationPeak = Math.max(restorationPeak, Math.abs(delta));
  }
  await settle();
  const restoredDrift = measureAnchor(scroller(), saved);
  results.push({
    name: "same-session return",
    drift: restoredDrift,
    peak: restorationPeak,
    missing: restorationMissing,
    pass:
      restoredDrift !== null &&
      Math.abs(restoredDrift) <= 1 &&
      restorationPeak <= 1 &&
      !restorationMissing &&
      getComputedStyle(scroller()).visibility !== "hidden",
  });
  results.push(
    await measured("concurrent prepend and arrival", () => {
      oldest -= 2;
      update([
        item(oldest),
        item(oldest + 1),
        ...model.timeline!.items,
        item(501),
      ]);
    }),
  );
  // Establish the live bottom after offscreen rows have been measured, just
  // as continued downward input would; an estimated scrollHeight is not yet it.
  for (let attempt = 0; attempt < 10; attempt++) {
    scroller().dispatchEvent(
      new KeyboardEvent("keydown", { key: "End", bubbles: true }),
    );
    scroller().scrollTop = scroller().scrollHeight;
    await settle();
    if (
      scroller().scrollHeight -
        scroller().clientHeight -
        scroller().scrollTop <=
      1
    )
      break;
  }
  const beforeDistance =
    scroller().scrollHeight - scroller().clientHeight - scroller().scrollTop;
  update([...model.timeline!.items, item(502)]);
  await wait(1500);
  await settle();
  const bottomDistance =
    scroller().scrollHeight - scroller().clientHeight - scroller().scrollTop;
  results.push({
    name: "live bottom follows arrival",
    beforeDistance,
    drift: bottomDistance,
    pass: beforeDistance <= 1 && bottomDistance <= 1,
  });
  return results;
}

const wait = (milliseconds: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, milliseconds));
function key(repeat = false) {
  scroller().dispatchEvent(
    new KeyboardEvent("keydown", { key: "ArrowUp", repeat, bubbles: true }),
  );
}
function touch(type: string, y: number, ended = false) {
  const target = scroller();
  // Desktop WebKitGTK exposes Touch but rejects its constructor. Dispatch the
  // same touch-list fields consumed by the native scroller event listeners.
  const point = { identifier: 1, target, clientX: 100, clientY: y };
  const event = new Event(type, { bubbles: true });
  Object.defineProperties(event, {
    touches: { value: ended ? [] : [point] },
    changedTouches: { value: [point] },
  });
  target.dispatchEvent(event);
}
async function runInput() {
  document.documentElement.dataset.fixturePhase = "input";
  // Establish the oldest edge through the real measured restoration path. A raw
  // scrollTop jump over the entire mixed-height list exercises seek estimates,
  // not the fresh input that this scenario is intended to test.
  setMounted(false);
  await settle();
  saveTimelineBookmark(selection, {
    eventId: model.timeline!.items[0].id,
    offsetPixels: 0,
    wasAtBottom: false,
  });
  setMounted(true);
  await wait(1500);
  await settle();
  const results = [];
  const before = requests;
  results.push({
    name: "programmatic top does not paginate",
    pass: before === 0,
  });
  holdPagination = true;
  paginationDelay = 20;
  emptyPages = false;
  const height = scroller().clientHeight;
  const firstGestureTop = scroller().scrollTop;
  key();
  await wait(3100);
  key(true);
  key();
  results.push({
    name: "slow operation remains locked beyond three seconds",
    firstGestureTop,
    before,
    requests,
    pass:
      requests === before + 1 &&
      !!document.querySelector(".room-timeline-pagination-loader"),
  });
  scroller().dispatchEvent(
    new KeyboardEvent("keydown", { key: "PageDown", bubbles: true }),
  );
  scroller().scrollTop = 300;
  await settle();
  const movingAnchor = captureVisibleAnchors(scroller())[0];
  holdPagination = false;
  releasePagination?.();
  releasePagination = null;
  await wait(350);
  await settle();
  const movingDrift = measureAnchor(scroller(), movingAnchor);
  results.push({
    name: "pagination preserves reader moving away during request",
    drift: movingDrift,
    pass:
      movingDrift !== null &&
      Math.abs(movingDrift) <= 1 &&
      requests === before + 1,
  });
  results.push({
    name: "floating spinner leaves geometry unchanged",
    pass:
      height === scroller().clientHeight &&
      !document.querySelector(".room-timeline-pagination-loader"),
  });
  scroller().dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
  await frame();
  scroller().scrollTop = 0;
  await settle();
  paginationDelay = 20;
  emptyPages = true;
  key(true);
  await wait(50);
  results.push({
    name: "key-repeat alone cannot start a batch",
    pass: requests === before + 1,
  });
  document.documentElement.dataset.fixturePhase = "touch";
  const touchBefore = requests;
  touch("touchstart", 200);
  touch("touchmove", 230);
  await wait(100);
  const waitingSpinner = !!document.querySelector(
    ".room-timeline-pagination-loader",
  );
  await wait(1750);
  touch("touchmove", 260);
  await wait(100);
  results.push({
    name: "touch drag admits one bounded batch and spinner spans waits",
    pass: waitingSpinner && requests === touchBefore + 3,
  });
  touch("touchend", 260, true);
  touch("touchstart", 200);
  touch("touchmove", 230);
  await wait(70);
  results.push({
    name: "a fresh touch drag replenishes the budget",
    pass: requests === touchBefore + 4,
  });
  // Moving away cancels continuation waits without cancelling SDK-owned work.
  scroller().scrollTop = 100;
  touch("touchend", 230, true);
  await wait(100);
  return results;
}

async function runLifecycle() {
  document.documentElement.dataset.fixturePhase = "lifecycle";
  setMounted(false);
  await settle();
  // Start with a measured fractional offset, independent of seek velocity from
  // preceding scenarios, then exercise repeated lifecycle restoration.
  saveTimelineBookmark(selection, {
    eventId: model.timeline!.items[Math.floor(model.timeline!.items.length / 2)].id,
    offsetPixels: -17.25,
    wasAtBottom: false,
  });
  setMounted(true);
  await wait(1500);
  await settle();
  const anchor = captureVisibleAnchors(scroller())[0];
  const baseline = model.timeline!;
  const results = [];
  let instance = 0;
  function open(
    next:
      | typeof selection
      | { accountKey: string; roomId: string; focusedEventId: string },
  ) {
    model = createTimelineModel(next);
    model = applyTimelineSnapshot(
      model,
      model.session,
      {
        ...baseline,
        readingPosition: undefined,
        timelineIdentity: { ...next, instanceId: "recreated-" + ++instance },
        roomId: next.roomId,
        focusedEventId: next.focusedEventId,
        revision: 1,
        firstItemIndex: 100000,
      },
      "initial",
    );
    publish(model);
  }
  for (const [name, other] of [
    ["room A → B → A", { ...selection, roomId: "!other" }],
    [
      "account switch and return",
      { ...selection, accountKey: "other-account" },
    ],
    [
      "live → focused → live",
      { ...selection, focusedEventId: baseline.items[30].id },
    ],
  ] as const) {
    const obsolete = model;
    open(other);
    await wait(1500);
    const active = model;
    model = applyTimelineSnapshot(
      model,
      obsolete.session,
      { ...baseline, revision: 999 },
      "update",
    );
    const rejected = model === active;
    open(selection);
    await wait(1500);
    await settle();
    const drift = measureAnchor(scroller(), anchor);
    results.push({
      name,
      drift,
      pass: rejected && drift !== null && Math.abs(drift) <= 1,
    });
  }
  const oldSession = model.session;
  setMounted(false);
  model = createTimelineModel(selection);
  await settle();
  const closed = model;
  model = applyTimelineSnapshot(
    model,
    oldSession,
    { ...baseline, revision: 999 },
    "update",
  );
  const ignoredAfterClose = model === closed;
  open(selection);
  setMounted(true);
  await wait(1500);
  await settle();
  const drift = measureAnchor(scroller(), anchor);
  results.push({
    name: "closed view ignores late updates and restores into a recreated instance",
    drift,
    pass: ignoredAfterClose && drift !== null && Math.abs(drift) <= 1,
  });
  return results;
}

async function runFailure() {
  document.documentElement.dataset.fixturePhase = "restoration failure";
  const retained = model.timeline!.items;
  update([]);
  await settle();
  const action = (label: string) =>
    Array.from(
      document.querySelectorAll<HTMLButtonElement>(".ui-toast__actions button"),
    ).find((button) => button.textContent === label);
  const offered = !!action("Retry") && !!action("Jump to latest");
  action("Retry")?.click();
  await settle();
  const retryable = !!action("Retry");
  update(retained);
  await wait(1500);
  await settle();
  action("Jump to latest")?.click();
  await wait(1500);
  await settle();
  const distance =
    scroller().scrollHeight - scroller().clientHeight - scroller().scrollTop;
  return [
    {
      name: "missing anchors expose retry and explicit latest recovery",
      pass: offered && retryable && distance <= 1 && !document.querySelector(".room-timeline-frozen, .room-timeline-empty"),
      distance,
    },
  ];
}

declare global {
  interface Window {
    timelineFixture: {
      run: typeof run;
      runInput: typeof runInput;
      runLifecycle: typeof runLifecycle;
      runFailure: typeof runFailure;
      settle: typeof settle;
      status: () => { requests: number; loading: boolean; count: number };
      configure: (delay: number, empty: boolean) => void;
      append: () => void;
    };
  }
}
function Fixture() {
  const [current, setCurrent] = useState(model);
  const [mounted, changeMounted] = useState(true);
  const [loading, setLoading] = useState(false);
  publish = setCurrent;
  setMounted = changeMounted;
  async function paginate(viewport: PaginationViewport) {
    setLoading(true);
    try {
      await batch.start(viewport);
    } finally {
      setLoading(false);
    }
  }
  window.timelineFixture = {
    run,
    runInput,
    runLifecycle,
    runFailure,
    settle,
    configure: (delay, empty) => {
      paginationDelay = delay;
      emptyPages = empty;
    },
    append: () => update([...model.timeline!.items, item(600 + requests)]),
    status: () => ({ requests, loading, count: model.timeline!.items.length }),
  };
  return (
    <div className="timeline-fixture">
      <h1>Timeline viewport fixture</h1>
      <div className="timeline-fixture-viewport">
        {mounted ? (
          <RoomTimelineView
            timeline={current.timeline}
            isLoadingOlderMessages={loading}
            onLoadOlderMessages={paginate}
            onBeginEditMessage={() => {}}
            onBeginReplyToMessage={() => {}}
            onRedactMessage={() => {}}
            onToggleReaction={() => {}}
          />
        ) : null}
      </div>
    </div>
  );
}
export function mountTimelineFixture() {
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <ThemeProvider>
        <ToastProvider>
          <Fixture />
        </ToastProvider>
      </ThemeProvider>
    </StrictMode>,
  );
}
