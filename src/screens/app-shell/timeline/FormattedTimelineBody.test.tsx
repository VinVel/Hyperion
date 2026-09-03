import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, test } from "vitest";
import type { BackendTimelineRichTextNode } from "../appShellAdapters";
import FormattedTimelineBody from "./FormattedTimelineBody";

describe("FormattedTimelineBody", () => {
  test("renders prevalidated rich-text nodes without parsing HTML", () => {
    const markup = render({
      type: "element",
      tag: "strong",
      attributes: {},
      children: [{ type: "text", text: "Hello" }],
    });

    expect(markup).toContain("<strong>Hello</strong>");
  });

  test("renders a backend-provided link with the normal anchor semantics", () => {
    const markup = render({
      type: "element",
      tag: "a",
      attributes: { href: "https://matrix.org" },
      children: [{ type: "text", text: "Matrix" }],
    });

    expect(markup).toContain('href="https://matrix.org"');
    expect(markup).toContain(">Matrix</a>");
  });

  test("does not render media for an image node", () => {
    const markup = render({
      type: "element",
      tag: "img",
      attributes: { alt: "Diagram" },
      children: [],
    });

    expect(markup).toContain("Diagram");
    expect(markup).not.toContain("<img");
  });

  test("keeps a compatibility fallback for older shell payloads", () => {
    const markup = renderToStaticMarkup(
      <FormattedTimelineBody body="Plain fallback" richText={null} />,
    );

    expect(markup).toContain("Plain fallback");
  });
});

function render(node: BackendTimelineRichTextNode) {
  return renderToStaticMarkup(
    <FormattedTimelineBody body="fallback" richText={[node]} />,
  );
}
