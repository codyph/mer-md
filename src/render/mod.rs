//! Native SVG rendering with automatic light and dark colours.

mod cleanup;
mod config;
mod dimensions;
mod graph;
mod palette;
mod theme;

use anyhow::{Context, Result, bail};

/// Cache identity for rendering behaviour.
pub const RENDERER_VERSION: &str = "merman/0.9.3;native-layout/4;auto-palette/4;svg-sizing/1";

/// Render an SVG that follows the browser colour scheme.
pub fn render(source: &str) -> Result<String> {
    if !crate::render::graph::supports(source) {
        bail!("unrecognized or unsupported Mermaid diagram type");
    }
    let light = crate::render::graph::render(source, &crate::render::palette::light_theme())?;
    let dark = crate::render::graph::render(source, &crate::render::palette::dark_theme())?;
    let svg = crate::render::theme::adaptive(&light, &dark)?;
    let svg = crate::render::cleanup::clean(&svg)?;
    let svg = fit_treemap_labels(&svg)?;
    let svg = remove_canvas_and_label_backgrounds(&svg)?;
    let mut svg = crate::render::dimensions::normalize(&svg)?;
    if !svg.ends_with('\n') {
        svg.push('\n');
    }
    Ok(svg)
}

/// Fit substituted fonts within treemap cells without changing data geometry.
fn fit_treemap_labels(svg: &str) -> Result<String> {
    use merman::svg::{DeterministicTextMeasurer, TextMeasurer, TextStyle};
    let document = roxmltree::Document::parse(svg)?;
    if document.root_element().attribute("aria-roledescription") != Some("treemap") {
        return Ok(svg.to_owned());
    }
    let mut edits = Vec::new();
    for label in document.descendants().filter(|node| {
        node.has_tag_name("text")
            && node.attribute("class") == Some("treemapLabel")
            && node.attribute("textLength").is_none()
    }) {
        let Some(id) = label
            .attribute("clip-path")
            .and_then(|value| value.strip_prefix("url(#"))
            .and_then(|value| value.strip_suffix(')'))
        else {
            continue;
        };
        let Some(rect) = document
            .descendants()
            .find(|node| node.attribute("id") == Some(id))
            .and_then(|node| node.children().find(|child| child.has_tag_name("rect")))
        else {
            continue;
        };
        let number = |node: roxmltree::Node<'_, '_>, name, fallback| {
            node.attribute(name)
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(fallback)
        };
        let x = number(label, "x", 0.0);
        let left = number(rect, "x", 0.0);
        let right = left + number(rect, "width", 0.0);
        let available = 2.0 * (x - left).min(right - x) - 4.0;
        if !available.is_finite() || available <= 0.0 {
            continue;
        }
        let font_size = label
            .attribute("style")
            .unwrap_or("")
            .split(';')
            .filter_map(|part| part.split_once(':'))
            .find(|(name, _)| name.trim() == "font-size")
            .and_then(|(_, value)| value.trim().trim_end_matches("px").parse().ok())
            .unwrap_or(16.0);
        let text: String = label
            .descendants()
            .filter(|node| node.is_text())
            .filter_map(|node| node.text())
            .collect();
        let style = TextStyle {
            font_size,
            font_family: Some(crate::render::palette::light_theme().font_family),
            font_weight: None,
            font_style: None,
        };
        let width = DeterministicTextMeasurer::default()
            .measure_svg_text_computed_length_px(&text, &style)
            .min(available);
        if width.is_finite() && width > 0.0 {
            let offset = label.range().start + "<text".len();
            edits.push((
                offset,
                format!(" textLength=\"{width}\" lengthAdjust=\"spacingAndGlyphs\""),
            ));
        }
    }
    let mut result = svg.to_owned();
    for (offset, value) in edits.into_iter().rev() {
        result.insert_str(offset, &value);
    }
    Ok(result)
}

