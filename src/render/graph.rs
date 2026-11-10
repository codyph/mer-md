//! Native Mermaid geometry with orthogonal ELK routing for graph diagrams.

use crate::render::palette::Theme;
use anyhow::{Context, Result, bail};
use merman::{
    DetectorRegistry, Engine, MermaidConfig, OperationControl, RenderOutput, RenderRequest,
    Renderer, SvgRequest,
};
use roxmltree::{Document, Node};
use serde_json::json;
use std::collections::HashMap;
use std::ops::Range;
use svgtypes::{SimplePathSegment, SimplifyingPathParser, Transform};

/// Detect diagrams using the renderer's frontmatter-aware parser.
pub fn supports(source: &str) -> bool {
    diagram_type(source).is_some()
}

fn diagram_type(source: &str) -> Option<&'static str> {
    let mut config = MermaidConfig::default();
    let kind = DetectorRegistry::pinned_mermaid_baseline()
        .detect_type(source, &mut config)
        .ok()?;
    (!matches!(kind, "error" | "---")).then_some(kind)
}

pub fn render(source: &str, theme: &Theme) -> Result<String> {
    let font_size = format!("{}px", theme.font_size);
    let fields = [
        ("fontFamily", theme.font_family.as_str()),
        ("fontSize", &font_size),
        ("background", "transparent"),
        ("edgeLabelBackground", "transparent"),
        ("labelBackgroundColor", "transparent"),
        ("relationLabelBackground", "transparent"),
        ("primaryColor", theme.primary_color.as_str()),
        ("mainBkg", theme.primary_color.as_str()),
        ("primaryTextColor", theme.primary_text_color.as_str()),
        ("nodeTextColor", theme.primary_text_color.as_str()),
        ("stateLabelColor", theme.primary_text_color.as_str()),
        ("labelColor", theme.text_color.as_str()),
        ("primaryBorderColor", theme.primary_border_color.as_str()),
        ("secondaryColor", theme.secondary_color.as_str()),
        ("secondaryTextColor", theme.text_color.as_str()),
        ("secondaryBorderColor", theme.primary_border_color.as_str()),
        ("tertiaryColor", theme.tertiary_color.as_str()),
        ("tertiaryTextColor", theme.text_color.as_str()),
        ("tertiaryBorderColor", theme.cluster_border.as_str()),
        ("textColor", theme.text_color.as_str()),
        ("titleColor", theme.text_color.as_str()),
        ("lineColor", theme.line_color.as_str()),
        ("arrowheadColor", theme.line_color.as_str()),
        ("defaultLinkColor", theme.line_color.as_str()),
        ("nodeBkg", theme.primary_color.as_str()),
        ("nodeBorder", theme.primary_border_color.as_str()),
        ("clusterBkg", theme.cluster_background.as_str()),
        ("clusterBorder", theme.cluster_border.as_str()),
        ("stateBkg", theme.primary_color.as_str()),
        ("stateBorder", theme.primary_border_color.as_str()),
        ("specialStateColor", theme.line_color.as_str()),
        ("innerEndBackground", theme.primary_color.as_str()),
        ("altBackground", theme.secondary_color.as_str()),
        ("compositeBackground", theme.cluster_background.as_str()),
        (
            "compositeTitleBackground",
            theme.cluster_background.as_str(),
        ),
        ("classText", theme.text_color.as_str()),
        ("attributeBackgroundColorOdd", theme.primary_color.as_str()),
        ("rowOdd", theme.primary_color.as_str()),
        (
            "attributeBackgroundColorEven",
            theme.secondary_color.as_str(),
        ),
        ("rowEven", theme.secondary_color.as_str()),
        ("noteBkgColor", theme.sequence_note_fill.as_str()),
        ("noteBorderColor", theme.sequence_note_border.as_str()),
        ("noteTextColor", theme.text_color.as_str()),
    ];
    let mut variables: serde_json::Map<String, serde_json::Value> = fields
        .into_iter()
        .map(|(key, value)| (key.to_owned(), serde_json::Value::String(value.to_owned())))
        .collect();
    variables.extend(crate::render::palette::native_variables(theme));
    let mut family_options = crate::render::config::family_options(theme);
    if let Some(serde_json::Value::Object(specialized)) = family_options.remove("themeVariables") {
        variables.extend(specialized);
    }
    let family = diagram_type(source);
    if family == Some("venn") {
        // Venn treats transparent as black when calculating label contrast.
        // Supply the theme's page colour; the exported canvas stays transparent.
        variables.insert(
            "background".to_owned(),
            json!(if theme.text_color.eq_ignore_ascii_case("#E7EDF7") {
                "#1F232B"
            } else {
                "#FFFFFF"
            }),
        );
    }
    let mut config_value = json!({
        "theme": "base", "look": "classic", "layout": "elk", "htmlLabels": true,
        "darkMode": theme.text_color.eq_ignore_ascii_case("#E7EDF7"),
        "fontFamily": theme.font_family,
        "flowchart": {
            "htmlLabels": true, "curve": "basis", "wrappingWidth": 240,
            "minNodeWidth": 0, "padding": 15, "nodeSpacing": 60,
            "rankSpacing": 80, "diagramPadding": 12
        },
        "state": {"wrappingWidth": 240, "minNodeWidth": 0},
        "themeVariables": variables
    });
    if family == Some("usecase") {
        // Measure usecase HTML labels before converting them to SVG text.
        config_value["htmlLabels"] = json!(true);
        // The converter requires unquoted font names.
        let family = theme.font_family.replace(['"', '\''], "");
        config_value["fontFamily"] = json!(family);
        config_value["themeVariables"]["fontFamily"] = json!(family);
        config_value["usecase"] = json!({"actorFontFamily": family, "usecaseFontFamily": family});
    }
    config_value.as_object_mut().unwrap().extend(family_options);
    if family == Some("c4") {
        let margin = c4_relationship_margin(source, &config_value)?;
        config_value["c4"]["c4ShapeMargin"] = json!(margin);
    }
    let config = MermaidConfig::from_value(config_value);
    let renderer = Renderer::new().with_engine(Engine::new().with_site_config(config));
    let mut layout = merman::svg::LayoutOptions::headless_svg_defaults();
    if family == Some("c4") {
        // Leave room for C4 actors, boundary headers and incoming relationships.
        layout.container_width = 1100.0;
        layout.screen_available_width = Some(1100.0);
    }
    let request = SvgRequest {
        layout,
        environment: crate::render::fonts::environment()?,
        pipeline: Some(merman::svg::SvgPipeline::resvg_safe()),
        options: merman::svg::SvgRenderOptions {
            diagram_id: Some("mer-md".to_owned()),
            ..Default::default()
        },
        ..Default::default()
    };
    let output = renderer
        .render(RenderRequest::svg(source, OperationControl::new(), request))
        .context("could not lay out Mermaid graph")?;
    let RenderOutput::Svg(Some(svg)) = output else {
        bail!("source did not contain a Mermaid graph")
    };
    let expanded = crate::render::labels::expand(svg.svg())?;
    normalize_svg(&expanded)
}

