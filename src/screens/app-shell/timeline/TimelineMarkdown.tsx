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

const markdownRenderer = new MarkdownIt({
  breaks: false,
  html: false,
  linkify: false,
  typographer: false,
});

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
    />
  );
}

export default memo(TimelineMarkdown);
