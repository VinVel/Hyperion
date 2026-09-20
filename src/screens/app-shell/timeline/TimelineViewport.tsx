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

import { Component, type ReactNode, type RefObject } from "react";
import type { VirtuosoHandle } from "react-virtuoso";
import { notifyFeedback } from "../../../components/ui/Toast";
import type { RoomTimeline } from "../appShellAdapters";
import { readTimelineBookmark, saveTimelineBookmark } from "./bookmarks";
import {
  captureVisibleAnchors,
  classifyTimelineChange,
  measureAnchor,
  survivingAnchor,
  type ViewportAnchor,
} from "./viewport";

// Allow virtual rows to mount and finish measurement, with a finite failure path.
const restorationTimeoutMilliseconds = 2_000;
// Two stable frames cover Virtuoso's measurement and subsequent list layout.
const settledFrameCount = 2;
// Initial positioning itself spans four SDK-renderer frames, followed by row
// measurement. The scrollIntoView completion now gates measured restoration.
// Match the renderer's existing bottom tolerance.
const bottomTolerancePixels = 8;
// Settle more tightly than the one-pixel acceptance limit to avoid accumulating
// rounding drift across repeated room returns.
const anchorCorrectionTolerancePixels = 0.5;

type Props = {
  timeline: RoomTimeline;
  rootRef: RefObject<HTMLDivElement | null>;
  virtuosoRef: RefObject<VirtuosoHandle | null>;
  children: ReactNode;
  onJumpToLatest?: () => void;
  onRestorationFailure: () => void;
  onRestoringChange: (restoring: boolean) => void;
  onFollowingChange: (following: boolean) => void;
};
type Snapshot = { anchors: ViewportAnchor[]; atBottom: boolean };

// This lifecycle is intentionally a class: hooks cannot read the old DOM in
// React's pre-mutation phase. Rows and firstItemIndex still share one model update.
export default class TimelineViewport extends Component<Props> {
  private anchors: ViewportAnchor[] = [];
  private pending: ViewportAnchor | null = null;
  private frame: number | null = null;
  private observer: ResizeObserver | null = null;
  private mutations: MutationObserver | null = null;
  private detached = false;
  private suppressMeasurements = false;
  private failed = false;
  private followingBottom = false;
  private frozen: HTMLDivElement | null = null;
  private frozenScrollTop = 0;
  private waitingForIndex = false;

  get isSettling() {
    return (
      this.pending !== null || this.suppressMeasurements || this.frozen !== null
    );
  }

  private scroller = () =>
    this.props.rootRef.current?.querySelector<HTMLDivElement>(
      ".room-timeline-scroller",
    ) ?? null;
  private selection = () =>
    this.props.timeline.readingPosition?.selection ??
    this.props.timeline.timelineIdentity;

  componentDidMount() {
    this.detached = false;
    this.suppressMeasurements = false;
    this.pending = null;
    this.failed = false;
    const viewport = this.scroller();
    if (!viewport) return;
    viewport.addEventListener("scroll", this.onScroll, { passive: true });
    viewport.addEventListener("wheel", this.onIntent, { passive: true });
    viewport.addEventListener("touchstart", this.onIntent, { passive: true });
    viewport.addEventListener("keydown", this.onIntent);
    viewport.addEventListener("pointerdown", this.onIntent, { passive: true });
    viewport.addEventListener("pointerup", this.onScroll, { passive: true });
    viewport.addEventListener("touchend", this.onScroll, { passive: true });
    this.observer = new ResizeObserver(this.onResize);
    this.observer.observe(viewport);
    this.mutations = new MutationObserver(this.observeRows);
    this.mutations.observe(viewport, { childList: true, subtree: true });
    this.observeRows();
    const bookmark =
      this.props.timeline.readingPosition?.bookmark ??
      readTimelineBookmark(this.selection());
    if (
      bookmark &&
      !(bookmark.wasAtBottom && !this.props.timeline.focusedEventId)
    ) {
      // Initial estimates must not paint unrelated rows before the saved row mounts.
      this.props.rootRef.current?.classList.add(
        "room-timeline-restoring-initial",
      );
      this.restore(
        survivingAnchor(
          [bookmark, ...(bookmark.nearby ?? [])],
          this.props.timeline.items,
        ) ?? bookmark,
        true,
      );
    } else {
      this.setFollowingBottom(!this.props.timeline.focusedEventId);
      this.capture();
    }
  }

  getSnapshotBeforeUpdate(previous: Props): Snapshot | null {
    if (previous.timeline.items === this.props.timeline.items) return null;
    const viewport = this.scroller();
    if (!viewport) return null;
    const change = classifyTimelineChange(
      previous.timeline.items,
      this.props.timeline.items,
    );
    if (!this.followingBottom && (change === "structural" || change === "same"))
      this.freeze(viewport);
    return {
      anchors: this.pending
        ? [this.pending, ...this.anchors]
        : captureVisibleAnchors(viewport),
      atBottom: this.followingBottom,
    };
  }