/// Reserve half a caption's clearance per C4 shape; margins apply on both sides.
fn c4_relationship_margin(source: &str, site: &serde_json::Value) -> Result<f64> {
    use merman::svg::TextStyle;
    let engine = Engine::new().with_site_config(MermaidConfig::from_value(site.clone()));
    let Some(parsed) = engine
        .parse_diagram_sync(source, merman::ParseOptions::default())
        .context("could not measure C4 relationship labels")?
    else {
        return Ok(50.0);
    };
    let config = parsed.meta.effective_config.as_value();
    let style = TextStyle {
        font_family: Some(
            config
                .pointer("/c4/messageFontFamily")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Open Sans, sans-serif")
                .to_owned(),
        ),
        font_size: config
            .pointer("/c4/messageFontSize")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(12.0),
        font_weight: config
            .pointer("/c4/messageFontWeight")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        ..Default::default()
    };
    let mut width: f64 = 0.0;
    if let Some(relations) = parsed
        .model
        .get("rels")
        .and_then(serde_json::Value::as_array)
    {
        for (index, relation) in relations.iter().enumerate() {
            for key in ["label", "techn", "descr"] {
                let Some(text) = relation
                    .get(key)
                    .and_then(|value| value.get("text"))
                    .and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                let text = if key == "label"
                    && parsed
                        .model
                        .get("c4Type")
                        .and_then(serde_json::Value::as_str)
                        == Some("C4Dynamic")
                {
                    format!("{}: {text}", index + 1)
                } else if key == "techn" {
                    format!("[{text}]")
                } else {
                    text.to_owned()
                };
                width = width.max(crate::render::fonts::text_width(&text, &style)?);
            }
        }
    }
    // Leave clearance for caption padding and font substitution in SVG viewers.
    Ok(((width * 1.15 + 90.0) / 2.0).max(50.0))
}

fn has_class(node: Node<'_, '_>, class: &str) -> bool {
    node.attribute("class")
        .is_some_and(|classes| classes.split_ascii_whitespace().any(|item| item == class))
}

/// Adjust generated SVG ranges while preserving authored styles and text.
fn normalize_svg(svg: &str) -> Result<String> {
    let centered = center_c4_relationship_text(svg)?;
    let positioned = position_edge_labels(&centered)?;
    let svg = positioned.as_str();
    let doc = Document::parse(svg).context("graph renderer returned invalid SVG")?;
    let root = doc.root_element();
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut removed = Vec::new();

    for node in root.descendants().filter(|node| node.is_element()) {
        let empty_label = node.tag_name().name() == "g"
            && has_class(node, "edgeLabel")
            && !node.descendants().any(|child| {
                child.is_text() && child.text().is_some_and(|text| !text.trim().is_empty())
            });
        let label_background = node.tag_name().name() == "rect"
            && (has_class(node, "labelBkg")
                || node
                    .ancestors()
                    .skip(1)
                    .any(|parent| has_class(parent, "label") || has_class(parent, "edgeLabel")));
        if empty_label || label_background {
            removed.push(node.range());
        }
    }
    removed.sort_by_key(|range| (range.start, std::cmp::Reverse(range.end)));
    let mut kept_removed: Vec<Range<usize>> = Vec::new();
    for range in removed {
        if !kept_removed
            .last()
            .is_some_and(|parent| parent.start <= range.start && parent.end >= range.end)
        {
            kept_removed.push(range);
        }
    }
    edits.extend(
        kept_removed
            .iter()
            .cloned()
            .map(|range| (range, String::new())),
    );

    edits.extend(label_colour_edits(root));

    // Correct left-aligned rows inside the native renderer's centred state box.
    for node in root
        .descendants()
        .filter(|node| has_class(*node, "statediagram-state"))
    {
        for label in node.children().filter(|child| {
            child.is_element()
                && has_class(*child, "label")
                && child.children().any(|content| content.has_tag_name("text"))
        }) {
            if let Some(transform) = label.attribute_node("transform")
                && let Some(y) = translate_y(transform.value())
            {
                edits.push((transform.range_value(), format!("translate(0, {y})")));
            }
            for row in label
                .descendants()
                .filter(|child| child.is_element() && has_class(*child, "text-outer-tspan"))
            {
                if let Some(x) = row.attribute_node("x") {
                    edits.push((x.range_value(), "0".to_owned()));
                }
                if let Some(anchor) = row.attribute_node("text-anchor") {
                    edits.push((anchor.range_value(), "middle".to_owned()));
                } else {
                    let position = row.range().start + "<tspan".len();
                    edits.push((position..position, " text-anchor=\"middle\"".to_owned()));
                }
            }
        }
    }

    // Allow for font substitution, marker strokes and shadows.
    if let Some(view_box) = root.attribute_node("viewBox") {
        let bounds: Vec<f64> = view_box
            .value()
            .split(|c: char| c.is_ascii_whitespace() || c == ',')
            .filter(|item| !item.is_empty())
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()
            .context("graph renderer returned an invalid viewBox")?;
        if let [x, y, width, height] = bounds.as_slice() {
            // Radar axis captions extend beyond the nominal viewport.
            let margin = if root.attribute("aria-roledescription") == Some("radar") {
                24.0
            } else {
                12.0
            };
            let fitted = has_class(root, "statediagram")
                .then(|| visible_state_bounds(root, &kept_removed))
                .flatten();
            let (x, y, width, height) = fitted.map_or((*x, *y, *width, *height), |bounds| {
                (
                    bounds.min_x,
                    bounds.min_y,
                    bounds.max_x - bounds.min_x,
                    bounds.max_y - bounds.min_y,
                )
            });
            edits.push((
                view_box.range_value(),
                format!(
                    "{} {} {} {}",
                    x - margin,
                    y - margin,
                    width + margin * 2.0,
                    height + margin * 2.0
                ),
            ));
            if let Some(style) = root.attribute_node("style") {
                let mut declarations: Vec<String> = style
                    .value()
                    .split(';')
                    .filter(|part| {
                        !part.trim().is_empty() && !part.trim_start().starts_with("max-width:")
                    })
                    .map(str::to_owned)
                    .collect();
                declarations.push(format!("max-width: {}px", width + margin * 2.0));
                edits.push((
                    style.range_value(),
                    escape_attribute(&format!("{};", declarations.join(";"))),
                ));
            }
        }
    }

    // Clip connector holes behind transparent labels. Add clip definitions after
    // fitting bounds so their outer regions cannot enlarge the viewBox.
    edits.extend(edge_label_gaps(root));

    edits.retain(|(range, _)| {
        !kept_removed.iter().any(|removed| {
            removed.start <= range.start && removed.end >= range.end && removed != range
        })
    });
    edits.sort_by_key(|(range, _)| std::cmp::Reverse((range.start, range.end)));
    let mut result = svg.to_owned();
    for (range, replacement) in edits {
        result.replace_range(range, &replacement);
    }
    Ok(result)
}

