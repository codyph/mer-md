//! Combine matched native renders without changing labels or source styles.

use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::OnceLock;

struct Edit {
    range: Range<usize>,
    value: String,
}

#[derive(Default)]
struct Palette {
    indices: HashMap<(String, String), usize>,
    values: Vec<(String, String)>,
}

impl Palette {
    fn variable(&mut self, light: &str, dark: &str) -> String {
        let pair = (light.to_owned(), dark.to_owned());
        let next = self.values.len();
        let index = *self.indices.entry(pair.clone()).or_insert_with(|| {
            self.values.push(pair);
            next
        });
        format!("var(--pm-paint-{index})")
    }

    fn stylesheet(&self) -> String {
        let declarations = |dark: bool| {
            let mut output = String::new();
            for (index, (light, dark_value)) in self.values.iter().enumerate() {
                let value = if dark { dark_value } else { light };
                output.push_str(&format!("--pm-paint-{index}:{value};"));
            }
            output
        };
        xml_escape(&format!(
            ":root{{{}}}@media(prefers-color-scheme:dark){{:root{{{}}}}}",
            declarations(false),
            declarations(true)
        ))
    }
}

/// Keep one SVG geometry and switch only paints that differ between themes.
/// Equal values remain literal, including explicit colours from Mermaid source.
pub fn adaptive(light: &str, dark: &str) -> Result<String> {
    let light_xml = roxmltree::Document::parse(light).context("invalid light-theme SVG")?;
    let dark_xml = roxmltree::Document::parse(dark).context("invalid dark-theme SVG")?;
    ensure!(
        light_xml.root_element().tag_name().name() == "svg"
            && dark_xml.root_element().tag_name().name() == "svg",
        "adaptive themes require SVG documents"
    );

    let light_elements: Vec<_> = light_xml.descendants().filter(|n| n.is_element()).collect();
    let dark_elements: Vec<_> = dark_xml.descendants().filter(|n| n.is_element()).collect();
    ensure!(
        light_elements.len() == dark_elements.len(),
        "adaptive themes changed SVG structure: element counts differ"
    );
    let mut palette = Palette::default();
    let mut edits = Vec::new();

    for (light_node, dark_node) in light_elements.iter().zip(&dark_elements) {
        let tag = light_node.tag_name().name();
        ensure!(
            light_node.tag_name() == dark_node.tag_name()
                && light_node.ancestors().count() == dark_node.ancestors().count(),
            "adaptive themes changed SVG structure at <{tag}>"
        );
        ensure!(
            light_node.attributes().len() == dark_node.attributes().len(),
            "adaptive themes changed SVG attributes on <{tag}>"
        );
        for light_attr in light_node.attributes() {
            let name = light_attr.name();
            let dark_attr = dark_node
                .attributes()
                .find(|a| a.name() == name && a.namespace() == light_attr.namespace())
                .with_context(|| format!("adaptive themes changed SVG attribute {tag}.{name}"))?;
            if name == "style" {
                adapt_css(
                    light,
                    dark,
                    light_attr.range_value(),
                    dark_attr.range_value(),
                    &mut palette,
                    &mut edits,
                )
                .with_context(|| format!("adaptive themes differ in {tag}.style"))?;
            } else if is_paint(name) && light_attr.value() != dark_attr.value() {
                edits.push(Edit {
                    range: light_attr.range_value(),
                    value: xml_escape(&palette.variable(light_attr.value(), dark_attr.value())),
                });
            } else {
                ensure!(
                    light_attr.value() == dark_attr.value(),
                    "adaptive themes changed SVG geometry or non-paint attribute {tag}.{name}: {:?} versus {:?}",
                    light_attr.value(),
                    dark_attr.value()
                );
            }
        }

        let light_text: Vec<_> = light_node.children().filter(|n| n.is_text()).collect();
        let dark_text: Vec<_> = dark_node.children().filter(|n| n.is_text()).collect();
        ensure!(
            light_text.len() == dark_text.len(),
            "adaptive themes changed text structure in <{tag}>"
        );
        for (light_text, dark_text) in light_text.iter().zip(&dark_text) {
            if tag == "style" {
                adapt_css(
                    light,
                    dark,
                    light_text.range(),
                    dark_text.range(),
                    &mut palette,
                    &mut edits,
                )
                .context("adaptive themes differ in SVG stylesheet")?;
            } else {
                ensure!(
                    light_text.text() == dark_text.text(),
                    "adaptive themes changed SVG label text in <{tag}>"
                );
            }
        }
    }

    if palette.values.is_empty() {
        return Ok(light.to_owned());
    }
    let root_start = light_xml.root_element().range().start;
    let insertion = opening_tag_end(light, root_start)?;
    ensure!(
        light.as_bytes().get(insertion - 2) != Some(&b'/'),
        "cannot insert an adaptive palette into an empty self-closing SVG"
    );
    edits.push(Edit {
        range: insertion..insertion,
        value: format!("<style>{}</style>", palette.stylesheet()),
    });
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
    let mut output = String::with_capacity(light.len() + palette.values.len() * 80);
    let mut cursor = 0;
    for edit in edits {
        ensure!(
            edit.range.start >= cursor,
            "adaptive SVG paint edits overlap"
        );
        output.push_str(&light[cursor..edit.range.start]);
        output.push_str(&edit.value);
        cursor = edit.range.end;
    }
    output.push_str(&light[cursor..]);
    Ok(output)
}

