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

import { openUrl } from "@tauri-apps/plugin-opener";
import type { MouseEvent as ReactMouseEvent } from "react";

const permittedLinkSchemes = new Set([
  "ftp:",
  "http:",
  "https:",
  "magnet:",
  "mailto:",
]);

// Text outside an existing anchor is recognized only when it starts with one
// of the Matrix-safe URI schemes supported by the timeline.
const timelineLinkExpression =
  /(?:ftp|https?):\/\/[^\s<>"']+|mailto:[^\s<>"']+|magnet:\?[^\s<>"']+/gi;

type TimelineTextPart =
  { href: string; type: "link" } | { text: string; type: "text" };

export function isPermittedTimelineLink(value: string): boolean {
  try {
    return permittedLinkSchemes.has(new URL(value).protocol);
  } catch {
    return false;
  }
}

export function splitTimelineTextLinks(value: string): TimelineTextPart[] {
  const parts: TimelineTextPart[] = [];
  let textStartIndex = 0;

  for (const match of value.matchAll(timelineLinkExpression)) {
    const matchIndex = match.index;
    const candidate = trimLinkPunctuation(match[0]);
    if (
      matchIndex === undefined ||
      !candidate ||
      !isPermittedTimelineLink(candidate)
    ) {
      continue;
    }

    if (matchIndex > textStartIndex) {
      parts.push({
        text: value.slice(textStartIndex, matchIndex),
        type: "text",
      });
    }
    parts.push({ href: candidate, type: "link" });
    textStartIndex = matchIndex + candidate.length;
  }

  if (textStartIndex < value.length) {
    parts.push({ text: value.slice(textStartIndex), type: "text" });
  }

  return parts.length > 0 ? parts : [{ text: value, type: "text" }];
}

function trimLinkPunctuation(value: string): string {
  return value.replace(/[.,!?;:]+$/, "");
}

export function openTimelineLinkExternally(
  event: ReactMouseEvent<HTMLElement>,
) {
  const target = event.target;
  if (!(target instanceof Element)) return;

  const link = target.closest<HTMLAnchorElement>("a[href]");
  if (!link) return;

  // Never allow timeline anchors to replace the app inside the WebView.
  event.preventDefault();
  const href = link.getAttribute("href");
  if (!href || !isPermittedTimelineLink(href)) return;

  void openUrl(href).catch(() => undefined);
}