struct PositionedLabel {
    transform: Range<usize>,
    translation: (f64, f64),
    parent_inverse: Transform,
    original: (f64, f64),
    center: (f64, f64),
    width: f64,
    height: f64,
    route: Vec<(f64, f64)>,
    self_loop: bool,
}

impl PositionedLabel {
    fn bounds_at(&self, center: (f64, f64)) -> Bounds {
        Bounds {
            min_x: center.0 - self.width / 2.0,
            min_y: center.1 - self.height / 2.0,
            max_x: center.0 + self.width / 2.0,
            max_y: center.1 + self.height / 2.0,
        }
    }
}

/// Move overlapping labels onto clear runs of their own measured SVG paths.
fn position_edge_labels(svg: &str) -> Result<String> {
    let aligned = center_formatted_edge_text(svg)?;
    let svg = aligned.as_str();
    let doc = Document::parse(svg).context("graph renderer returned invalid SVG")?;
    let root = doc.root_element();
    let nodes: Vec<Bounds> = root
        .descendants()
        .filter(|node| has_class(*node, "node") && !has_class(*node, "cluster"))
        .filter_map(|node| {
            node.children()
                .filter_map(shape_global_bounds)
                .reduce(Bounds::union)
        })
        .collect();
    let paths: HashMap<_, _> = root
        .descendants()
        .filter(|node| node.has_tag_name("path"))
        .filter_map(|node| Some((node.attribute("data-id")?, connector_points(node)?)))
        .collect();
    let mut labels = Vec::new();
    for outer in root
        .descendants()
        .filter(|node| has_class(*node, "edgeLabel"))
    {
        let Some(inner) = outer
            .descendants()
            .find(|node| has_class(*node, "label") && node.attribute("data-id").is_some())
        else {
            continue;
        };
        if !inner
            .descendants()
            .any(|node| node.is_text() && node.text().is_some_and(|text| !text.trim().is_empty()))
        {
            continue;
        }
        let Some(route) = paths.get(inner.attribute("data-id").unwrap()) else {
            continue;
        };
        let Some(bounds) = inner
            .descendants()
            .filter(|node| node.has_tag_name("rect"))
            .filter_map(shape_global_bounds)
            .reduce(Bounds::union)
        else {
            continue;
        };
        let Some(transform) = outer.attribute_node("transform") else {
            continue;
        };
        let Ok(translation) = transform.value().parse::<Transform>() else {
            continue;
        };
        if (translation.a - 1.0).abs() > 1e-6
            || (translation.d - 1.0).abs() > 1e-6
            || translation.b.abs() > 1e-6
            || translation.c.abs() > 1e-6
        {
            continue;
        }
        let Some(parent_inverse) = outer.parent_element().and_then(inverse_global_transform) else {
            continue;
        };
        let center = (
            (bounds.min_x + bounds.max_x) / 2.0,
            (bounds.min_y + bounds.max_y) / 2.0,
        );
        let self_loop = route
            .first()
            .zip(route.last())
            .is_some_and(|(&start, &end)| {
                nodes.iter().any(|node| {
                    contains_point(node.padded(8.0), start) && contains_point(node.padded(8.0), end)
                })
            });
        labels.push(PositionedLabel {
            transform: transform.range_value(),
            translation: (translation.e, translation.f),
            parent_inverse,
            original: center,
            center,
            width: bounds.max_x - bounds.min_x,
            height: bounds.max_y - bounds.min_y,
            route: route.clone(),
            self_loop,
        });
    }
    // Place wider labels first, checking all labels at their current positions.
    labels.sort_by(|a, b| b.width.total_cmp(&a.width));
    for _ in 0..3 {
        let mut changed = false;
        for index in 0..labels.len() {
            let label = &labels[index];
            let score = |center| {
                let bounds = label.bounds_at(center).padded(6.0);
                let node_area: f64 = nodes
                    .iter()
                    .map(|node| overlap_area(bounds, node.padded(4.0)))
                    .sum();
                let label_area: f64 = labels
                    .iter()
                    .enumerate()
                    .filter(|(other, _)| *other != index)
                    .map(|(_, other)| {
                        overlap_area(bounds, other.bounds_at(other.center).padded(6.0))
                    })
                    .sum();
                let distance = squared_distance(center, label.original);
                (node_area + label_area) * 1000.0 + distance * 0.002
            };
            let current_score = score(label.center);
            let mut candidates = Vec::new();
            for segment in label.route.windows(2) {
                let (start, end) = (segment[0], segment[1]);
                candidates.push(project_to_segment(label.original, start, end));
                for fraction in [0.25, 0.5, 0.75] {
                    candidates.push((
                        start.0 + (end.0 - start.0) * fraction,
                        start.1 + (end.1 - start.1) * fraction,
                    ));
                }
            }
            candidates.sort_by(|a, b| {
                squared_distance(*a, label.original)
                    .total_cmp(&squared_distance(*b, label.original))
            });
            candidates.truncate(96);
            let Some(best) = candidates
                .into_iter()
                .min_by(|a, b| score(*a).total_cmp(&score(*b)))
            else {
                continue;
            };
            let off_path = !label.self_loop
                && label
                    .route
                    .windows(2)
                    .map(|segment| {
                        squared_distance(
                            label.center,
                            project_to_segment(label.center, segment[0], segment[1]),
                        )
                    })
                    .fold(f64::INFINITY, f64::min)
                    > 0.01;
            // Keep clear self-loop side labels outside the node.
            if (score(best) + 1e-6 < current_score || (off_path && score(best) < 100.0))
                && squared_distance(best, label.center) > 1e-6
            {
                labels[index].center = best;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut edits = Vec::new();
    let mut occupied = Bounds::empty();
    for label in &labels {
        let (dx, dy) = (
            label.center.0 - label.original.0,
            label.center.1 - label.original.1,
        );
        if dx.abs() < 1e-6 && dy.abs() < 1e-6 {
            continue;
        }
        let inverse = label.parent_inverse;
        let (dx, dy) = (
            inverse.a * dx + inverse.c * dy,
            inverse.b * dx + inverse.d * dy,
        );
        edits.push((
            label.transform.clone(),
            format!(
                "translate({}, {})",
                label.translation.0 + dx,
                label.translation.1 + dy
            ),
        ));
        occupied = occupied.union(label.bounds_at(label.center));
    }
    if edits.is_empty() {
        return Ok(svg.to_owned());
    }
    // Retain original bounds and include labels moved onto outside feedback lanes.
    if occupied.min_x.is_finite()
        && let Some(view_box) = root.attribute_node("viewBox")
    {
        let values: Vec<f64> = view_box
            .value()
            .split_whitespace()
            .filter_map(|part| part.parse().ok())
            .collect();
        if let [x, y, width, height] = values.as_slice() {
            let bounds = occupied.union(Bounds {
                min_x: *x,
                min_y: *y,
                max_x: x + width,
                max_y: y + height,
            });
            edits.push((
                view_box.range_value(),
                format!(
                    "{} {} {} {}",
                    bounds.min_x,
                    bounds.min_y,
                    bounds.max_x - bounds.min_x,
                    bounds.max_y - bounds.min_y
                ),
            ));
        }
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut result = svg.to_owned();
    for (range, value) in edits {
        result.replace_range(range, &value);
    }
    Ok(result)
}

/// Undo C4's extra half-width on relationship captions using native font metrics.
/// Relationship callers already supply the midpoint; authored offsets stay intact.
fn center_c4_relationship_text(svg: &str) -> Result<String> {
    use merman::svg::TextStyle;
    let doc = Document::parse(svg).context("graph renderer returned invalid SVG")?;
    if doc.root_element().attribute("aria-roledescription") != Some("c4") {
        return Ok(svg.to_owned());
    }
    let is_connector = |node: Node<'_, '_>| {
        node.has_tag_name("line")
            || (node.has_tag_name("path")
                && (node.attribute("marker-end").is_some()
                    || node.attribute("marker-start").is_some()))
    };
    let measure = |text: Node<'_, '_>, technology: bool| {
        let content: String = text
            .descendants()
            .filter(Node::is_text)
            .filter_map(|node| node.text())
            .collect();
        let content = if technology {
            content
                .strip_prefix('[')
                .and_then(|value| value.strip_suffix(']'))
                .unwrap_or(&content)
        } else {
            &content
        };
        let style = text.attribute("style").unwrap_or_default();
        crate::render::fonts::text_width(
            content,
            &TextStyle {
                font_family: inline_property(style, "font-family").map(str::to_owned),
                font_size: inline_property(style, "font-size")
                    .and_then(|size| size.trim_end_matches("px").parse().ok())
                    .unwrap_or(12.0),
                font_weight: inline_property(style, "font-weight").map(str::to_owned),
                ..Default::default()
            },
        )
    };
    let mut edits = Vec::new();
    for group in doc
        .descendants()
        .filter(|node| node.has_tag_name("g") && node.children().any(is_connector))
    {
        let mut blocks: Vec<Vec<Node<'_, '_>>> = Vec::new();
        for child in group.children().filter(Node::is_element) {
            if is_connector(child) {
                blocks.push(Vec::new());
            } else if child.has_tag_name("text")
                && let Some(block) = blocks.last_mut()
            {
                block.push(child);
            }
        }
        for block in blocks {
            let mut label_width: f64 = 0.0;
            let mut technology_width: f64 = 0.0;
            for text in &block {
                if text.attribute("font-style") == Some("italic") {
                    technology_width = technology_width.max(measure(*text, true)?);
                } else {
                    label_width = label_width.max(measure(*text, false)?);
                }
            }
            technology_width = technology_width.max(label_width);
            for text in block {
                let Some(x) = text.attribute_node("x") else {
                    continue;
                };
                let Ok(value) = x.value().parse::<f64>() else {
                    continue;
                };
                let width = if text.attribute("font-style") == Some("italic") {
                    technology_width
                } else {
                    label_width
                };
                edits.push((x.range_value(), (value - width / 2.0).to_string()));
            }
        }
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut result = svg.to_owned();
    for (range, value) in edits {
        result.replace_range(range, &value);
    }
    Ok(result)
}

/// Centre native state/class row blocks in their measured label boxes.
/// Row baselines override the parent's baseline; retain fonts and line spacing.
fn center_formatted_edge_text(svg: &str) -> Result<String> {
    let doc = Document::parse(svg).context("graph renderer returned invalid SVG")?;
    let mut edits = Vec::new();
    for label in doc
        .descendants()
        .filter(|node| has_class(*node, "edgeLabel"))
    {
        let Some(bounds) = label
            .descendants()
            .filter(|node| node.has_tag_name("rect"))
            .filter_map(shape_global_bounds)
            .reduce(Bounds::union)
        else {
            continue;
        };
        for text in label.descendants().filter(|node| node.has_tag_name("text")) {
            let rows: Vec<_> = text.children().filter(Node::is_element).collect();
            if rows.is_empty()
                || rows
                    .iter()
                    .any(|row| !has_class(*row, "text-outer-tspan") || !row.has_tag_name("tspan"))
            {
                continue;
            }
            // Native rows use em; leave other authored lengths untouched.
            let Some(positions) = rows
                .iter()
                .map(|row| row.attribute("y")?.strip_suffix("em")?.parse::<f64>().ok())
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let Some(inverse) = inverse_global_transform(text) else {
                continue;
            };
            let center = (
                (bounds.min_x + bounds.max_x) / 2.0,
                (bounds.min_y + bounds.max_y) / 2.0,
            );
            let center = (
                inverse.a * center.0 + inverse.c * center.1 + inverse.e,
                inverse.b * center.0 + inverse.d * center.1 + inverse.f,
            );
            let translation = format!("translate({}, {})", center.0, center.1);
            if let Some(transform) = text.attribute_node("transform") {
                edits.push((
                    transform.range_value(),
                    format!("{} {translation}", transform.value()),
                ));
            } else {
                let insert = text.range().start + "<text".len();
                edits.push((insert..insert, format!(" transform=\"{translation}\"")));
            }
            let midpoint = (positions[0] + positions[positions.len() - 1]) / 2.0;
            for (row, position) in rows.into_iter().zip(positions) {
                for (name, value) in [
                    ("x", "0".to_owned()),
                    ("y", format!("{}em", position - midpoint)),
                    ("dy", "0".to_owned()),
                    ("text-anchor", "middle".to_owned()),
                    ("dominant-baseline", "central".to_owned()),
                ] {
                    if let Some(attribute) = row.attribute_node(name) {
                        edits.push((attribute.range_value(), value));
                    } else {
                        let insert = row.range().start + "<tspan".len();
                        edits.push((insert..insert, format!(" {name}=\"{value}\"")));
                    }
                }
            }
        }
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut result = svg.to_owned();
    for (range, replacement) in edits {
        result.replace_range(range, &replacement);
    }
    Ok(result)
}

fn squared_distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)
}

fn project_to_segment(point: (f64, f64), start: (f64, f64), end: (f64, f64)) -> (f64, f64) {
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    let length_squared = dx * dx + dy * dy;
    let fraction = if length_squared < 1e-12 {
        0.0
    } else {
        ((point.0 - start.0) * dx + (point.1 - start.1) * dy) / length_squared
    }
    .clamp(0.0, 1.0);
    (start.0 + dx * fraction, start.1 + dy * fraction)
}

fn overlap_area(a: Bounds, b: Bounds) -> f64 {
    (a.max_x.min(b.max_x) - a.min_x.max(b.min_x)).max(0.0)
        * (a.max_y.min(b.max_y) - a.min_y.max(b.min_y)).max(0.0)
}

fn contains_point(bounds: Bounds, point: (f64, f64)) -> bool {
    point.0 >= bounds.min_x
        && point.0 <= bounds.max_x
        && point.1 >= bounds.min_y
        && point.1 <= bounds.max_y
}

fn shape_global_bounds(node: Node<'_, '_>) -> Option<Bounds> {
    let local = match node.tag_name().name() {
        "rect" => {
            let (x, y, width, height) = rect_measurement(node)?;
            if width <= 0.0 || height <= 0.0 {
                return None;
            }
            Bounds {
                min_x: x,
                min_y: y,
                max_x: x + width,
                max_y: y + height,
            }
        }
        "circle" | "ellipse" => {
            let (x, y) = (number(node, "cx", 0.0)?, number(node, "cy", 0.0)?);
            let (rx, ry) = if node.has_tag_name("circle") {
                let radius = number(node, "r", 0.0)?;
                (radius, radius)
            } else {
                (number(node, "rx", 0.0)?, number(node, "ry", 0.0)?)
            };
            Bounds {
                min_x: x - rx,
                min_y: y - ry,
                max_x: x + rx,
                max_y: y + ry,
            }
        }
        "path" => path_control_bounds(node)?,
        _ => return None,
    };
    let mut global = Bounds::empty();
    for (x, y) in [
        (local.min_x, local.min_y),
        (local.max_x, local.min_y),
        (local.max_x, local.max_y),
        (local.min_x, local.max_y),
    ] {
        let (x, y) = point_to_global(node, x, y)?;
        global.point(x, y);
    }
    Some(global)
}

fn connector_points(path: Node<'_, '_>) -> Option<Vec<(f64, f64)>> {
    let mut points = Vec::new();
    let mut current = (0.0, 0.0);
    for segment in SimplifyingPathParser::from(path.attribute("d")?) {
        match segment.ok()? {
            SimplePathSegment::MoveTo { x, y } | SimplePathSegment::LineTo { x, y } => {
                current = (x, y);
                points.push(point_to_global(path, x, y)?);
            }
            SimplePathSegment::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                for step in 1..=12 {
                    let t = f64::from(step) / 12.0;
                    let s = 1.0 - t;
                    points.push(point_to_global(
                        path,
                        s.powi(3) * current.0
                            + 3.0 * s * s * t * x1
                            + 3.0 * s * t * t * x2
                            + t.powi(3) * x,
                        s.powi(3) * current.1
                            + 3.0 * s * s * t * y1
                            + 3.0 * s * t * t * y2
                            + t.powi(3) * y,
                    )?);
                }
                current = (x, y);
            }
            SimplePathSegment::Quadratic { x1, y1, x, y } => {
                for step in 1..=8 {
                    let t = f64::from(step) / 8.0;
                    let s = 1.0 - t;
                    points.push(point_to_global(
                        path,
                        s * s * current.0 + 2.0 * s * t * x1 + t * t * x,
                        s * s * current.1 + 2.0 * s * t * y1 + t * t * y,
                    )?);
                }
                current = (x, y);
            }
            SimplePathSegment::ClosePath => {}
        }
    }
    (points.len() >= 2).then_some(points)
}

/// Adaptive paint variables distinguish default text from authored HTML colours.
pub fn contrast_automatic_labels(svg: &str) -> Result<String> {
    let document = Document::parse(svg).context("invalid SVG during label colour adjustment")?;
    let mut edits = label_colour_edits(document.root_element());
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut result = svg.to_owned();
    for (range, replacement) in edits {
        result.replace_range(range, &replacement);
    }
    Ok(result)
}

fn label_colour_edits(root: Node<'_, '_>) -> Vec<(Range<usize>, String)> {
    let fixed_node_colours: HashMap<_, _> = root
        .descendants()
        .filter(|node| has_class(*node, "node"))
        .filter_map(|node| {
            node.children()
                .filter(|shape| {
                    shape.is_element()
                        && matches!(
                            shape.tag_name().name(),
                            "rect" | "circle" | "ellipse" | "polygon" | "path"
                        )
                })
                .find_map(|shape| {
                    let style = shape.attribute("style")?;
                    if ["opacity", "fill-opacity"].into_iter().any(|property| {
                        inline_property(style, property)
                            .or_else(|| shape.attribute(property))
                            .is_some_and(|value| {
                                value
                                    .split('!')
                                    .next()
                                    .unwrap_or(value)
                                    .trim()
                                    .parse::<f64>()
                                    .ok()
                                    != Some(1.0)
                            })
                    }) {
                        return None;
                    }
                    let fill = inline_property(style, "fill")?;
                    crate::render::palette::contrasting_text(fill).map(|colour| (node.id(), colour))
                })
        })
        .collect();
    let mut edits = Vec::new();
    for text in root
        .descendants()
        .filter(|node| node.has_tag_name("text") || node.has_tag_name("tspan"))
    {
        let Some(color) = authored_label_color(text)
            .or_else(|| fixed_node_label_colour(text, &fixed_node_colours))
        else {
            continue;
        };
        if let Some(style) = text.attribute_node("style") {
            let old = style.value().trim_end();
            let separator = if old.is_empty() || old.ends_with(';') {
                ""
            } else {
                ";"
            };
            edits.push((
                style.range_value(),
                escape_attribute(&format!("{old}{separator}fill:{color}")),
            ));
        } else {
            let insert = text.range().start + 1 + text.tag_name().name().len();
            edits.push((
                insert..insert,
                format!(" style=\"{}\"", escape_attribute(&format!("fill:{color}"))),
            ));
        }
    }
    edits
}

fn authored_label_color<'a>(node: Node<'a, '_>) -> Option<&'a str> {
    if node
        .ancestors()
        .any(|parent| has_class(parent, "merman-foreignobject-fallback-text"))
    {
        return None;
    }
    if !node.ancestors().any(|parent| has_class(parent, "label")) {
        return None;
    }
    for ancestor in node.ancestors().filter(Node::is_element) {
        let style = ancestor.attribute("style").unwrap_or("");
        // More specific authored fills, including intermediate spans, take precedence.
        if ancestor.attribute("fill").is_some() || inline_property(style, "fill").is_some() {
            return None;
        }
        if let Some(color) = inline_property(style, "color") {
            return Some(color);
        }
        if has_class(ancestor, "label") {
            break;
        }
    }
    None
}

fn fixed_node_label_colour(
    node: Node<'_, '_>,
    colours: &HashMap<roxmltree::NodeId, &'static str>,
) -> Option<&'static str> {
    let mut label = false;
    for ancestor in node.ancestors().filter(Node::is_element) {
        if has_class(ancestor, "merman-foreignobject-fallback-text") {
            let fill = inline_property(ancestor.attribute("style").unwrap_or(""), "fill")?;
            if fill.starts_with("var(--pm-paint-") {
                continue;
            }
            return None;
        }
        let style = ancestor.attribute("style").unwrap_or("");
        if ancestor.attribute("fill").is_some()
            || inline_property(style, "fill").is_some()
            || inline_property(style, "color").is_some()
        {
            return None;
        }
        if has_class(ancestor, "label") {
            label = true;
            break;
        }
    }
    if !label {
        return None;
    }
    let owner = node.ancestors().find(|parent| has_class(*parent, "node"))?;
    colours.get(&owner.id()).copied()
}

fn inline_property<'a>(style: &'a str, property: &str) -> Option<&'a str> {
    let mut selected: Option<&str> = None;
    for declaration in style.split(';') {
        let Some((name, value)) = declaration.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if !name.trim().eq_ignore_ascii_case(property) || value.is_empty() {
            continue;
        }
        if selected.is_some_and(|old| old.to_ascii_lowercase().ends_with("!important"))
            && !value.to_ascii_lowercase().ends_with("!important")
        {
            continue;
        }
        selected = Some(value);
    }
    selected
}