fn is_paint(name: &str) -> bool {
    matches!(
        name,
        "fill" | "stroke" | "color" | "stop-color" | "flood-color" | "lighting-color"
    )
}

struct Declaration {
    property: String,
    range: Range<usize>,
}

fn declarations(css: &str) -> Vec<Declaration> {
    static PAINT: OnceLock<Regex> = OnceLock::new();
    let paint = PAINT.get_or_init(|| {
        Regex::new(
            r"(?i)(?:^|[;{])\s*(?P<property>fill|stroke|color|background-color|background|border-color|border|stop-color|flood-color|lighting-color)\s*:\s*(?P<value>(?:&(?:#[xX][0-9a-f]+|#[0-9]+|[a-z_][a-z0-9_.:-]*);|[^;{}])*)",
        )
        .expect("valid SVG paint declaration regex")
    });
    paint
        .captures_iter(css)
        .filter_map(|capture| {
            let value = capture.name("value").unwrap();
            let raw = value.as_str();
            let trimmed = raw.trim();
            let core = if trimmed.to_ascii_lowercase().ends_with("!important") {
                trimmed[..trimmed.len() - "!important".len()].trim_end()
            } else {
                trimmed
            };
            let start = value.start() + raw.len() - raw.trim_start().len();
            let property = capture["property"].to_ascii_lowercase();
            let component = match property.as_str() {
                // These are shorthands, so only their colour component may
                // change. Border widths/styles and background positioning
                // remain in the skeleton and still trigger mismatch errors.
                "background" => is_css_colour(core).then_some(0..core.len())?,
                "border" => border_colour(core)?,
                _ => 0..core.len(),
            };
            Some(Declaration {
                property,
                range: start + component.start..start + component.end,
            })
        })
        .collect()
}

fn is_css_colour(value: &str) -> bool {
    value.eq_ignore_ascii_case("transparent")
        || value.eq_ignore_ascii_case("currentcolor")
        || value.parse::<svgtypes::Color>().is_ok()
}

fn border_colour(value: &str) -> Option<Range<usize>> {
    static COLOUR: OnceLock<Regex> = OnceLock::new();
    let colour = COLOUR.get_or_init(|| {
        Regex::new(r"(?i)#[0-9a-f]{3,8}\b|(?:rgba?|hsla?)\([^)]*\)|[a-z]+")
            .expect("valid CSS border colour regex")
    });
    let mut candidates = colour
        .find_iter(value)
        .filter(|token| is_css_colour(token.as_str()));
    let colour = candidates.next()?;
    candidates.next().is_none().then_some(colour.range())
}

