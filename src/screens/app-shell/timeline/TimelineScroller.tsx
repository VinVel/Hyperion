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
  type PointerEvent as ReactPointerEvent,
  type RefObject,
  type TouchEvent as ReactTouchEvent,
  type WheelEvent as ReactWheelEvent,
} from "react";
import { classNames } from "../../../components/ui/classNames";
import { useScrollAreaOverlay } from "../../../components/ui";

export type TimelineScrollerContext = {
  onScrollInteractionStart: () => void;
  onScrollInteractionEnd: () => void;
};

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
    const scrollInteractionEndTimeoutRef = useRef<number | null>(null);
    useScrollAreaOverlay({
      rootRef,
      viewportRef: scrollerRef,
    });

    useEffect(() => {
      return () => {
        if (scrollInteractionEndTimeoutRef.current !== null) {
          window.clearTimeout(scrollInteractionEndTimeoutRef.current);
        }
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
          onPointerCancel={(event) => {
            props.onPointerCancel?.(event);
            clearScheduledScrollInteractionEnd(scrollInteractionEndTimeoutRef);
            handleScrollInteractionEnd(event, context);
          }}
          onPointerDownCapture={(event) => {
            props.onPointerDownCapture?.(event);
            clearScheduledScrollInteractionEnd(scrollInteractionEndTimeoutRef);
            handleScrollInteractionStart(event, context);
          }}
          onPointerUp={(event) => {
            props.onPointerUp?.(event);
            clearScheduledScrollInteractionEnd(scrollInteractionEndTimeoutRef);
            handleScrollInteractionEnd(event, context);
          }}
          onTouchCancel={(event) => {
            props.onTouchCancel?.(event);
            clearScheduledScrollInteractionEnd(scrollInteractionEndTimeoutRef);
            handleScrollInteractionEnd(event, context);
          }}
          onTouchEnd={(event) => {
            props.onTouchEnd?.(event);
            clearScheduledScrollInteractionEnd(scrollInteractionEndTimeoutRef);
            handleScrollInteractionEnd(event, context);
          }}
          onTouchStartCapture={(event) => {
            props.onTouchStartCapture?.(event);
            clearScheduledScrollInteractionEnd(scrollInteractionEndTimeoutRef);
            handleScrollInteractionStart(event, context);
          }}
          onWheelCapture={(event) => {
            props.onWheelCapture?.(event);
            handleScrollInteractionStart(event, context);
            scheduleScrollInteractionEnd(
              scrollInteractionEndTimeoutRef,
              context,
            );
          }}
          ref={(node) => assignScrollerRef(node, scrollerRef, ref)}
          style={style}
        >
          {children}
        </div>
      </div>
    );
  },
);

// Wheel gestures at an already-clamped scroll boundary may not produce a
// Virtuoso scrolling transition, so end wheel ownership after input settles.
const scrollInteractionIdleMilliseconds = 180;

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

function handleScrollInteractionStart(
  _event:
    | ReactPointerEvent<HTMLDivElement>
    | ReactTouchEvent<HTMLDivElement>
    | ReactWheelEvent<HTMLDivElement>,
  context: TimelineScrollerContext | undefined,
) {
  context?.onScrollInteractionStart();
}

function handleScrollInteractionEnd(
  _event: ReactPointerEvent<HTMLDivElement> | ReactTouchEvent<HTMLDivElement>,
  context: TimelineScrollerContext | undefined,
) {
  context?.onScrollInteractionEnd();
}

function scheduleScrollInteractionEnd(
  timeoutRef: RefObject<number | null>,
  context: TimelineScrollerContext | undefined,
) {
  clearScheduledScrollInteractionEnd(timeoutRef);
  timeoutRef.current = window.setTimeout(() => {
    timeoutRef.current = null;
    context?.onScrollInteractionEnd();
  }, scrollInteractionIdleMilliseconds);
}

function clearScheduledScrollInteractionEnd(
  timeoutRef: RefObject<number | null>,
) {
  if (timeoutRef.current === null) {
    return;
  }

  window.clearTimeout(timeoutRef.current);
  timeoutRef.current = null;
}

export default TimelineScroller;