fn edge_label_gaps(root: Node<'_, '_>) -> Vec<(Range<usize>, String)> {
    let mut labels: Vec<Bounds> = Vec::new();
    // Clear architecture captions beneath icons from bottom-port connectors.
    for service in root
        .descendants()
        .filter(|node| has_class(*node, "architecture-service"))
    {
        for text in service.descendants().filter(|node| {
            node.has_tag_name("text")
                && !node
                    .ancestors()
                    .take_while(|parent| *parent != service)
                    .any(|parent| parent.has_tag_name("svg"))
        }) {
            let content: String = text
                .descendants()
                .filter(Node::is_text)
                .filter_map(|node| node.text())
                .collect();
            if content.trim().is_empty() {
                continue;
            }
            let font_size = text
                .ancestors()
                .filter_map(|parent| parent.attribute("style"))
                .find_map(|style| {
                    inline_property(style, "font-size")
                        .and_then(|value| value.trim_end_matches("px").parse::<f64>().ok())
                })
                .unwrap_or(16.0);
            let metrics = merman::svg::TextMeasurer::measure(
                &merman::svg::DeterministicTextMeasurer::default(),
                &content,
                &merman::svg::TextStyle {
                    font_size,
                    ..Default::default()
                },
            );
            let Some((x, y)) = point_to_global(text, number(text, "x", 0.0).unwrap_or(0.0), 0.0)
            else {
                continue;
            };
            let half_width = metrics.width * 0.575;
            let half_height = metrics.height.max(font_size) * 0.625;
            let measured = Bounds {
                min_x: x - half_width,
                min_y: y - half_height,
                max_x: x + half_width,
                max_y: y + half_height,
            }
            .padded(4.0);
            let mut combined = measured;
            while let Some(index) = labels.iter().position(|other| combined.overlaps(*other)) {
                combined = combined.union(labels.remove(index));
            }
            labels.push(combined);
        }
    }
    for label in root.descendants().filter(|node| {
        has_class(*node, "label")
            && node
                .ancestors()
                .skip(1)
                .any(|parent| has_class(parent, "edgeLabel"))
    }) {
        // Removed backgrounds still provide native label measurements.
        if !label
            .descendants()
            .any(|node| node.is_text() && node.text().is_some_and(|text| !text.trim().is_empty()))
        {
            continue;
        }
        let mut measured = Bounds::empty();
        let mut valid = true;
        let mut found = false;
        for rect in label.descendants().filter(|node| node.has_tag_name("rect")) {
            let Some((x, y, width, height)) = rect_measurement(rect) else {
                valid = false;
                break;
            };
            if width <= 0.0 || height <= 0.0 {
                continue;
            }
            for (x, y) in [
                (x, y),
                (x + width, y),
                (x + width, y + height),
                (x, y + height),
            ] {
                if let Some((x, y)) = point_to_global(rect, x, y) {
                    measured.point(x, y);
                } else {
                    valid = false;
                }
            }
            found = true;
        }
        if valid && found {
            // Merge touching boxes so even-odd clip holes cannot cancel out.
            let mut combined = measured.padded(4.0);
            while let Some(index) = labels.iter().position(|other| combined.overlaps(*other)) {
                combined = combined.union(labels.remove(index));
            }
            labels.push(combined);
        }
    }

    if labels.is_empty() {
        return Vec::new();
    }

    let mut edits = Vec::new();
    let mut definitions = String::new();
    for path in root.descendants().filter(|node| {
        node.has_tag_name("path")
            && !node.ancestors().any(|parent| parent.has_tag_name("defs"))
            && (node.attribute("data-edge") == Some("true")
                || node.ancestors().skip(1).any(|parent| {
                    has_class(parent, "edgePaths") || has_class(parent, "architecture-edges")
                }))
    }) {
        if path.attribute("clip-path").is_some()
            || path
                .attribute("style")
                .is_some_and(|style| style.contains("clip-path"))
        {
            continue;
        }
        let Some(path_bounds) = path_control_bounds(path) else {
            continue;
        };
        let Some(inverse) = inverse_global_transform(path) else {
            continue;
        };
        // Retain clearance for markers and authored stroke widths.
        let outer = path_bounds.padded(256.0);
        let mut clip_data = rectangle_path(outer);
        let mut usable = true;
        for label in &labels {
            let corners = [
                (label.min_x, label.min_y),
                (label.max_x, label.min_y),
                (label.max_x, label.max_y),
                (label.min_x, label.max_y),
            ];
            let points: Vec<_> = corners
                .into_iter()
                .map(|(x, y)| apply_transform(inverse, x, y))
                .collect();
            if points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
                usable = false;
                break;
            }
            clip_data.push_str(&format!(
                "M{} {}L{} {}L{} {}L{} {}Z",
                points[0].0,
                points[0].1,
                points[1].0,
                points[1].1,
                points[2].0,
                points[2].1,
                points[3].0,
                points[3].1
            ));
        }
        if !usable {
            continue;
        }
        let clip_id = format!(
            "{}-label-gap-{}",
            root.attribute("id").unwrap_or("mer-md"),
            edits.len()
        );
        definitions.push_str(&format!("<clipPath id=\"{}\" clipPathUnits=\"userSpaceOnUse\"><path d=\"{}\" clip-rule=\"evenodd\" fill-rule=\"evenodd\"/></clipPath>", escape_attribute(&clip_id), clip_data));
        let insert = path.range().start + "<path".len();
        edits.push((
            insert..insert,
            format!(" clip-path=\"url(#{})\"", escape_attribute(&clip_id)),
        ));
    }
    if !definitions.is_empty() {
        // Preserve unsupported root forms rather than guess a closing-tag offset.
        let insert = root.range().end - "</svg>".len();
        edits.push((insert..insert, format!("<defs>{definitions}</defs>")));
    }
    edits
}

