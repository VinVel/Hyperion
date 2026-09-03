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

import { createElement, memo, type CSSProperties, type ReactNode } from "react";
import type { BackendTimelineRichTextNode } from "../appShellAdapters";
import TimelineMarkdown from "./TimelineMarkdown";
import { openTimelineLinkExternally } from "./timelineLinks";

type FormattedTimelineBodyProps = {
  body: string;
  className?: string;
  richText: BackendTimelineRichTextNode[] | null;
};

function FormattedTimelineBody({
  body,
  className,
  richText,
}: FormattedTimelineBodyProps) {
  if (!richText) {
    return <TimelineMarkdown className={className} markdown={body} />;
  }

  return (
    <div className={className} onClick={openTimelineLinkExternally}>
      {renderNodes(richText)}
    </div>
  );
}

function renderNodes(
  nodes: BackendTimelineRichTextNode[],
  keyPrefix = "",
): ReactNode[] {
  return nodes.map((node, index) => {
    const key = `${keyPrefix}${index}`;
    if (node.type === "text") return node.text;
    if (node.tag === "img") return node.attributes.alt ?? "";

    const props = nodeProperties(node, key);
    if (node.tag === "br" || node.tag === "hr") {
      return createElement(node.tag, props);
    }
    return createElement(
      node.tag,
      props,
      renderNodes(node.children, `${key}-`),
    );
  });
}

function nodeProperties(
  node: Extract<BackendTimelineRichTextNode, { type: "element" }>,
  key: string,
) {
  const { attributes } = node;
  const style: CSSProperties = {};
  if (attributes.color) style.color = attributes.color;
  if (attributes.background_color)
    style.backgroundColor = attributes.background_color;

  return {
    key,
    ...(attributes.href ? { href: attributes.href } : {}),
    ...(attributes.target ? { target: attributes.target } : {}),
    ...(attributes.target ? { rel: "noopener noreferrer" } : {}),
    ...(attributes.language ? { className: attributes.language } : {}),
    ...(attributes.start === null || attributes.start === undefined
      ? {}
      : { start: attributes.start }),
    ...(Object.keys(style).length > 0 ? { style } : {}),
    ...(attributes.spoiler === null || attributes.spoiler === undefined
      ? {}
      : { "data-mx-spoiler": attributes.spoiler }),
    ...(attributes.maths === null || attributes.maths === undefined
      ? {}
      : { "data-mx-maths": attributes.maths }),
  };
}

export default memo(FormattedTimelineBody);
