//! Correct native SVG text export without changing authored paints or layout.

use anyhow::{Context, Result};
use roxmltree::{Document, Node};
use std::ops::Range;

pub fn clean(svg: &str) -> Result<String> {
    let document = Document::parse(svg).context("invalid SVG during text cleanup")?;
    let root = document.root_element();
    let fallback_prefix = format!(
        "{}-foreignobject-label-",
        root.attribute("id").unwrap_or("mer-md")
    );
    let event_modeling = root.attribute("aria-roledescription") == Some("eventmodeling");
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();

    for node in root.descendants().filter(Node::is_element) {
        // Substituted fonts can exceed HTML-label viewport estimates.
        // Allow overflow without changing measured geometry.
        let fallback_viewport = node.has_tag_name("svg")
            && node.parent_element().is_some_and(|parent| {
                parent.has_tag_name("g")
                    && parent
                        .parent_element()
                        .is_some_and(|defs| defs.has_tag_name("defs"))
                    && parent.attribute("id").is_some_and(|id| {
                        id.strip_prefix(&fallback_prefix)
                            .is_some_and(|suffix| suffix.parse::<usize>().is_ok())
                    })
            });
        if fallback_viewport {
            if let Some(attribute) = node.attribute_node("overflow") {
                if attribute.value() != "visible" {
                    edits.push((attribute.range_value(), "visible".to_owned()));
                }
            } else {
                let insertion = node.range().start + "<svg".len();
                edits.push((insertion..insertion, " overflow=\"visible\"".to_owned()));
            }
            // Inline overflow overrides the presentation attribute.
            if let Some(attribute) = node.attribute_node("style") {
                let style = visible_overflow_style(attribute.value());
                if style != attribute.value() {
                    edits.push((attribute.range_value(), escape_attribute(&style)));
                }
            } else {
                let insertion = node.range().start + "<svg".len();
                edits.push((
                    insertion..insertion,
                    " style=\"overflow:visible\"".to_owned(),
                ));
            }
        }

        // EventModeling headings use CSS color; SVG text needs fill.
        // A presentation attribute preserves authored CSS precedence.
        if event_modeling
            && node.has_tag_name("text")
            && node
                .ancestors()
                .any(|parent| has_class(parent, "em-swimlane"))
            && !node.ancestors().any(has_explicit_fill)
        {
            let insertion = node.range().start + "<text".len();
            edits.push((insertion..insertion, " fill=\"currentColor\"".to_owned()));
        }
    }

    edits.sort_by_key(|(range, _)| std::cmp::Reverse((range.start, range.end)));
    let mut output = svg.to_owned();
    for (range, replacement) in edits {
        output.replace_range(range, &replacement);
    }
    Ok(output)
}

fn has_class(node: Node<'_, '_>, class: &str) -> bool {
    node.attribute("class")
        .is_some_and(|classes| classes.split_ascii_whitespace().any(|token| token == class))
}

fn has_explicit_fill(node: Node<'_, '_>) -> bool {
    node.attribute("fill").is_some()
        || node.attribute("style").is_some_and(|style| {
            style
                .split(';')
                .filter_map(|part| part.split_once(':'))
                .any(|(property, _)| property.trim().eq_ignore_ascii_case("fill"))
        })
}

fn visible_overflow_style(style: &str) -> String {
    let mut found = false;
    let mut output = style
        .split(';')
        .map(|declaration| {
            if let Some((property, value)) = declaration.split_once(':')
                && property.trim().eq_ignore_ascii_case("overflow")
            {
                found = true;
                if value.trim().eq_ignore_ascii_case("visible")
                    || value.trim().eq_ignore_ascii_case("visible!important")
                    || value.trim().eq_ignore_ascii_case("visible !important")
                {
                    return declaration.to_owned();
                }
                let importance = if value
                    .trim_end()
                    .to_ascii_lowercase()
                    .ends_with("!important")
                {
                    "!important"
                } else {
                    ""
                };
                return format!("{property}:visible{importance}");
            }
            declaration.to_owned()
        })
        .collect::<Vec<_>>()
        .join(";");
    if !found {
        if !output.is_empty() && !output.ends_with(';') {
            output.push(';');
        }
        output.push_str("overflow:visible");
    }
    output
}