fn remove_canvas_and_label_backgrounds(svg: &str) -> Result<String> {
    let document = roxmltree::Document::parse(svg).context("renderer produced invalid SVG")?;
    let root = document.root_element();
    let mut removals = Vec::new();
    let viewbox = root.attribute("viewBox").and_then(|value| {
        let values = value
            .split_whitespace()
            .map(str::parse::<f64>)
            .collect::<std::result::Result<Vec<_>, _>>()
            .ok()?;
        (values.len() == 4).then_some(values)
    });
    for node in document
        .descendants()
        .filter(|node| node.has_tag_name("rect"))
    {
        let frame_canvas = node.attribute("class").is_some_and(|classes| {
            classes
                .split_ascii_whitespace()
                .any(|name| matches!(name, "frame-border-outer" | "frame-border-inner"))
        });
        let edge_label =
            node.attribute("data-edge-id").is_some() && node.attribute("data-label-kind").is_some();
        let canvas = node.parent_element() == Some(root)
            && viewbox.as_ref().is_some_and(|viewbox| {
                let number = |name, fallback| {
                    node.attribute(name)
                        .and_then(|value| value.parse::<f64>().ok())
                        .unwrap_or(fallback)
                };
                (number("x", 0.0) - viewbox[0]).abs() < 0.01
                    && (number("y", 0.0) - viewbox[1]).abs() < 0.01
                    && (number("width", -1.0) - viewbox[2]).abs() < 0.01
                    && (number("height", -1.0) - viewbox[3]).abs() < 0.01
            });
        if edge_label || canvas || frame_canvas {
            removals.push(node.range());
        }
    }
    let mut result = svg.to_owned();
    for range in removals.into_iter().rev() {
        result.replace_range(range, "");
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn treemap_leaf_text_fits_its_cell_without_changing_data_geometry() {
        let source = include_str!("../../tests/fixtures/diagrams/treemap.mmd");
        let svg = render(source).unwrap();
        let document = roxmltree::Document::parse(&svg).unwrap();
        let mut count = 0;
        for text in document
            .descendants()
            .filter(|node| node.attribute("class") == Some("treemapLabel"))
        {
            let width: f64 = text.attribute("textLength").unwrap().parse().unwrap();
            let id = text
                .attribute("clip-path")
                .unwrap()
                .strip_prefix("url(#")
                .unwrap()
                .strip_suffix(')')
                .unwrap();
            let clip = document
                .descendants()
                .find(|node| node.attribute("id") == Some(id))
                .unwrap();
            let rect = clip
                .children()
                .find(|node| node.has_tag_name("rect"))
                .unwrap();
            let available: f64 = rect.attribute("width").unwrap().parse().unwrap();
            assert!(width > 0.0 && width < available);
            assert_eq!(text.attribute("lengthAdjust"), Some("spacingAndGlyphs"));
            count += 1;
        }
        assert!(count >= 4);
    }

    #[test]
    fn zenuml_canvas_chrome_is_transparent_without_removing_participants() {
        let svg = r##"<svg viewBox="-24 -24 448 348"><rect class="frame-border-outer" width="400" height="300" fill="#777"/><rect class="frame-border-inner" x="1" y="1" width="398" height="298" fill="transparent"/><rect class="participant" x="50" y="50" width="100" height="40" fill="#363052"/></svg>"##;
        let result = remove_canvas_and_label_backgrounds(svg).unwrap();
        assert!(!result.contains("frame-border-"));
        assert!(result.contains("class=\"participant\""));
    }

    #[test]
    fn renders_arrows_with_one_adaptive_geometry() {
        let svg = render("flowchart LR\nA[Start] --> B[Finish]").unwrap();
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("@media(prefers-color-scheme:dark)"));
        assert!(svg.to_ascii_lowercase().contains("#eee9ff"));
        assert!(svg.to_ascii_lowercase().contains("#363052"));
        assert!(!svg.contains("var(--pm-background)"));
        assert!(!svg.contains("var(--pm-edge-label-background)"));
        assert_eq!(svg.matches(">Start</tspan>").count(), 1);
        let document = roxmltree::Document::parse(&svg).unwrap();
        assert!(
            document
                .descendants()
                .any(|node| node.has_tag_name("path") && node.attribute("marker-end").is_some())
        );
        let open_edge = render("flowchart LR\nA[Start] --- B[Finish]").unwrap();
        let open_document = roxmltree::Document::parse(&open_edge).unwrap();
        assert!(
            !open_document
                .descendants()
                .any(|node| node.has_tag_name("path") && node.attribute("marker-end").is_some())
        );
        assert!(!svg.contains("<script"));
        assert!(!svg.contains("<foreignObject"));
        assert!(!svg.contains("--pm-pie-colors-"));
    }

    #[test]
    fn explicit_styles_and_color_looking_labels_are_preserved() {
        let source = "flowchart LR\nA[\"#F8FAFC\"] --> B[Fixed]\nstyle B fill:#ff0000,stroke:#0000ff,color:#ffffff";
        let svg = render(source).unwrap();
        assert!(svg.contains(">#F8FAFC</tspan>"));
        assert!(svg.contains("fill=\"#ff0000\"") || svg.contains("fill:#ff0000"));
        assert!(svg.contains("stroke=\"#0000ff\"") || svg.contains("stroke:#0000ff"));
    }

    #[test]
    fn sequence_notes_and_pie_slices_use_adaptive_colors() {
        let sequence = render("sequenceDiagram\nAlice->>Bob: Hello\nNote over Bob: Note").unwrap();
        assert!(sequence.contains("var(--pm-paint-"));
        assert!(sequence.contains("#FFF0CC"));

        let pie = render("pie\n\"One\": 2\n\"Two\": 3").unwrap();
        assert!(pie.contains("var(--pm-paint-"));
        assert!(pie.contains("#C5B6F1"));
    }

    #[test]
    fn invalid_mermaid_is_an_error() {
        assert!(render("this is not a Mermaid diagram").is_err());
    }
}