fn rectangle_path(bounds: Bounds) -> String {
    format!(
        "M{} {}H{}V{}H{}Z",
        bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y, bounds.min_x
    )
}

fn rect_measurement(node: Node<'_, '_>) -> Option<(f64, f64, f64, f64)> {
    Some((
        number(node, "x", 0.0)?,
        number(node, "y", 0.0)?,
        number(node, "width", 0.0)?,
        number(node, "height", 0.0)?,
    ))
}

fn path_control_bounds(node: Node<'_, '_>) -> Option<Bounds> {
    let mut bounds = Bounds::empty();
    for segment in SimplifyingPathParser::from(node.attribute("d")?) {
        match segment.ok()? {
            SimplePathSegment::MoveTo { x, y } | SimplePathSegment::LineTo { x, y } => {
                bounds.point(x, y)
            }
            SimplePathSegment::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                bounds.point(x1, y1);
                bounds.point(x2, y2);
                bounds.point(x, y);
            }
            SimplePathSegment::Quadratic { x1, y1, x, y } => {
                bounds.point(x1, y1);
                bounds.point(x, y);
            }
            SimplePathSegment::ClosePath => {}
        }
    }
    bounds.min_x.is_finite().then_some(bounds)
}

fn point_to_global(node: Node<'_, '_>, mut x: f64, mut y: f64) -> Option<(f64, f64)> {
    for ancestor in node.ancestors().filter(Node::is_element) {
        if let Some(value) = ancestor.attribute("transform") {
            let transform: Transform = value.parse().ok()?;
            (x, y) = apply_transform(transform, x, y);
        }
    }
    (x.is_finite() && y.is_finite()).then_some((x, y))
}