fn escape_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_html_labels_are_not_clipped_to_substitute_font_measurements() {
        for (family, source) in [
            (
                "block",
                include_str!("../../tests/fixtures/diagrams/block.mmd"),
            ),
            (
                "kanban",
                include_str!("../../tests/fixtures/diagrams/kanban.mmd"),
            ),
            (
                "mindmap",
                include_str!("../../tests/fixtures/diagrams/mindmap.mmd"),
            ),
            (
                "usecase",
                include_str!("../../tests/fixtures/diagrams/usecase.mmd"),
            ),
        ] {
            let svg = crate::render::render(source).unwrap();
            let document = Document::parse(&svg).unwrap();
            assert!(
                !document
                    .descendants()
                    .any(|node| node.has_tag_name("foreignObject")),
                "{family} did not exercise native SVG text export"
            );
            let labels: Vec<_> = document
                .descendants()
                .filter(|node| {
                    node.has_tag_name("svg")
                        && node.parent_element().is_some_and(|parent| {
                            parent
                                .attribute("id")
                                .is_some_and(|id| id.starts_with("mer-md-foreignobject-label-"))
                        })
                })
                .collect();
            assert!(
                labels.len() >= 3,
                "{family} must exercise multiple HTML label viewports"
            );
            for label in labels {
                assert_eq!(label.attribute("overflow"), Some("visible"));
                let inline_overflows: Vec<_> = label
                    .attribute("style")
                    .unwrap_or("")
                    .split(';')
                    .filter_map(|part| part.split_once(':'))
                    .filter(|(name, _)| name.trim().eq_ignore_ascii_case("overflow"))
                    .map(|(_, value)| value.trim())
                    .collect();
                assert!(
                    !inline_overflows.is_empty()
                        && inline_overflows
                            .iter()
                            .all(|value| value.starts_with("visible")),
                    "{family} still clips through higher-priority inline SVG CSS: {inline_overflows:?}"
                );
                assert!(
                    label.attribute("viewBox").is_some(),
                    "keep measured label coordinates"
                );
            }
        }
    }

    #[test]
    fn real_event_modeling_lane_headings_use_adaptive_text_paint() {
        let source = include_str!("../../tests/fixtures/diagrams/event-modeling.mmd");
        let svg = crate::render::render(source).unwrap();
        let document = Document::parse(&svg).unwrap();
        let headings: Vec<_> = document
            .descendants()
            .filter(|node| {
                node.has_tag_name("text")
                    && node
                        .ancestors()
                        .any(|parent| has_class(parent, "em-swimlane"))
            })
            .collect();
        assert_eq!(headings.len(), 3);
        for heading in headings {
            assert_eq!(heading.attribute("fill"), Some("currentColor"));
        }
        assert!(svg.contains("prefers-color-scheme:dark"));
        assert!(svg.contains("color: var(--pm-paint-"));
    }

    #[test]
    fn authored_fills_and_other_nested_svg_viewports_are_preserved() {
        let svg = r##"<svg id="mer-md" aria-roledescription="eventmodeling"><style>.em-swimlane text{color:#fff;fill:#abcdef}</style><defs><g id="mer-md-foreignobject-label-0"><svg overflow="hidden" style="opacity:0.75;overflow:hidden!important;filter:none" viewBox="0 0 20 20"><text>Generated</text></svg></g><g id="authored-foreignobject-label-1"><svg overflow="hidden"><text>Authored</text></svg></g></defs><g class="em-swimlane"><text fill="#123456">Explicit attribute</text><text style="fill:#234567">Explicit style</text><g fill="#345678"><text>Inherited authored fill</text></g><text>No explicit fill</text></g></svg>"##;
        let output = clean(svg).unwrap();
        assert!(output.contains(r##"<text fill="#123456">Explicit attribute</text>"##));
        assert!(output.contains(r##"<text style="fill:#234567">Explicit style</text>"##));
        assert!(output.contains(r##"<g fill="#345678"><text>Inherited authored fill</text></g>"##));
        assert!(output.contains(r#"<text fill="currentColor">No explicit fill</text>"#));
        assert!(output.contains(".em-swimlane text{color:#fff;fill:#abcdef}"));
        assert!(output.contains("opacity:0.75;overflow:visible!important;filter:none"));
        assert!(
            output.contains(r#"<g id="authored-foreignobject-label-1"><svg overflow="hidden">"#)
        );
        assert_eq!(
            clean(&output).unwrap(),
            output,
            "cleanup must be idempotent"
        );
    }
}
