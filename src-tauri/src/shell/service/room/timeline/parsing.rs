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

use std::collections::HashMap;

use crate::shell::types::{RoomTimelineRichTextAttributes, RoomTimelineRichTextNode};

const MATRIX_HTML_FORMAT: &str = "org.matrix.custom.html";
const MAX_NESTING: usize = 100;
const SUPPORTED_TAGS: &[&str] = &[
    "a",
    "b",
    "blockquote",
    "br",
    "caption",
    "code",
    "del",
    "details",
    "div",
    "em",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "img",
    "i",
    "li",
    "ol",
    "p",
    "pre",
    "s",
    "span",
    "strong",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "th",
    "thead",
    "tr",
    "u",
    "ul",
];
const VOID_TAGS: &[&str] = &["br", "hr", "img"];
const DISCARDED_TAGS: &[&str] = &["mx-reply", "script", "style"];
const LINK_SCHEMES: &[&str] = &["ftp://", "http://", "https://", "mailto:", "magnet:?"];

/// Converts untrusted Matrix content into a constrained timeline node tree.
pub(in crate::shell) fn project_timeline_rich_text(
    body: &str,
    formatted_body: Option<&str>,
    formatted_body_format: Option<&str>,
) -> Vec<RoomTimelineRichTextNode> {
    if formatted_body_format == Some(MATRIX_HTML_FORMAT)
        && let Some(formatted) = formatted_body.filter(|value| !value.trim().is_empty())
        && let Some(nodes) = parse_matrix_html(formatted)
    {
        return nodes;
    }
    linkify_text(body)
}

fn parse_matrix_html(source: &str) -> Option<Vec<RoomTimelineRichTextNode>> {
    let mut roots = Vec::new();
    let mut stack: Vec<(
        String,
        RoomTimelineRichTextAttributes,
        Vec<RoomTimelineRichTextNode>,
    )> = Vec::new();

    let mut suppressed: Vec<String> = Vec::new();
    let mut index = 0;

    while index < source.len() {
        let remaining = &source[index..];
        let Some(relative_start) = remaining.find('<') else {
            append_text(&mut roots, &mut stack, &suppressed, remaining);
            break;
        };

        let start = index + relative_start;

        append_text(&mut roots, &mut stack, &suppressed, &source[index..start]);

        let end = source[start..].find('>')? + start;

        index = end + 1;

        let (closing, self_closing, name, attributes) = parse_tag(&source[start + 1..end])?;

        if !suppressed.is_empty() {
            if closing && suppressed.last().is_some_and(|tag| tag == &name) {
                suppressed.pop();
            } else if !closing && !self_closing && DISCARDED_TAGS.contains(&name.as_str()) {
                suppressed.push(name);
            }
            continue;
        }

        if DISCARDED_TAGS.contains(&name.as_str()) {
            if self_closing {
                return None;
            }
            if !closing {
                suppressed.push(name);
            }
            continue;
        }

        if !SUPPORTED_TAGS.contains(&name.as_str()) {
            continue;
        }

        if closing {
            let (open_name, open_attributes, children) = stack.pop()?;

            if open_name != name {
                return None;
            }
            append_node(
                &mut roots,
                &mut stack,
                RoomTimelineRichTextNode::Element {
                    tag: name,
                    attributes: open_attributes,
                    children,
                },
            );
        } else {
            let attributes = sanitize_attributes(&name, &attributes);

            if VOID_TAGS.contains(&name.as_str()) {
                append_node(
                    &mut roots,
                    &mut stack,
                    RoomTimelineRichTextNode::Element {
                        tag: name,
                        attributes,
                        children: Vec::new(),
                    },
                );
            } else {
                if self_closing || stack.len() >= MAX_NESTING {
                    return None;
                }
                stack.push((name, attributes, Vec::new()));
            }
        }
    }

    if stack.is_empty() && suppressed.is_empty() {
        Some(roots)
    } else {
        None
    }
}

fn parse_tag(raw: &str) -> Option<(bool, bool, String, HashMap<String, String>)> {
    let raw = raw.trim();
    let closing = raw.starts_with('/');
    let raw = raw.strip_prefix('/').unwrap_or(raw).trim();
    let self_closing = raw.ends_with('/');
    let raw = raw.strip_suffix('/').unwrap_or(raw).trim();
    let name_end = raw.find(char::is_whitespace).unwrap_or(raw.len());
    let name = raw[..name_end].to_ascii_lowercase();

    if name.is_empty()
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
        || (closing && name_end != raw.len())
    {
        return None;
    }

    let mut attributes = HashMap::new();
    let mut rest = raw[name_end..].trim();

    while !rest.is_empty() {
        let equals = rest.find('=')?;
        let key = rest[..equals].trim().to_ascii_lowercase();
        let quote = rest[equals + 1..].trim_start().chars().next()?;

        if !matches!(quote, '\'' | '"') {
            return None;
        }

        let value_end = rest[1..].find(quote)? + 1;

        attributes.insert(key, decode_entities(&rest[1..value_end]));
        rest = rest[value_end + 1..].trim_start();
    }
    Some((closing, self_closing, name, attributes))
}

