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

import MarkdownIt from "markdown-it";
import { memo, useMemo } from "react";
import { openTimelineLinkExternally } from "./timelineLinks";

const markdownRenderer = new MarkdownIt({
  breaks: false,
  html: false,
  linkify: true,
  typographer: false,
});

// Plain timeline bodies only auto-link the same five schemes as formatted HTML.
markdownRenderer.linkify.set({
  fuzzyEmail: false,
  fuzzyIP: false,
  fuzzyLink: false,
});
markdownRenderer.linkify.add("//", null);
markdownRenderer.linkify.add("magnet:", { validate: validateMagnetUri });

type TimelineMarkdownProps = {
  className?: string;
  markdown: string;
};

function TimelineMarkdown({ className, markdown }: TimelineMarkdownProps) {
  const renderedMarkdown = useMemo(
    () => markdownRenderer.render(markdown),
    [markdown],
  );

  return (
    <div
      className={className}
      dangerouslySetInnerHTML={{ __html: renderedMarkdown }}
      onClick={openTimelineLinkExternally}
    />
  );
}

function validateMagnetUri(text: string, position: number): number {
  const magnetQuery = text.slice(position).match(/^\?[^\s<>()]+/);
  return magnetQuery?.[0].replace(/[.,!?;:]+$/, "").length ?? 0;
}

export default memo(TimelineMarkdown);