fn apply_transform(transform: Transform, x: f64, y: f64) -> (f64, f64) {
    (
        transform.a * x + transform.c * y + transform.e,
        transform.b * x + transform.d * y + transform.f,
    )
}

fn inverse_global_transform(node: Node<'_, '_>) -> Option<Transform> {
    let (e, f) = point_to_global(node, 0.0, 0.0)?;
    let (x, y) = point_to_global(node, 1.0, 0.0)?;
    let (u, v) = point_to_global(node, 0.0, 1.0)?;
    let (a, b, c, d) = (x - e, y - f, u - e, v - f);
    let determinant = a * d - b * c;
    if !determinant.is_finite() || determinant.abs() < 1e-12 {
        return None;
    }
    Some(Transform::new(
        d / determinant,
        -b / determinant,
        -c / determinant,
        a / determinant,
        (c * f - d * e) / determinant,
        (b * e - a * f) / determinant,
    ))
}

#[derive(Clone, Copy, Debug)]
struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Bounds {
    fn empty() -> Self {
        Self {
            min_x: f64::INFINITY,
            min_y: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            max_y: f64::NEG_INFINITY,
        }
    }

    fn padded(self, padding: f64) -> Self {
        Self {
            min_x: self.min_x - padding,
            min_y: self.min_y - padding,
            max_x: self.max_x + padding,
            max_y: self.max_y + padding,
        }
    }

    fn overlaps(self, other: Self) -> bool {
        self.min_x <= other.max_x
            && self.max_x >= other.min_x
            && self.min_y <= other.max_y
            && self.max_y >= other.min_y
    }

    fn union(self, other: Self) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }
    fn point(&mut self, x: f64, y: f64) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }
}

