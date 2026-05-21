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

const outboundMarkdownRenderer = new MarkdownIt({
  breaks: false,
  html: false,
  linkify: false,
  typographer: false,
});

const nonFormattingInlineTokenTypes = new Set(["text", "softbreak"]);

export type OutboundMessageContent = {
  body: string;
  formatted_body: string | null;
};

export function outboundMessageContentFromMarkdown(
  markdown: string,
): OutboundMessageContent {
  const body = markdown.trim();
  return {
    body,
    formatted_body: markdownHasFormatting(body)
      ? renderOutboundFormattedBody(body)
      : null,
  };
}

function markdownHasFormatting(markdown: string): boolean {
  const tokens = outboundMarkdownRenderer.parse(markdown, {});
  if (tokens.length !== 3) {
    return tokens.length > 0;
  }

  const [paragraphOpen, inlineToken, paragraphClose] = tokens;
  if (
    paragraphOpen?.type !== "paragraph_open" ||
    inlineToken?.type !== "inline" ||
    paragraphClose?.type !== "paragraph_close"
  ) {
    return true;
  }

  return (inlineToken.children ?? []).some(
    (childToken) => !nonFormattingInlineTokenTypes.has(childToken.type),
  );
}

function renderOutboundFormattedBody(markdown: string): string {
  const tokens = outboundMarkdownRenderer.parse(markdown, {});
  const isSingleParagraph =
    tokens.length === 3 &&
    tokens[0]?.type === "paragraph_open" &&
    tokens[1]?.type === "inline" &&
    tokens[2]?.type === "paragraph_close";

  if (isSingleParagraph) {
    return outboundMarkdownRenderer.renderInline(markdown);
  }

  return outboundMarkdownRenderer.render(markdown);
}