  componentDidUpdate(
    previous: Props,
    _state: unknown,
    snapshot: Snapshot | null,
  ) {
    if (!snapshot) return;
    if (this.frozen && !this.frozen.isConnected) {
      this.props.rootRef.current?.appendChild(this.frozen);
      this.frozen.scrollTop = this.frozenScrollTop;
    }
    const change = classifyTimelineChange(
      previous.timeline.items,
      this.props.timeline.items,
    );
    if (change === "prepend") {
      // Never compete with Virtuoso's normal prepend correction.
      this.cancelFrame();
      const interruptedRestoration = this.pending;
      this.pending = null;
      this.suppressMeasurements = true;
      let remaining = settledFrameCount;
      const settle = () => {
        if (this.detached) return;
        if (remaining-- > 0) {
          this.frame = requestAnimationFrame(settle);
          return;
        }
        this.frame = null;
        this.suppressMeasurements = false;
        if (interruptedRestoration) this.restore(interruptedRestoration);
        else {
          this.unfreeze();
          this.capture();
        }
      };
      this.frame = requestAnimationFrame(settle);
      return;
    }
    if (snapshot.atBottom && !this.props.timeline.focusedEventId) return;
    const anchor = survivingAnchor(snapshot.anchors, this.props.timeline.items);
    if (anchor) this.restore(anchor);
    else if (snapshot.anchors.length) this.reportFailure(snapshot.anchors[0]);
    else {
      this.unfreeze();
      this.capture();
    }
  }

  componentWillUnmount() {
    if (!this.pending && !this.failed) this.capture();
    this.detached = true;
    this.unfreeze();
    this.cancelFrame();
    this.observer?.disconnect();
    this.mutations?.disconnect();
    const viewport = this.scroller();
    viewport?.removeEventListener("scroll", this.onScroll);
    viewport?.removeEventListener("wheel", this.onIntent);
    viewport?.removeEventListener("touchstart", this.onIntent);
    viewport?.removeEventListener("keydown", this.onIntent);
    viewport?.removeEventListener("pointerdown", this.onIntent);
    viewport?.removeEventListener("pointerup", this.onScroll);
    viewport?.removeEventListener("touchend", this.onScroll);
  }

  private freeze(viewport: HTMLDivElement) {
    if (this.frozen) return;
    // Retain only the mounted presentation while a structural replacement is
    // measured. This prevents a frame of unrelated rows from reaching paint.
    const clone = viewport.cloneNode(true) as HTMLDivElement;
    const root = this.props.rootRef.current!;
    const rect = viewport.getBoundingClientRect();
    const host = root.getBoundingClientRect();
    clone.classList.add("room-timeline-frozen");
    clone.setAttribute("aria-hidden", "true");
    clone.inert = true;
    Object.assign(clone.style, {
      top: rect.top - host.top + "px",
      left: rect.left - host.left + "px",
      width: rect.width + "px",
      height: rect.height + "px",
    });
    this.frozenScrollTop = viewport.scrollTop;
    this.frozen = clone;
  }

  private unfreeze() {
    this.props.rootRef.current?.classList.remove(
      "room-timeline-restoring-initial",
    );
    this.props.onRestoringChange(false);
    this.frozen?.remove();
    this.frozen = null;
  }

  private cancelFrame() {
    if (this.frame !== null) cancelAnimationFrame(this.frame);
    this.frame = null;
  }

  private observeRows = () => {
    const viewport = this.scroller();
    if (this.detached || !viewport || !this.observer) return;
    this.observer.disconnect();
    this.observer.observe(viewport);
    const list = viewport.querySelector('[data-testid="virtuoso-item-list"]');
    if (list) this.observer.observe(list);
    viewport
      .querySelectorAll("[data-event-id]")
      .forEach((row) => this.observer?.observe(row));
  };

  private setFollowingBottom(value: boolean) {
    this.followingBottom = value;
    this.props.onFollowingChange(value);
  }

  releaseFollow = () => {
    this.setFollowingBottom(false);
    // A resize may arrive before the scroll event for this input. Its old
    // anchor must not undo the reader's movement in that interval.
    this.anchors = [];
  };

  private onIntent = (event: Event) => {
    if (
      event instanceof KeyboardEvent &&
      ![
        "ArrowUp",
        "ArrowDown",
        "PageUp",
        "PageDown",
        "Home",
        "End",
        " ",
      ].includes(event.key)
    )
      return;
    this.releaseFollow();
    // User movement takes precedence over any unfinished measured restoration.
    if (this.pending) {
      this.cancelFrame();
      this.pending = null;
      this.unfreeze();
      this.failed = false;
    }
  };

