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

import { useEffect, useRef } from "react";
import { trace, tracingComponents } from "../../../utils/tracing";
import type { RoomTimelineItem } from "../appShellAdapters";

export function timelineItemMeasurementKey(item: RoomTimelineItem): string {
  return `${item.id}:${item.groupPosition}`;
}

export function useTimelineRowDebug(
  enabled: boolean,
  item: RoomTimelineItem,
): void {
  const renderCountRef = useRef(0);
  renderCountRef.current += 1;
  const measurementKey = timelineItemMeasurementKey(item);

  useEffect(() => {
    if (!enabled) {
      return;
    }

    logTimelineDebug("row-effect-mounted", {
      eventId: item.id,
      groupPosition: item.groupPosition,
      measurementKey,
      renderCount: renderCountRef.current,
    });

    return () => {
      logTimelineDebug("row-effect-cleanup", {
        eventId: item.id,
        groupPosition: item.groupPosition,
        measurementKey,
        renderCount: renderCountRef.current,
      });
    };
  }, [enabled, item.groupPosition, item.id, measurementKey]);

  if (enabled && renderCountRef.current > 1) {
    logTimelineDebug("row-rendered-again", {
      eventId: item.id,
      groupPosition: item.groupPosition,
      measurementKey,
      renderCount: renderCountRef.current,
    });
  }
}

export function logTimelineDebug(label: string, payload: unknown): void {
  trace(`timeline.${label}`, () => ({
    component: tracingComponents.timeline,
    operation: label,
    diagnosticDetails: payload,
  }));
}

export function logTimelineGeometry(
  label: string,
  root: HTMLDivElement | null,
  items: RoomTimelineItem[],
  bottomAnchorTolerancePixels: number,
): void {
  trace("timeline.geometry_snapshot", () => ({
    component: tracingComponents.timeline,
    operation: label,
    itemCount: items.length,
    diagnosticDetails: timelineGeometrySnapshot(
      root,
      items,
      bottomAnchorTolerancePixels,
    ),
  }));
}

export function logTimelineItemIdentityChanges(
  previousItems: RoomTimelineItem[],
  currentItems: RoomTimelineItem[],
): void {
  trace("timeline.item_identity_changes", () => {
    const previousById = new Map(
      previousItems.map((item) => [item.id, item] as const),
    );
    const currentById = new Map(
      currentItems.map((item) => [item.id, item] as const),
    );
    const roleChanges = currentItems
      .map((item) => {
        const previousItem = previousById.get(item.id);
        if (
          !previousItem ||
          previousItem.groupPosition === item.groupPosition
        ) {
          return null;
        }

        return {
          eventId: item.id,
          previousGroupPosition: previousItem.groupPosition,
          nextGroupPosition: item.groupPosition,
          previousMeasurementKey: timelineItemMeasurementKey(previousItem),
          nextMeasurementKey: timelineItemMeasurementKey(item),
        };
      })
      .filter((change) => change !== null);
    const insertedItems = currentItems
      .filter((item) => !previousById.has(item.id))
      .map((item) => ({
        eventId: item.id,
        groupPosition: item.groupPosition,
        measurementKey: timelineItemMeasurementKey(item),
      }));
    const removedItems = previousItems
      .filter((item) => !currentById.has(item.id))
      .map((item) => ({
        eventId: item.id,
        groupPosition: item.groupPosition,
        measurementKey: timelineItemMeasurementKey(item),
      }));

    return {
      component: tracingComponents.timeline,
      operation: "reconcile_item_identity",
      itemCount:
        roleChanges.length + insertedItems.length + removedItems.length,
      diagnosticDetails: { roleChanges, insertedItems, removedItems },
    };
  });
}

export function rawTimelineScrollerMetrics(
  root: HTMLDivElement | null,
  bottomAnchorTolerancePixels: number,
) {
  const scroller = timelineScroller(root);
  if (!scroller) {
    return null;
  }

  const scrollTop = scroller.scrollTop;
  const scrollHeight = scroller.scrollHeight;
  const clientHeight = scroller.clientHeight;
  const maxScrollTop = timelineMaximumScrollTop(scroller);
  const distanceFromBottom = scrollHeight - clientHeight - scrollTop;
  return {
    scrollTop: metric(scrollTop),
    scrollHeight: metric(scrollHeight),
    clientHeight: metric(clientHeight),
    maxScrollTop: metric(maxScrollTop),
    distanceFromBottom: metric(distanceFromBottom),
    atRawBottom: distanceFromBottom <= bottomAnchorTolerancePixels,
  };
}

