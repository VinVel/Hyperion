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

import {
  forwardRef,
  useEffect,
  useRef,
  type ForwardedRef,
  type HTMLAttributes,
  type RefObject,
} from "react";
import { attachTimelineTouchPagination } from "./touchPagination";
import { classNames } from "../../../components/ui/classNames";
import { useScrollAreaOverlay } from "../../../components/ui";

export type TimelineScrollerContext = {
  onTopScrollIntent: () => void;
};

// WebKitGTK loses consistent scroll velocity with extreme free-spin mouse input.
// Keep normal wheel events native and cap only the outlier velocity path.
const timelineMaximumWheelVelocityPixelsPerSecond = 15_000;
const estimatedWheelLineHeightPixels = 16;

type TimelineScrollerProps = HTMLAttributes<HTMLDivElement> & {
  context?: TimelineScrollerContext;
  "data-testid"?: string;
};

const TimelineScroller = forwardRef<HTMLDivElement, TimelineScrollerProps>(
  function TimelineScroller(
    { children, className, context, style, ...props },
    ref,
  ) {
    const rootRef = useRef<HTMLDivElement | null>(null);
    const scrollerRef = useRef<HTMLDivElement | null>(null);
    useScrollAreaOverlay({
      rootRef,
      viewportRef: scrollerRef,
    });
    const topScrollIntentRef = useRef(context?.onTopScrollIntent);
    const previousWheelTimestampRef = useRef<number | null>(null);
    topScrollIntentRef.current = context?.onTopScrollIntent;

    useEffect(() => {
      const scrollerElement = scrollerRef.current;
      if (!scrollerElement) {
        return;
      }
      const activeScroller: HTMLDivElement = scrollerElement;

      function handleWheel(event: WheelEvent) {
        const deltaY = wheelDeltaPixels(event, activeScroller);
        if (deltaY < 0 && activeScroller.scrollTop <= 0) {
          topScrollIntentRef.current?.();
        }

        const previousTimestamp = previousWheelTimestampRef.current;
        previousWheelTimestampRef.current = event.timeStamp;
        if (previousTimestamp === null) {
          return;
        }

        const elapsedMilliseconds = Math.max(
          1,
          event.timeStamp - previousTimestamp,
        );
        const maximumDelta =
          (timelineMaximumWheelVelocityPixelsPerSecond * elapsedMilliseconds) /
          1_000;
        if (Math.abs(deltaY) <= maximumDelta) {
          return;
        }

        event.preventDefault();
        activeScroller.scrollTop += Math.sign(deltaY) * maximumDelta;
      }

      activeScroller.addEventListener("wheel", handleWheel, { passive: false });
      const detachTouchPagination = attachTimelineTouchPagination(
        activeScroller,
        () => topScrollIntentRef.current?.(),
      );
      return () => {
        detachTouchPagination();
        activeScroller.removeEventListener("wheel", handleWheel);
      };
    }, []);

    return (
      <div
        className="ui-scroll-area ui-scroll-area--custom room-timeline-scroller-shell"
        data-overlayscrollbars-initialize=""
        ref={rootRef}
      >
        <div
          {...props}
          className={classNames(
            "ui-scroll-area__viewport",
            "room-timeline-scroller",
            className,
          )}
          data-overlayscrollbars-contents=""
          ref={(node) => assignScrollerRef(node, scrollerRef, ref)}
          style={style}
        >
          {children}
        </div>
      </div>
    );
  },
);

function wheelDeltaPixels(event: WheelEvent, scroller: HTMLDivElement): number {
  if (event.deltaMode === WheelEvent.DOM_DELTA_LINE) {
    return event.deltaY * estimatedWheelLineHeightPixels;
  }
  if (event.deltaMode === WheelEvent.DOM_DELTA_PAGE) {
    return event.deltaY * scroller.clientHeight;
  }
  return event.deltaY;
}

function assignScrollerRef(
  node: HTMLDivElement | null,
  localRef: RefObject<HTMLDivElement | null>,
  forwardedRef: ForwardedRef<HTMLDivElement>,
) {
  localRef.current = node;

  if (typeof forwardedRef === "function") {
    forwardedRef(node);
    return;
  }

  if (forwardedRef) {
    forwardedRef.current = node;
  }
}

export default TimelineScroller;