fn css_without_paints(css: &str, declarations: &[Declaration]) -> String {
    let mut skeleton = String::new();
    let mut cursor = 0;
    for declaration in declarations {
        skeleton.push_str(&css[cursor..declaration.range.start]);
        skeleton.push_str("<paint>");
        cursor = declaration.range.end;
    }
    skeleton.push_str(&css[cursor..]);
    skeleton
}

fn adapt_css(
    light_svg: &str,
    dark_svg: &str,
    light_range: Range<usize>,
    dark_range: Range<usize>,
    palette: &mut Palette,
    edits: &mut Vec<Edit>,
) -> Result<()> {
    let light = &light_svg[light_range.clone()];
    let dark = &dark_svg[dark_range];
    if light == dark {
        return Ok(());
    }
    let light_declarations = declarations(light);
    let dark_declarations = declarations(dark);
    ensure!(
        light_declarations.len() == dark_declarations.len(),
        "SVG CSS paint declaration counts differ"
    );
    ensure!(
        css_without_paints(light, &light_declarations)
            == css_without_paints(dark, &dark_declarations),
        "SVG CSS geometry or non-paint declarations differ"
    );
    for (a, b) in light_declarations.iter().zip(&dark_declarations) {
        ensure!(a.property == b.property, "SVG CSS paint properties differ");
        let light_value = &light[a.range.clone()];
        let dark_value = &dark[b.range.clone()];
        if light_value != dark_value {
            let light_value = decode_xml_paint(light_value, light)?;
            let dark_value = decode_xml_paint(dark_value, dark)?;
            edits.push(Edit {
                range: light_range.start + a.range.start..light_range.start + a.range.end,
                value: xml_escape(&palette.variable(&light_value, &dark_value)),
            });
        }
    }
    Ok(())
}

fn decode_xml_paint(value: &str, container: &str) -> Result<String> {
    if !value.contains('&') || container.starts_with("<![CDATA[") {
        return Ok(value.to_owned());
    }
    let fragment = format!("<paint>{value}</paint>");
    let document = roxmltree::Document::parse(&fragment).context("invalid XML in CSS paint")?;
    Ok(document
        .root_element()
        .text()
        .unwrap_or_default()
        .to_owned())
}