function timelineGeometrySnapshot(
  root: HTMLDivElement | null,
  items: RoomTimelineItem[],
  bottomAnchorTolerancePixels: number,
) {
  const scroller = timelineScroller(root);
  const scrollerRect = scroller?.getBoundingClientRect() ?? null;
  const listElement = root?.querySelector<HTMLElement>(
    '[data-testid="virtuoso-item-list"]',
  );
  const listRect = listElement?.getBoundingClientRect() ?? null;
  const listStyle = listElement ? window.getComputedStyle(listElement) : null;
  const articleElements = Array.from(
    root?.querySelectorAll<HTMLElement>("[data-event-id]") ?? [],
  );
  const itemById = new Map(items.map((item) => [item.id, item] as const));
  const finalItem = items.length > 0 ? items[items.length - 1] : null;
  const finalArticle = finalItem
    ? (root?.querySelector<HTMLElement>(
        `[data-event-id="${CSS.escape(finalItem.id)}"]`,
      ) ?? null)
    : null;
  const lastArticle =
    articleElements.length > 0
      ? articleElements[articleElements.length - 1]
      : null;
  const finalWrapper = itemWrapperForArticle(finalArticle);
  const lastWrapper = itemWrapperForArticle(lastArticle);
  const finalArticleRect = finalArticle?.getBoundingClientRect() ?? null;
  const finalWrapperRect = finalWrapper?.getBoundingClientRect() ?? null;
  const lastArticleRect = lastArticle?.getBoundingClientRect() ?? null;
  const lastWrapperRect = lastWrapper?.getBoundingClientRect() ?? null;
  const offsetExtentAfterList =
    scroller && listElement
      ? scroller.scrollHeight -
        (listElement.offsetTop + listElement.offsetHeight)
      : null;

  return {
    summary: {
      roomItemCount: items.length,
      renderedArticleCount: articleElements.length,
      finalItem: finalItem
        ? {
            eventId: finalItem.id,
            groupPosition: finalItem.groupPosition,
            measurementKey: timelineItemMeasurementKey(finalItem),
          }
        : null,
      rawMetrics: rawTimelineScrollerMetrics(root, bottomAnchorTolerancePixels),
      list: {
        rect: rectSnapshot(listRect),
        inlineStyle: listElement?.getAttribute("style") ?? null,
        paddingTop: listStyle?.paddingTop ?? null,
        paddingBottom: listStyle?.paddingBottom ?? null,
        marginTop: listStyle?.marginTop ?? null,
        marginBottom: listStyle?.marginBottom ?? null,
      },
      finalArticle: geometryComparison(scroller, finalArticleRect),
      finalWrapper: {
        ...geometryComparison(scroller, finalWrapperRect),
        dataIndex: finalWrapper?.dataset.index ?? null,
        dataItemIndex: finalWrapper?.dataset.itemIndex ?? null,
        knownSize: finalWrapper?.dataset.knownSize ?? null,
      },
      lastRenderedArticle: geometryComparison(scroller, lastArticleRect),
      lastRenderedWrapper: {
        ...geometryComparison(scroller, lastWrapperRect),
        dataIndex: lastWrapper?.dataset.index ?? null,
        dataItemIndex: lastWrapper?.dataset.itemIndex ?? null,
        knownSize: lastWrapper?.dataset.knownSize ?? null,
      },
      scroller: {
        rect: rectSnapshot(scrollerRect),
        className: scroller?.className ?? null,
        inlineStyle: scroller?.getAttribute("style") ?? null,
        computedStyle: scroller ? elementBoxStyle(scroller) : null,
      },
      scrollerOffsetMetrics:
        scroller && listElement
          ? {
              scrollHeight: metric(scroller.scrollHeight),
              clientHeight: metric(scroller.clientHeight),
              listOffsetTop: metric(listElement.offsetTop),
              listOffsetHeight: metric(listElement.offsetHeight),
              listBottomByOffset: metric(
                listElement.offsetTop + listElement.offsetHeight,
              ),
              extraAfterListByOffset: metric(offsetExtentAfterList),
            }
          : null,
      scrollExtentAfterList:
        scroller && listRect
          ? metric(
              scroller.scrollHeight - contentBottomFromRect(scroller, listRect),
            )
          : null,
      listBottomMinusLastWrapperBottom:
        listRect && lastWrapperRect
          ? metric(listRect.bottom - lastWrapperRect.bottom)
          : null,
      listSiblings: listElement ? siblingNodeSnapshots(listElement) : null,
    },
    scrollerChildren: scroller ? directChildSnapshots(scroller) : [],
    scrollerParentChain: scroller ? parentChainSnapshots(scroller, 5) : [],
    listParentChain: listElement ? parentChainSnapshots(listElement, 5) : [],
    renderedItems: articleElements.slice(-8).map((article) => {
      const eventId = article.dataset.eventId ?? "";
      const item = itemById.get(eventId);
      const wrapper = itemWrapperForArticle(article);
      const articleRect = article.getBoundingClientRect();
      const wrapperRect = wrapper?.getBoundingClientRect() ?? null;
      return {
        eventId,
        groupPosition: item?.groupPosition ?? null,
        measurementKey: item ? timelineItemMeasurementKey(item) : null,
        wrapperDataIndex: wrapper?.dataset.index ?? null,
        wrapperItemIndex: wrapper?.dataset.itemIndex ?? null,
        wrapperKnownSize: wrapper?.dataset.knownSize ?? null,
        articleHeight: metric(articleRect.height),
        wrapperHeight: metric(wrapperRect?.height ?? null),
        articleVisualGapToScrollerBottom:
          scroller && scrollerRect
            ? metric(scrollerRect.bottom - articleRect.bottom)
            : null,
        wrapperVisualGapToScrollerBottom:
          scroller && scrollerRect && wrapperRect
            ? metric(scrollerRect.bottom - wrapperRect.bottom)
            : null,
        scrollExtentAfterArticle: scroller
          ? metric(
              scroller.scrollHeight -
                contentBottomFromRect(scroller, articleRect),
            )
          : null,
        scrollExtentAfterWrapper:
          scroller && wrapperRect
            ? metric(
                scroller.scrollHeight -
                  contentBottomFromRect(scroller, wrapperRect),
              )
            : null,
      };
    }),
  };
}

