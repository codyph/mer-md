//! Intrinsic SVG dimensions with responsive scaling.

use anyhow::{Context, Result, bail};
use std::ops::Range;

/// Export one pixel per viewBox unit to prevent external images from upscaling.
pub fn normalize(svg: &str) -> Result<String> {
    let document = roxmltree::Document::parse(svg).context("invalid SVG during sizing")?;
    let root = document.root_element();
    let Some(view_box) = root.attribute("viewBox") else {
        // Mermaid's `info` utility intentionally exports no geometric viewBox.
        return Ok(svg.to_owned());
    };
    let values = view_box
        .split(|character: char| character.is_ascii_whitespace() || character == ',')
        .filter(|part| !part.is_empty())
        .map(str::parse::<f64>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("invalid SVG viewBox during sizing")?;
    if values.len() != 4
        || values.iter().any(|value| !value.is_finite())
        || values[2] <= 0.0
        || values[3] <= 0.0
    {
        bail!("invalid SVG viewBox dimensions during sizing");
    }

    let width = values[2].to_string();
    let height = values[3].to_string();
    let style = responsive_style(root.attribute("style").unwrap_or(""));
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut additions = String::new();
    for (name, value) in [("width", width), ("height", height), ("style", style)] {
        let escaped = escape_attribute(&value);
        if let Some(attribute) = root.attribute_node(name) {
            if attribute.value() != value {
                edits.push((attribute.range_value(), escaped));
            }
        } else {
            additions.push_str(&format!(" {name}=\"{escaped}\""));
        }
    }
    if !additions.is_empty() {
        // Keep the original root name (including a possible namespace prefix).
        let name_end = svg[root.range().start + 1..]
            .find(|character: char| {
                character.is_ascii_whitespace() || matches!(character, '/' | '>')
            })
            .context("missing SVG root element name")?
            + root.range().start
            + 1;
        edits.push((name_end..name_end, additions));
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut output = svg.to_owned();
    for (range, replacement) in edits {
        output.replace_range(range, &replacement);
    }
    Ok(output)
}

fn responsive_style(style: &str) -> String {
    let mut declarations = style
        .split(';')
        .filter(|declaration| !declaration.trim().is_empty())
        .filter(|declaration| {
            declaration.split_once(':').is_none_or(|(name, _)| {
                !matches!(
                    name.trim().to_ascii_lowercase().as_str(),
                    "width" | "height" | "max-width" | "max-height"
                )
            })
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    declarations.push("max-width:100%".to_owned());
    declarations.push("height:auto".to_owned());
    format!("{};", declarations.join(";"))
}

fn escape_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_actual_intrinsic_size_without_touching_layout() {
        let source = r#"<svg width="100%" height="565" viewBox="-12 -24 328.88 130" style="max-width:328.88px;"><g transform="translate(3,4)"><text font-size="16">Label</text></g></svg>"#;
        let output = normalize(source).unwrap();
        let document = roxmltree::Document::parse(&output).unwrap();
        let root = document.root_element();
        assert_eq!(root.attribute("width"), Some("328.88"));
        assert_eq!(root.attribute("height"), Some("130"));
        assert_eq!(root.attribute("viewBox"), Some("-12 -24 328.88 130"));
        assert_eq!(root.attribute("style"), Some("max-width:100%;height:auto;"));
        assert!(
            output.contains(
                "<g transform=\"translate(3,4)\"><text font-size=\"16\">Label</text></g>"
            )
        );
        assert_eq!(normalize(&output).unwrap(), output);
    }

    #[test]
    fn preserves_paints_custom_properties_and_nested_svg_dimensions() {
        let source = r#"<svg viewBox="0,0,2e2,80.5" style="fill:var(--paint);--family:'A &amp; B';WIDTH:100%;height:100%;max-height:400px"><svg width="20" height="10" style="overflow:visible"><text>Child</text></svg></svg>"#;
        let output = normalize(source).unwrap();
        let document = roxmltree::Document::parse(&output).unwrap();
        let root = document.root_element();
        assert_eq!(root.attribute("width"), Some("200"));
        assert_eq!(root.attribute("height"), Some("80.5"));
        assert_eq!(
            root.attribute("style"),
            Some("fill:var(--paint);--family:'A & B';max-width:100%;height:auto;")
        );
        assert!(output.contains("<svg width=\"20\" height=\"10\" style=\"overflow:visible\">"));
    }

    #[test]
    fn keeps_namespace_and_quote_styles_valid() {
        let source = "<!-- cache --><s:svg xmlns:s='http://www.w3.org/2000/svg' width='100%' style='--font:&quot;A&apos;s font&quot;;max-width:90px' viewBox='0 0 90 30'/>";
        let output = normalize(source).unwrap();
        let document = roxmltree::Document::parse(&output).unwrap();
        assert_eq!(document.root_element().attribute("width"), Some("90"));
        assert_eq!(document.root_element().attribute("height"), Some("30"));
        assert_eq!(
            document.root_element().attribute("style"),
            Some("--font:\"A's font\";max-width:100%;height:auto;")
        );
        assert!(output.starts_with("<!-- cache --><s:svg"));
    }

    #[test]
    fn leaves_viewbox_free_info_output_unchanged() {
        let source = "<svg width=\"100%\" style=\"max-width:400px\"><text>Version</text></svg>";
        assert_eq!(normalize(source).unwrap(), source);
    }

    #[test]
    fn rejects_unusable_dimensions() {
        for view_box in [
            "0 0 0 20",
            "0 0 20 -5",
            "0 0 NaN 20",
            "0 0 20",
            "0 0 inf 20",
        ] {
            assert!(normalize(&format!("<svg viewBox=\"{view_box}\"/>")).is_err());
        }
    }
}