  private onScroll = () => {
    if (this.pending || this.suppressMeasurements || this.failed) return;
    const viewport = this.scroller();
    if (viewport && !this.followingBottom)
      this.setFollowingBottom(
        !this.props.timeline.focusedEventId &&
          viewport.scrollHeight - viewport.clientHeight - viewport.scrollTop <=
            bottomTolerancePixels,
      );
    if (this.followingBottom) this.followMeasuredBottom();
    this.capture();
  };

  private followMeasuredBottom() {
    const viewport = this.scroller();
    if (viewport) viewport.scrollTop = viewport.scrollHeight;
  }

  private onResize = () => {
    if (this.detached || this.suppressMeasurements || this.failed) return;
    if (this.pending) {
      if (!this.waitingForIndex) this.correct(this.pending);
      return;
    }
    if (this.followingBottom) {
      // Size estimates and upward compensation can settle after followOutput.
      // Only an explicitly following reader tracks the measured live bottom.
      this.followMeasuredBottom();
      return;
    }
    const anchor = survivingAnchor(this.anchors, this.props.timeline.items);
    if (anchor) this.correct(anchor);
    this.capture();
  };

  // Initial/programmatic positioning must not overwrite a saved bookmark
  // before the measured restoration path has had a chance to use it.
  private capture() {
    if (this.pending || this.failed || this.suppressMeasurements) return;
    const viewport = this.scroller();
    if (!viewport) return;
    this.anchors = captureVisibleAnchors(viewport);
    this.save();
  }

  private save() {
    const anchor = this.anchors[0];
    if (!anchor) return;
    saveTimelineBookmark(this.selection(), {
      ...anchor,
      wasAtBottom: this.followingBottom,
      nearby: this.anchors.slice(1),
    });
  }

  private correct(anchor: ViewportAnchor): number | null {
    const viewport = this.scroller();
    if (!viewport) return null;
    const delta = measureAnchor(viewport, anchor);
    if (delta !== null && Math.abs(delta) > anchorCorrectionTolerancePixels) viewport.scrollTop += delta;
    return delta;
  }

  private restore(anchor: ViewportAnchor, initial = false) {
    this.cancelFrame();
    this.pending = anchor;
    this.waitingForIndex = false;
    this.props.onRestoringChange(true);
    this.setFollowingBottom(false);
    this.failed = false;
    this.suppressMeasurements = false;
    const index = this.props.timeline.items.findIndex(
      (item) => item.id === anchor.eventId,
    );
    if (index < 0) {
      this.reportFailure(anchor);
      return;
    }
    let requestedIndex = false;
    const requestIndex = () => {
      requestedIndex = true;
      this.waitingForIndex = true;
      this.props.virtuosoRef.current?.scrollIntoView({
        index,
        calculateViewLocation: () => ({
          index,
          align: "start",
          offset: -anchor.offsetPixels,
          behavior: "auto",
        }),
        done: () => {
          if (!this.detached && this.pending === anchor)
            this.waitingForIndex = false;
        },
      });
    };
    if (initial || this.correct(anchor) === null) requestIndex();
    const deadline = performance.now() + restorationTimeoutMilliseconds;
    let stable = 0;
    const measure = () => {
      if (this.detached || this.pending !== anchor) return;
      const delta = this.waitingForIndex ? null : this.correct(anchor);
      if (delta === null && !requestedIndex) requestIndex();
      stable = delta !== null && Math.abs(delta) <= anchorCorrectionTolerancePixels ? stable + 1 : 0;
      if (stable >= settledFrameCount) {
        this.pending = null;
        this.frame = null;
        this.unfreeze();
        this.capture();
      } else if (performance.now() < deadline)
        this.frame = requestAnimationFrame(measure);
      else this.reportFailure(anchor);
    };
    this.frame = requestAnimationFrame(measure);
  }

  private reportFailure(anchor: ViewportAnchor) {
    this.cancelFrame();
    this.pending = null;
    this.failed = true;
    this.props.onRestorationFailure();
    this.unfreeze();
    notifyFeedback({
      tone: "error",
      text: "Could not restore the saved reading position.",
      actions: [
        {
          label: "Retry",
          onSelect: () => {
            if (!this.detached) this.restore(anchor);
          },
        },
        {
          label: "Jump to latest",
          onSelect: () => {
            if (this.detached) return;
            this.failed = false;
            if (this.props.onJumpToLatest) {
              this.props.onJumpToLatest();
              return;
            }
            this.setFollowingBottom(!this.props.timeline.focusedEventId);
            this.props.virtuosoRef.current?.scrollToIndex({
              index: "LAST",
              align: "end",
              behavior: "auto",
            });
          },
        },
      ],
    });
  }

  render() {
    return this.props.children;
  }
}