fn opening_tag_end(svg: &str, start: usize) -> Result<usize> {
    let mut quote = None;
    for (offset, ch) in svg[start..].char_indices() {
        if quote == Some(ch) {
            quote = None;
        } else if quote.is_none() {
            if matches!(ch, '\'' | '"') {
                quote = Some(ch);
            } else if ch == '>' {
                return Ok(start + offset + 1);
            }
        }
    }
    bail!("SVG root opening tag is incomplete")
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapts_only_differing_paints_and_preserves_source_colours_and_labels() {
        let light = r##"<svg viewBox="0 0 100 80"><style>.node{fill:#EEE9FF;stroke:#8B6DD6}.custom{fill:#EEE9FF!important}.label{color:#263347;background-color:transparent}</style><rect x="5" y="10" width="90" height="60" fill="#EEE9FF" stroke="#8B6DD6"/><path d="M0 0L10 10" style="fill:none; stroke: #8B6DD6 !important;stroke-width:2"/><rect fill="#EEE9FF" class="custom"/><text fill="#263347">#EEE9FF &amp; café</text></svg>"##;
        let dark = r##"<svg viewBox="0 0 100 80"><style>.node{fill:#363052;stroke:#AD95EB}.custom{fill:#EEE9FF!important}.label{color:#E7EDF7;background-color:transparent}</style><rect x="5" y="10" width="90" height="60" fill="#363052" stroke="#AD95EB"/><path d="M0 0L10 10" style="fill:none; stroke: #AD95EB !important;stroke-width:2"/><rect fill="#EEE9FF" class="custom"/><text fill="#E7EDF7">#EEE9FF &amp; café</text></svg>"##;
        let result = adaptive(light, dark).unwrap();
        assert!(result.contains("@media(prefers-color-scheme:dark)"));
        assert!(result.contains("--pm-paint-0:#EEE9FF;"));
        assert!(result.contains("--pm-paint-0:#363052;"));
        assert!(result.contains(".custom{fill:#EEE9FF!important}"));
        assert!(result.contains(r##"<rect fill="#EEE9FF" class="custom"/>"##));
        assert!(result.contains("#EEE9FF &amp; café</text>"));
        assert!(result.contains("stroke: var(--pm-paint-1) !important;stroke-width:2"));
        assert!(result.contains("background-color:transparent"));
        assert_eq!(result.matches(r#"d="M0 0L10 10""#).count(), 1);
        assert_eq!(result.matches("--pm-paint-0:").count(), 2);
        roxmltree::Document::parse(&result).unwrap();
    }

    #[test]
    fn keeps_xml_entity_offsets_when_adapting_style_attributes() {
        let light = r##"<svg><g style="font-family:&quot;A&amp;B&quot;;fill:&#35;EEE9FF;stroke:none"><text>hello &amp; #EEE9FF</text></g></svg>"##;
        let dark = r##"<svg><g style="font-family:&quot;A&amp;B&quot;;fill:&#35;363052;stroke:none"><text>hello &amp; #EEE9FF</text></g></svg>"##;
        let result = adaptive(light, dark).unwrap();
        assert!(result.contains("font-family:&quot;A&amp;B&quot;;fill:var(--pm-paint-0)"));
        assert!(result.contains("--pm-paint-0:#EEE9FF;"));
        assert!(result.contains("hello &amp; #EEE9FF"));
    }

    #[test]
    fn rejects_geometry_and_nonpaint_css_changes() {
        let light = r##"<svg viewBox="0 0 10 10"><path d="M0 0L5 5" style="stroke:#111;stroke-width:2"/></svg>"##;
        for dark in [
            light.replace("M0 0L5 5", "M0 0L6 5"),
            light.replace("0 0 10 10", "0 0 20 20"),
            light.replace("stroke-width:2", "stroke-width:3"),
        ] {
            let error = format!("{:#}", adaptive(light, &dark).unwrap_err());
            assert!(error.contains("geometry"), "{error}");
        }
    }

    #[test]
    fn adapts_shorthand_colour_without_adapting_border_geometry() {
        let light = "<svg><style>.tip{background:#DCF4EB;border:1px solid #8E7AA9}</style></svg>";
        let dark = "<svg><style>.tip{background:#183F3B;border:1px solid #7C7493}</style></svg>";
        let result = adaptive(light, dark).unwrap();
        assert!(result.contains("background:var(--pm-paint-0)"));
        assert!(result.contains("border:1px solid var(--pm-paint-1)"));
        let error = format!(
            "{:#}",
            adaptive(light, &dark.replace("1px solid", "2px dashed")).unwrap_err()
        );
        assert!(error.contains("geometry"), "{error}");
    }

    #[test]
    fn adapts_actual_native_flowchart_without_duplicating_geometry() {
        let source = "flowchart LR\nA[Start] --> B[Finish]";
        let light =
            crate::render::graph::render(source, &crate::render::palette::light_theme()).unwrap();
        let dark =
            crate::render::graph::render(source, &crate::render::palette::dark_theme()).unwrap();
        let result = adaptive(&light, &dark).unwrap();
        assert!(result.contains("@media(prefers-color-scheme:dark)"));
        let before = roxmltree::Document::parse(&light).unwrap();
        let after = roxmltree::Document::parse(&result).unwrap();
        let paths = |doc: &roxmltree::Document<'_>| {
            doc.descendants()
                .filter(|node| node.has_tag_name("path"))
                .map(|node| node.attribute("d").unwrap_or_default().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(paths(&before), paths(&after));
        assert!(result.contains("#363052"));
        assert!(result.contains("#EEE9FF"));
    }

    #[test]
    fn rejects_changed_structure_or_labels() {
        let light = "<svg><text>Label</text></svg>";
        assert!(adaptive(light, "<svg><g><text>Label</text></g></svg>").is_err());
        assert!(adaptive(light, "<svg><text>Different</text></svg>").is_err());
        assert_eq!(adaptive(light, light).unwrap(), light);
    }
}