fn sanitize_attributes(
    tag: &str,
    values: &HashMap<String, String>,
) -> RoomTimelineRichTextAttributes {
    let mut attributes = RoomTimelineRichTextAttributes::default();

    if tag == "a" {
        attributes.href = values
            .get("href")
            .filter(|value| permitted_link(value))
            .cloned();
        attributes.target = values.get("target").cloned();
    }

    if tag == "img" && values.get("src").is_some_and(|value| permitted_mxc(value)) {
        attributes.alt = values.get("alt").cloned();
        attributes.title = values.get("title").cloned();
    }

    if tag == "code" {
        attributes.language = values.get("class").and_then(|value| {
            value
                .split_whitespace()
                .find(|class| class.starts_with("language-"))
                .map(ToOwned::to_owned)
        });
    }

    if tag == "ol" {
        attributes.start = values.get("start").and_then(|value| value.parse().ok());
    }

    if tag == "span" {
        attributes.color = values
            .get("data-mx-color")
            .filter(|value| matrix_color(value))
            .cloned();
        attributes.background_color = values
            .get("data-mx-bg-color")
            .filter(|value| matrix_color(value))
            .cloned();
        attributes.spoiler = values.get("data-mx-spoiler").cloned();
        attributes.maths = values.get("data-mx-maths").cloned();
    }

    if tag == "div" {
        attributes.maths = values.get("data-mx-maths").cloned();
    }
    attributes
}

fn append_text(
    roots: &mut Vec<RoomTimelineRichTextNode>,
    stack: &mut [(
        String,
        RoomTimelineRichTextAttributes,
        Vec<RoomTimelineRichTextNode>,
    )],
    suppressed: &[String],
    value: &str,
) {
    if suppressed.is_empty() {
        if stack.iter().any(|(tag, _, _)| tag == "a") {
            append_node(
                roots,
                stack,
                RoomTimelineRichTextNode::Text {
                    text: decode_entities(value),
                },
            );
            return;
        }
        for node in linkify_text(&decode_entities(value)) {
            append_node(roots, stack, node);
        }
    }
}

fn append_node(
    roots: &mut Vec<RoomTimelineRichTextNode>,
    stack: &mut [(
        String,
        RoomTimelineRichTextAttributes,
        Vec<RoomTimelineRichTextNode>,
    )],
    node: RoomTimelineRichTextNode,
) {
    if let Some((_, _, children)) = stack.last_mut() {
        children.push(node);
    } else {
        roots.push(node);
    }
}

fn linkify_text(value: &str) -> Vec<RoomTimelineRichTextNode> {
    let mut nodes = Vec::new();
    let mut cursor = 0;

    while let Some(start) = next_link(value, cursor) {
        if start > cursor {
            nodes.push(RoomTimelineRichTextNode::Text {
                text: value[cursor..start].to_owned(),
            });
        }

        let end = value[start..]
            .char_indices()
            .find_map(|(offset, character)| {
                (offset > 0 && (character.is_whitespace() || "<>()\"'".contains(character)))
                    .then_some(start + offset)
            })
            .unwrap_or(value.len());
        let link =
            value[start..end].trim_end_matches(|character: char| ".,!?;:".contains(character));

        if link.is_empty() {
            break;
        }

        nodes.push(RoomTimelineRichTextNode::Element {
            tag: "a".to_owned(),
            attributes: RoomTimelineRichTextAttributes {
                href: Some(link.to_owned()),
                ..Default::default()
            },
            children: vec![RoomTimelineRichTextNode::Text {
                text: link.to_owned(),
            }],
        });

        cursor = start + link.len();
    }

    if cursor < value.len() {
        nodes.push(RoomTimelineRichTextNode::Text {
            text: value[cursor..].to_owned(),
        });
    }

    nodes
}

fn next_link(value: &str, cursor: usize) -> Option<usize> {
    LINK_SCHEMES
        .iter()
        .filter_map(|scheme| value[cursor..].find(scheme).map(|offset| cursor + offset))
        .min()
}

fn permitted_link(value: &str) -> bool {
    LINK_SCHEMES.iter().any(|scheme| value.starts_with(scheme))
}

fn permitted_mxc(value: &str) -> bool {
    value
        .strip_prefix("mxc://")
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(server, media)| {
            !server.is_empty() && !media.is_empty() && !value.contains(['?', '#'])
        })
}

fn matrix_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

fn decode_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

#[cfg(test)]
mod tests {
    use super::project_timeline_rich_text;

    #[test]
    fn projects_matrix_html_into_safe_typed_nodes() {
        assert_eq!(
            project_timeline_rich_text(
                "fallback",
                Some("<strong>Hello</strong>"),
                Some("org.matrix.custom.html")
            )
            .len(),
            1
        );
    }

    #[test]
    fn rejects_unsafe_formatted_html_in_favour_of_plain_text() {
        assert!(
            project_timeline_rich_text(
                "Plain fallback",
                Some("<script>window.stolen = true</script>"),
                Some("org.matrix.custom.html")
            )
            .is_empty()
        );
    }

    #[test]
    fn linkifies_all_permitted_plain_text_schemes() {
        assert_eq!(project_timeline_rich_text("https://matrix.org http://matrix.org ftp://matrix.org mailto:hello@matrix.org magnet:?xt=urn:btih:example", None, None).len(), 9);
    }
}