function timelineScroller(root: HTMLDivElement | null): HTMLDivElement | null {
  return root?.querySelector<HTMLDivElement>(".room-timeline-scroller") ?? null;
}

function timelineMaximumScrollTop(scroller: HTMLDivElement): number {
  return Math.max(0, scroller.scrollHeight - scroller.clientHeight);
}

function directChildSnapshots(parent: HTMLElement) {
  return Array.from(parent.children).map((child, index) => {
    if (!(child instanceof HTMLElement)) {
      return {
        index,
        tagName: child.tagName,
      };
    }

    return elementSnapshot(child, index);
  });
}

function siblingNodeSnapshots(element: HTMLElement) {
  return {
    previousSiblings: siblingSnapshots(element, "previousElementSibling"),
    nextSiblings: siblingSnapshots(element, "nextElementSibling"),
  };
}

function siblingSnapshots(
  element: HTMLElement,
  direction: "nextElementSibling" | "previousElementSibling",
) {
  const siblings = [];
  let sibling = element[direction];
  let index = 0;
  while (sibling && index < 6) {
    if (sibling instanceof HTMLElement) {
      siblings.push(elementSnapshot(sibling, index));
    } else {
      siblings.push({
        index,
        tagName: sibling.tagName,
      });
    }

    sibling = sibling[direction];
    index += 1;
  }

  return siblings;
}

function parentChainSnapshots(element: HTMLElement, limit: number) {
  const parents = [];
  let currentElement: HTMLElement | null = element;
  for (let depth = 0; currentElement && depth < limit; depth += 1) {
    parents.push(elementSnapshot(currentElement, depth));
    currentElement = currentElement.parentElement;
  }

  return parents;
}

function elementSnapshot(element: HTMLElement, index: number) {
  const rect = element.getBoundingClientRect();
  return {
    index,
    tagName: element.tagName,
    className: element.className,
    testId: element.getAttribute("data-testid"),
    dataIndex: element.dataset.index ?? null,
    dataItemIndex: element.dataset.itemIndex ?? null,
    dataKnownSize: element.dataset.knownSize ?? null,
    inlineStyle: element.getAttribute("style"),
    boxStyle: elementBoxStyle(element),
    rect: rectSnapshot(rect),
    offsetTop: metric(element.offsetTop),
    offsetHeight: metric(element.offsetHeight),
    offsetBottom: metric(element.offsetTop + element.offsetHeight),
    scrollHeight: metric(element.scrollHeight),
    clientHeight: metric(element.clientHeight),
  };
}

function elementBoxStyle(element: HTMLElement) {
  const style = window.getComputedStyle(element);
  return {
    display: style.display,
    position: style.position,
    boxSizing: style.boxSizing,
    overflow: style.overflow,
    overflowY: style.overflowY,
    height: style.height,
    minHeight: style.minHeight,
    maxHeight: style.maxHeight,
    paddingTop: style.paddingTop,
    paddingBottom: style.paddingBottom,
    marginTop: style.marginTop,
    marginBottom: style.marginBottom,
    borderTopWidth: style.borderTopWidth,
    borderBottomWidth: style.borderBottomWidth,
  };
}

function geometryComparison(
  scroller: HTMLDivElement | null,
  rect: DOMRect | null,
) {
  const scrollerRect = scroller?.getBoundingClientRect() ?? null;
  return {
    rect: rectSnapshot(rect),
    visualGapToScrollerBottom:
      rect && scrollerRect ? metric(scrollerRect.bottom - rect.bottom) : null,
    scrollExtentAfter:
      rect && scroller
        ? metric(scroller.scrollHeight - contentBottomFromRect(scroller, rect))
        : null,
  };
}

function itemWrapperForArticle(
  article: HTMLElement | null | undefined,
): HTMLElement | null {
  return (
    article?.closest<HTMLElement>("[data-index][data-known-size]") ??
    article?.parentElement ??
    null
  );
}

function contentBottomFromRect(
  scroller: HTMLDivElement,
  rect: DOMRect,
): number {
  const scrollerRect = scroller.getBoundingClientRect();
  return scroller.scrollTop + rect.bottom - scrollerRect.top;
}

function rectSnapshot(rect: DOMRect | null) {
  if (!rect) {
    return null;
  }

  return {
    top: metric(rect.top),
    bottom: metric(rect.bottom),
    height: metric(rect.height),
  };
}

function metric(value: number | null): number | null {
  if (value === null) {
    return null;
  }

  return Math.round(value * 100) / 100;
}