/// Fit visible state geometry, excluding invisible self-loop helpers.
/// Bezier control-point hulls safely bound curves without sampling.
fn visible_state_bounds(root: Node<'_, '_>, removed: &[Range<usize>]) -> Option<Bounds> {
    let mut bounds = Bounds {
        min_x: f64::INFINITY,
        min_y: f64::INFINITY,
        max_x: f64::NEG_INFINITY,
        max_y: f64::NEG_INFINITY,
    };
    for node in root.descendants().filter(Node::is_element) {
        if node.ancestors().any(|parent| parent.has_tag_name("defs"))
            || node.ancestors().any(|parent| {
                parent.has_tag_name("g") && removed.iter().any(|range| *range == parent.range())
            })
        {
            continue;
        }
        let mut points = Vec::new();
        match node.tag_name().name() {
            "rect" => {
                let width = number(node, "width", 0.0)?;
                let height = number(node, "height", 0.0)?;
                if width <= 0.0 || height <= 0.0 {
                    continue;
                }
                let x = number(node, "x", 0.0)?;
                let y = number(node, "y", 0.0)?;
                // Removed backgrounds retain measured text bounds; helpers were skipped.
                points.extend([
                    (x, y),
                    (x + width, y),
                    (x, y + height),
                    (x + width, y + height),
                ]);
            }
            "circle" | "ellipse" => {
                let x = number(node, "cx", 0.0)?;
                let y = number(node, "cy", 0.0)?;
                let rx = number(
                    node,
                    if node.has_tag_name("circle") {
                        "r"
                    } else {
                        "rx"
                    },
                    0.0,
                )?;
                let ry = number(
                    node,
                    if node.has_tag_name("circle") {
                        "r"
                    } else {
                        "ry"
                    },
                    0.0,
                )?;
                points.extend([
                    (x - rx, y - ry),
                    (x + rx, y - ry),
                    (x - rx, y + ry),
                    (x + rx, y + ry),
                ]);
            }
            "line" => points.extend([
                (number(node, "x1", 0.0)?, number(node, "y1", 0.0)?),
                (number(node, "x2", 0.0)?, number(node, "y2", 0.0)?),
            ]),
            "path" => {
                for segment in SimplifyingPathParser::from(node.attribute("d").unwrap_or("")) {
                    match segment.ok()? {
                        SimplePathSegment::MoveTo { x, y } | SimplePathSegment::LineTo { x, y } => {
                            points.push((x, y))
                        }
                        SimplePathSegment::CurveTo {
                            x1,
                            y1,
                            x2,
                            y2,
                            x,
                            y,
                        } => points.extend([(x1, y1), (x2, y2), (x, y)]),
                        SimplePathSegment::Quadratic { x1, y1, x, y } => {
                            points.extend([(x1, y1), (x, y)])
                        }
                        SimplePathSegment::ClosePath => {}
                    }
                }
            }
            // Keep original bounds for unboxed titles or unknown renderer elements.
            "text"
                if !node.ancestors().any(|parent| {
                    has_class(parent, "node")
                        || has_class(parent, "edgeLabel")
                        || has_class(parent, "statediagram-cluster")
                }) =>
            {
                return None;
            }
            "use" | "image" | "foreignObject" | "polygon" | "polyline" => return None,
            _ => {}
        }
        for (mut x, mut y) in points {
            for ancestor in node.ancestors().filter(Node::is_element) {
                if let Some(value) = ancestor.attribute("transform") {
                    let transform: Transform = value.parse().ok()?;
                    (x, y) = (
                        transform.a * x + transform.c * y + transform.e,
                        transform.b * x + transform.d * y + transform.f,
                    );
                }
            }
            if !x.is_finite() || !y.is_finite() {
                return None;
            }
            bounds.point(x, y);
        }
    }
    (bounds.min_x.is_finite() && bounds.max_x > bounds.min_x && bounds.max_y > bounds.min_y)
        .then_some(bounds)
}

fn number(node: Node<'_, '_>, name: &str, default: f64) -> Option<f64> {
    node.attribute(name)
        .map_or(Some(default), |value| value.parse().ok())
}

fn translate_y(transform: &str) -> Option<f64> {
    let values: Vec<f64> = transform
        .trim()
        .strip_prefix("translate(")?
        .strip_suffix(')')?
        .split(|c: char| c.is_ascii_whitespace() || c == ',')
        .filter(|part| !part.is_empty())
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()
        .ok()?;
    match values.as_slice() {
        [_, y] => Some(*y),
        [_] => Some(0.0),
        _ => None,
    }
}

fn escape_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

#[cfg(test)]
mod tests;
