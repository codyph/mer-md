use super::*;

fn visible_texts<'a, 'input>(node: Node<'a, 'input>) -> Vec<Node<'a, 'input>> {
    node.descendants()
        .filter(|node| {
            node.has_tag_name("text")
                && !node.ancestors().any(|parent| parent.has_tag_name("defs"))
                && !text_content(*node).trim().is_empty()
        })
        .collect()
}

fn text_content(node: Node<'_, '_>) -> String {
    node.descendants()
        .filter(Node::is_text)
        .filter_map(|node| node.text())
        .collect()
}

fn text_center(node: Node<'_, '_>) -> (f64, f64) {
    point_to_global(
        node,
        number(node, "x", 0.0).unwrap(),
        number(node, "y", 0.0).unwrap(),
    )
    .unwrap()
}

fn block_center(rows: &[Node<'_, '_>]) -> (f64, f64) {
    let mut bounds = Bounds::empty();
    for row in rows {
        let (x, y) = text_center(*row);
        bounds.point(x, y);
    }
    (
        (bounds.min_x + bounds.max_x) / 2.0,
        (bounds.min_y + bounds.max_y) / 2.0,
    )
}

fn text_fill<'a, 'input>(node: Node<'a, 'input>) -> Option<&'a str> {
    inline_property(node.attribute("style").unwrap_or(""), "fill")
        .or_else(|| node.attribute("fill"))
        .map(|value| value.strip_suffix("!important").unwrap_or(value).trim())
}

#[test]
fn detects_diagram_families_through_frontmatter_and_comments() {
    assert!(supports(
        "---\ntitle: Example\n---\n%% comment\nflowchart LR\nA --> B"
    ));
    assert!(supports(
        "%%{init: {'theme':'dark'}}%%\nstateDiagram-v2\n[*] --> A"
    ));
    assert!(supports("classDiagram\nA --> B"));
    assert!(supports("erDiagram\nA ||--o{ B : has"));
    assert!(supports("sequenceDiagram\nA->>B: Hello"));
    assert!(!supports("this is not Mermaid"));
    assert!(!supports("error"));
}

#[test]
fn state_uses_centered_native_text_without_label_backgrounds() {
    let svg = render("stateDiagram-v2\n[*] --> Editable\nEditable --> Rendered: render\nRendered --> Rendered: again", &crate::render::palette::light_theme()).unwrap();
    let doc = Document::parse(&svg).unwrap();
    assert!(
        !doc.descendants()
            .any(|node| node.has_tag_name("foreignObject"))
    );
    assert!(!doc.descendants().any(|node| {
        node.has_tag_name("rect")
            && node
                .ancestors()
                .skip(1)
                .any(|parent| has_class(parent, "label") || has_class(parent, "edgeLabel"))
    }));
    let state = doc
        .descendants()
        .find(|node| has_class(*node, "statediagram-state"))
        .unwrap();
    let label = state
        .children()
        .find(|node| has_class(*node, "label"))
        .unwrap();
    let shape = state
        .children()
        .find(|child| child.has_tag_name("rect"))
        .unwrap();
    let bounds = shape_global_bounds(shape).unwrap();
    let rows = visible_texts(label);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].attribute("text-anchor"), Some("middle"));
    let center = text_center(rows[0]);
    assert!((center.0 - (bounds.min_x + bounds.max_x) / 2.0).abs() < 0.1);
    assert!((center.1 - (bounds.min_y + bounds.max_y) / 2.0).abs() < 0.1);
}

#[test]
fn long_node_labels_wrap_without_shrinking_or_clipping_the_text() {
    let content = "Receive customer requests and validate all required project documents before scheduling the approved work for delivery";
    let source = format!("flowchart LR\nA[\"{content}\"] --> B[Complete]");
    let svg = render(&source, &crate::render::palette::light_theme()).unwrap();
    let doc = Document::parse(&svg).unwrap();
    let node = doc
        .descendants()
        .find(|node| {
            has_class(*node, "node") && node.attribute("id").is_some_and(|id| id.contains("-A-"))
        })
        .unwrap();
    let body = node
        .children()
        .filter_map(shape_global_bounds)
        .reduce(Bounds::union)
        .unwrap();
    let rows = visible_texts(node);
    assert!(rows.len() >= 2, "long labels must wrap at normal text size");
    assert_eq!(
        rows.iter()
            .map(|row| text_content(*row))
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        content
    );
    assert!(
        body.max_x - body.min_x > 240.0,
        "long labels need a suitably wide node"
    );
    for row in rows {
        let style = row.attribute("style").unwrap();
        let font_size: f64 = inline_property(style, "font-size")
            .unwrap()
            .trim_end_matches("px")
            .parse()
            .unwrap();
        assert!(
            font_size >= 16.0,
            "labels must fit without reducing the font size"
        );
        let width = crate::render::fonts::text_width(
            &text_content(row),
            &merman::svg::TextStyle {
                font_family: inline_property(style, "font-family").map(str::to_owned),
                font_size,
                font_weight: inline_property(style, "font-weight").map(str::to_owned),
                ..Default::default()
            },
        )
        .unwrap();
        let (x, y) = text_center(row);
        assert!(
            x - width / 2.0 >= body.min_x + 10.0 && x + width / 2.0 <= body.max_x - 10.0,
            "measured text must fit with padding: {:?}; width {width}; body {body:?}",
            text_content(row)
        );
        assert!(y - font_size / 2.0 > body.min_y && y + font_size / 2.0 < body.max_y);
    }
}

#[test]
fn titled_state_keeps_native_title_and_multiline_description_alignment() {
    let source = "stateDiagram-v2\nstate \"A much longer state title\" as Ready\nReady: x\nReady: y\n[*] --> Ready";
    let svg = render(source, &crate::render::palette::light_theme()).unwrap();
    let doc = Document::parse(&svg).unwrap();
    let titled_state = doc
        .descendants()
        .find(|node| {
            has_class(*node, "statediagram-state")
                && node.children().any(|child| {
                    child
                        .descendants()
                        .any(|item| has_class(item, "title-state"))
                })
        })
        .unwrap();
    let label = titled_state
        .children()
        .find(|child| has_class(*child, "label"))
        .unwrap();
    let rows = visible_texts(label);
    assert_eq!(
        rows.iter()
            .map(|row| text_content(*row))
            .collect::<Vec<_>>(),
        ["A much longer state title", "x", "y"]
    );
    let bounds = titled_state
        .descendants()
        .find(|node| node.has_tag_name("rect") && has_class(*node, "outer"))
        .and_then(shape_global_bounds)
        .unwrap();
    let centers: Vec<_> = rows.iter().map(|row| text_center(*row)).collect();
    let middle_x = (bounds.min_x + bounds.max_x) / 2.0;
    for (x, y) in &centers {
        assert!(
            (x - middle_x).abs() < 0.1,
            "state title and descriptions must share a center"
        );
        assert!(
            *y > bounds.min_y && *y < bounds.max_y,
            "state text must fit inside its body"
        );
    }
    assert!(centers.windows(2).all(|pair| pair[0].1 < pair[1].1));
}

#[test]
fn explicit_node_paint_survives_svg_normalization() {
    let svg = render(
        "flowchart LR\nA[Custom] --> B[Default]\nstyle A fill:#ff0000,stroke:#0000ff",
        &crate::render::palette::light_theme(),
    )
    .unwrap();
    assert!(svg.contains("fill:#ff0000"));
    assert!(svg.contains("stroke:#0000ff"));
}

#[test]
fn fixed_fill_labels_are_readable_while_default_and_edge_labels_follow_the_theme() {
    let source = "flowchart LR\nA[Light] -->|Transition| B[Dark]\nB --> C[Default]\nstyle A fill:#ffffff,opacity:1\nclassDef dark fill:#000000\nclass B dark";
    let svg = crate::render::render(source).unwrap();
    let document = Document::parse(&svg).unwrap();
    for (label, colour) in [("Light", "#263347"), ("Dark", "#E7EDF7")] {
        let text = visible_texts(document.root_element())
            .into_iter()
            .find(|node| text_content(*node) == label)
            .unwrap();
        assert_eq!(
            text_fill(text),
            Some(colour),
            "{label} must contrast with its fixed fill in both themes"
        );
        assert!(
            text.descendants()
                .filter(|node| node.has_tag_name("tspan"))
                .all(|node| text_fill(node).is_none_or(|paint| paint == colour))
        );
    }
    let styles = document
        .descendants()
        .filter(|node| node.has_tag_name("style"))
        .filter_map(|node| node.text())
        .collect::<String>();
    for label in ["Default", "Transition"] {
        let text = visible_texts(document.root_element())
            .into_iter()
            .find(|node| text_content(*node) == label)
            .unwrap();
        let variable = text_fill(text)
            .unwrap()
            .strip_prefix("var(")
            .and_then(|value| value.strip_suffix(')'))
            .expect("theme-dependent text paint");
        for colour in ["#263347", "#E7EDF7"] {
            assert!(
                styles.contains(&format!("{variable}:{colour};")),
                "{label} must follow the theme's text paint {colour}"
            );
        }
    }
    assert!(svg.contains("fill:#ffffff"));
    assert!(svg.contains("fill:#000000"));
}

#[test]
fn fixed_state_fill_gets_readable_text_without_overriding_explicit_colour() {
    let source = "stateDiagram-v2\n[*] --> Light\nLight --> Custom\nclassDef light fill:#ffffff\nclassDef custom fill:#ffffff,color:#ff0000\nclass Light light\nclass Custom custom";
    let svg = crate::render::render(source).unwrap();
    let document = Document::parse(&svg).unwrap();
    for (label, colour) in [("Light", "#263347"), ("Custom", "#ff0000")] {
        let text = visible_texts(document.root_element())
            .into_iter()
            .find(|node| text_content(*node) == label)
            .unwrap();
        assert_eq!(text_fill(text), Some(colour));
    }
}

#[test]
fn authored_label_colors_are_visible_svg_text_fills() {
    let source = "flowchart LR\nA[Custom success] --> B[Custom warning]\nclassDef success fill:#d1fae5,stroke:#047857,color:#064e3b\nclass A success\nstyle B fill:#fef3c7,stroke:#b45309,color:#78350f";
    let svg = render(source, &crate::render::palette::dark_theme()).unwrap();
    let doc = Document::parse(&svg).unwrap();
    for (word, color) in [("success", "#064e3b"), ("warning", "#78350f")] {
        let text = visible_texts(doc.root_element())
            .into_iter()
            .find(|node| text_content(*node).contains(word))
            .unwrap();
        assert_eq!(text_fill(text), Some(color));
        assert!(
            text.descendants()
                .filter(|node| node.has_tag_name("tspan"))
                .all(|node| text_fill(node).is_none_or(|paint| paint == color))
        );
    }
}

#[test]
fn inline_label_colours_take_precedence_over_node_colours_and_contrast_defaults() {
    let source = "flowchart LR\nA[\"<span style='color:#ff0000'>Explicit red</span>\"] --> B[\"<span style='color:#00ff00'>Explicit green</span>\"] --> C[\"<span style='color:#263347'>Explicit default</span>\"]\nstyle A fill:#ffffff\nstyle B fill:#000000,color:#0000ff\nstyle C fill:#000000";
    for (stage, svg) in [
        (
            "light",
            render(source, &crate::render::palette::light_theme()).unwrap(),
        ),
        (
            "dark",
            render(source, &crate::render::palette::dark_theme()).unwrap(),
        ),
        ("adaptive", crate::render::render(source).unwrap()),
    ] {
        let document = Document::parse(&svg).unwrap();
        for (label, colour) in [
            ("Explicit red", "#ff0000"),
            ("Explicit green", "#00ff00"),
            ("Explicit default", "#263347"),
        ] {
            let text = visible_texts(document.root_element())
                .into_iter()
                .find(|node| text_content(*node) == label)
                .unwrap();
            assert_eq!(
                text_fill(text),
                Some(colour),
                "{label} must retain its authored inline colour in {stage} output"
            );
        }
    }
}

#[test]
fn authored_fill_is_preserved_over_inherited_label_color() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><g class="node"><rect style="fill:#ffffff"/><g class="label" style="color:#ff0000!important"><text style="fill:#0000ff!important"><tspan>Blue text</tspan></text></g></g></svg>"##;
    let svg = normalize_svg(source).unwrap();
    let doc = Document::parse(&svg).unwrap();
    let text = doc
        .descendants()
        .find(|node| node.has_tag_name("text"))
        .unwrap();
    assert_eq!(
        inline_property(text.attribute("style").unwrap(), "fill"),
        Some("#0000ff!important")
    );
    assert!(
        doc.descendants()
            .find(|node| node.has_tag_name("tspan"))
            .unwrap()
            .attribute("style")
            .is_none()
    );
}

#[test]
fn dark_er_attribute_rows_use_the_supplied_palette() {
    let theme = crate::render::palette::dark_theme();
    let svg = render(
        "erDiagram\nDOCUMENT {\n string path\n string title\n}",
        &theme,
    )
    .unwrap();
    let doc = Document::parse(&svg).unwrap();
    let fills: Vec<_> = doc
        .descendants()
        .filter(|node| {
            node.has_tag_name("path") && !node.ancestors().any(|parent| parent.has_tag_name("defs"))
        })
        .filter_map(|node| node.attribute("fill"))
        .collect();
    assert!(fills.contains(&theme.primary_color.as_str()));
    assert!(fills.contains(&theme.secondary_color.as_str()));
    assert!(!fills.iter().any(|fill| matches!(
        fill.to_ascii_lowercase().as_str(),
        "white" | "#fff" | "#ffffff"
    )));
}

#[test]
fn state_bounds_exclude_helpers_and_defs_but_include_curve_hulls() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" class="statediagram" viewBox="0 0 1000 1000" style="max-width:1000px"><defs><path d="M900 900L999 999"/></defs><g transform="translate(10,20)"><rect x="20" y="30" width="100" height="40"/><path d="M120 50C150 180 200 180 250 50"/></g><g class="edgeLabel" transform="translate(900,900)"><rect width="0.1" height="0.1"/></g></svg>"#;
    let normalized = normalize_svg(svg).unwrap();
    let doc = Document::parse(&normalized).unwrap();
    assert_eq!(
        doc.root_element().attribute("viewBox"),
        Some("18 38 254 174")
    );
    assert!(!normalized.contains("translate(900,900)"));
    assert!(normalized.contains("max-width: 254px"));
}

#[test]
fn state_bounds_retain_measured_text_after_background_removal() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" class="statediagram" viewBox="0 0 1000 1000"><g class="edgeLabel" transform="translate(50,60)"><g class="label" transform="translate(-40,-10)"><rect class="background" width="80" height="20"/><text>Long label</text></g></g></svg>"#;
    let normalized = normalize_svg(svg).unwrap();
    let doc = Document::parse(&normalized).unwrap();
    assert_eq!(
        doc.root_element().attribute("viewBox"),
        Some("-2 38 104 44")
    );
    assert!(!normalized.contains("<rect"));
}

#[test]
fn edge_labels_cut_transparent_holes_in_all_connectors() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" id="test" class="statediagram" viewBox="0 0 200 200"><g class="edgePaths" transform="translate(10,20)"><path data-id="edge1" d="M50 0V150" marker-end="url(#arrow)"/><path data-id="edge2" d="M100 0V150"/></g><g class="node"><path data-id="node-shape" d="M0 0H10V10Z"/></g><g class="edgeLabel" transform="translate(60,80)"><g class="label" data-id="edge1" transform="translate(-20,-10)"><rect class="background" width="40" height="20"/><text>Label</text></g></g></svg>"#;
    let normalized = normalize_svg(svg).unwrap();
    let doc = Document::parse(&normalized).unwrap();
    let edge = doc
        .descendants()
        .find(|node| node.attribute("data-id") == Some("edge1") && node.has_tag_name("path"))
        .unwrap();
    assert_eq!(edge.attribute("clip-path"), Some("url(#test-label-gap-0)"));
    assert_eq!(edge.attribute("marker-end"), Some("url(#arrow)"));
    let other = doc
        .descendants()
        .find(|node| node.attribute("data-id") == Some("edge2"))
        .unwrap();
    assert_eq!(other.attribute("clip-path"), Some("url(#test-label-gap-1)"));
    let shape = doc
        .descendants()
        .find(|node| node.attribute("data-id") == Some("node-shape"))
        .unwrap();
    assert!(shape.attribute("clip-path").is_none());
    let clip = doc
        .descendants()
        .find(|node| node.has_tag_name("clipPath"))
        .unwrap();
    assert_eq!(clip.attribute("clipPathUnits"), Some("userSpaceOnUse"));
    let hole = clip.children().find(Node::is_element).unwrap();
    assert_eq!(hole.attribute("clip-rule"), Some("evenodd"));
    // Label's global padded box(36,66)-(84,94) becomes(26,46)-(74,74)
    // in the independently translated edge path's coordinates.
    assert!(
        hole.attribute("d")
            .unwrap()
            .ends_with("M26 46L74 46L74 74L26 74Z")
    );
    assert!(!normalized.contains("<rect"));
    assert!(!normalized.contains("<mask"));
}

#[test]
fn overlapping_edge_labels_move_along_their_own_routes() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 240 220"><g class="edgePaths"><path data-id="left" d="M80 20V200"/><path data-id="right" d="M140 20V200"/></g><g class="edgeLabels"><g class="edgeLabel" transform="translate(80,110)"><g class="label" data-id="left" transform="translate(-55,-9)"><rect width="110" height="18"/><text>First long label</text></g></g><g class="edgeLabel" transform="translate(140,110)"><g class="label" data-id="right" transform="translate(-55,-9)"><rect width="110" height="18"/><text>Second long label</text></g></g></g></svg>"#;
    let positioned = position_edge_labels(svg).unwrap();
    let doc = Document::parse(&positioned).unwrap();
    let labels: Vec<_> = doc
        .descendants()
        .filter(|node| node.has_tag_name("rect"))
        .filter_map(shape_global_bounds)
        .collect();
    assert_eq!(labels.len(), 2);
    assert_eq!(
        overlap_area(labels[0].padded(4.0), labels[1].padded(4.0)),
        0.0,
        "edge labels must have separate readable boxes"
    );
    assert_eq!((labels[0].min_x + labels[0].max_x) / 2.0, 80.0);
    assert_eq!((labels[1].min_x + labels[1].max_x) / 2.0, 140.0);
    assert!(positioned.contains("First long label") && positioned.contains("Second long label"));
}

#[test]
fn clear_self_loop_side_label_keeps_its_position() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 240 220"><g class="node"><rect x="100" y="80" width="100" height="40"/></g><g class="edgePaths"><path data-id="loop" d="M100 90H80V110H100"/></g><g class="edgeLabel" transform="translate(40,100)"><g class="label" data-id="loop" transform="translate(-25,-9)"><rect width="50" height="18"/><text>retry</text></g></g></svg>"#;
    assert_eq!(position_edge_labels(svg).unwrap(), svg);
}

#[test]
fn state_lifecycle_uses_orthogonal_routing_instead_of_crossing_diagonals() {
    let source = "stateDiagram-v2\n[*] --> Editable\nEditable --> Rendered: render\nRendered --> Rendered: render unchanged\nRendered --> Modified: edit commented source\nEditable --> Modified: edit source\nModified --> Rendered: render updated SVG\nRendered --> [*]: commit files";
    let svg = render(source, &crate::render::palette::light_theme()).unwrap();
    let doc = Document::parse(&svg).unwrap();
    let paths: Vec<_> = doc
        .descendants()
        .filter(|node| node.attribute("data-edge") == Some("true"))
        .collect();
    assert_eq!(paths.len(), 7);
    for path in paths {
        let route = path.attribute("d").unwrap();
        assert!(
            !route.contains('C'),
            "default state routes must use straight runs and rounded corners: {route}"
        );
    }
}

#[test]
fn class_relationship_text_is_centered_on_its_connector() {
    let svg = render("classDiagram\nProject --> Document : contains\nDocument --> Diagram : produces\nDiagram --> Document : referenced by", &crate::render::palette::light_theme()).unwrap();
    let doc = Document::parse(&svg).unwrap();
    let paths: HashMap<_, _> = doc
        .descendants()
        .filter(|node| node.has_tag_name("path"))
        .filter_map(|node| Some((node.attribute("data-id")?, connector_points(node)?)))
        .collect();
    let mut count = 0;
    for label in doc
        .descendants()
        .filter(|node| has_class(*node, "label") && node.attribute("data-id").is_some())
    {
        let rows = visible_texts(label);
        assert!(
            !rows.is_empty(),
            "relationship label must contain visible text"
        );
        let center = block_center(&rows);
        let route = &paths[label.attribute("data-id").unwrap()];
        let distance = route
            .windows(2)
            .map(|segment| {
                squared_distance(center, project_to_segment(center, segment[0], segment[1]))
            })
            .fold(f64::INFINITY, f64::min)
            .sqrt();
        assert!(
            distance < 0.1,
            "class text must stay centered on its own connector; distance {distance}"
        );
        count += 1;
    }
    assert_eq!(count, 3);
}

#[test]
fn formatted_relationship_rows_center_as_a_block_on_the_route() {
    for source in [
        "flowchart TB\nA -->|First line<br/>Second line| B",
        "stateDiagram-v2\nA --> B: First line<br/>Second line",
        "classDiagram\nA --> B : First line<br/>Second line",
        "erDiagram\nA ||--o{ B : \"First line<br/>Second line\"",
    ] {
        let svg = render(source, &crate::render::palette::light_theme()).unwrap();
        let doc = Document::parse(&svg).unwrap();
        let paths: HashMap<_, _> = doc
            .descendants()
            .filter(|node| node.has_tag_name("path"))
            .filter_map(|node| Some((node.attribute("data-id")?, connector_points(node)?)))
            .collect();
        let label = doc
            .descendants()
            .find(|node| has_class(*node, "label") && node.attribute("data-id").is_some())
            .unwrap();
        let rows = visible_texts(label);
        assert_eq!(
            rows.len(),
            2,
            "multiline relationship must retain both rows: {source}"
        );
        let centers: Vec<_> = rows
            .iter()
            .map(|row| {
                assert_eq!(row.attribute("dominant-baseline"), Some("central"));
                assert_eq!(row.attribute("text-anchor"), Some("middle"));
                text_center(*row)
            })
            .collect();
        assert_eq!(
            rows.iter()
                .map(|row| text_content(*row))
                .collect::<Vec<_>>(),
            ["First line", "Second line"]
        );
        assert!((centers[0].0 - centers[1].0).abs() < 0.1);
        let line_height: f64 = inline_property(rows[0].attribute("style").unwrap(), "line-height")
            .unwrap()
            .trim_end_matches("px")
            .parse()
            .unwrap();
        assert!(
            (centers[1].1 - centers[0].1 - line_height).abs() < 0.1,
            "multiline spacing must retain the measured line height: {source}; {centers:?}"
        );
        let center = block_center(&rows);
        let route = &paths[label.attribute("data-id").unwrap()];
        let distance = route
            .windows(2)
            .map(|segment| {
                squared_distance(center, project_to_segment(center, segment[0], segment[1]))
            })
            .fold(f64::INFINITY, f64::min)
            .sqrt();
        assert!(
            distance < 0.1,
            "complete text block must center on its connector: {source}; {distance}"
        );
    }
}

#[test]
fn centering_formatted_relationship_text_preserves_authored_font_styles() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg"><g class="edgeLabel" transform="translate(70,90)"><g class="label" transform="translate(-40,-24)"><rect x="0" y="0" width="80" height="48"/><g transform="translate(40,24)"><text style="font-size:24px;font-family:serif;fill:#123456;font-style:italic" y="-10.1" text-anchor="middle"><tspan class="text-outer-tspan row" x="0" y="-0.1em" dy="1.1em"><tspan font-weight="bold">First</tspan></tspan><tspan class="text-outer-tspan row" x="0" y="1em" dy="1.1em"><tspan>Second</tspan></tspan></text></g></g></g></svg>"##;
    let centered = center_formatted_edge_text(source).unwrap();
    let doc = Document::parse(&centered).unwrap();
    let text = doc
        .descendants()
        .find(|node| node.has_tag_name("text"))
        .unwrap();
    assert_eq!(
        text.attribute("style"),
        Some("font-size:24px;font-family:serif;fill:#123456;font-style:italic")
    );
    assert!(
        text.descendants()
            .any(|node| node.attribute("font-weight") == Some("bold"))
    );
    assert_eq!(point_to_global(text, 0.0, 0.0).unwrap(), (70.0, 90.0));
    let ys: Vec<_> = text
        .children()
        .filter(Node::is_element)
        .map(|row| row.attribute("y").unwrap())
        .collect();
    assert_eq!(ys, ["-0.55em", "0.55em"]);
}

#[test]
fn html_label_families_export_complete_native_svg_text() {
    for (source, words) in [
        (
            "block-beta\ncolumns 3\nA[\"Start\"] B{\"Decision\"} C[\"Complete\"]\nA --> B\nB --> C",
            &["Start", "Decision", "Complete"][..],
        ),
        (
            "mindmap\n  root((Project))\n    Planning\n      Requirements\n    Delivery",
            &["Project", "Planning", "Requirements", "Delivery"][..],
        ),
        (
            "kanban\n  backlog[Backlog]\n    task[Implement diagram rendering]\n  done[Complete]\n    ready[Reviewed and verified]",
            &[
                "Backlog",
                "Implement diagram rendering",
                "Complete",
                "Reviewed and verified",
            ][..],
        ),
    ] {
        let svg = render(source, &crate::render::palette::light_theme()).unwrap();
        let doc = Document::parse(&svg).unwrap();
        assert!(
            !doc.descendants()
                .any(|node| node.has_tag_name("foreignObject")),
            "HTML labels must become portable native SVG text"
        );
        let text: String = doc
            .descendants()
            .filter(|node| {
                node.is_text() && node.ancestors().any(|parent| parent.has_tag_name("text"))
            })
            .filter_map(|node| node.text())
            .collect::<Vec<_>>()
            .join(" ");
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        for word in words {
            assert!(
                text.contains(word),
                "missing complete SVG label {word:?} in {text:?}"
            );
        }
    }
}

#[test]
fn usecase_text_is_centered_inside_its_ellipse() {
    let svg = render("usecase-beta\ndirection LR\nactor Applicant\nsystemBoundary \"Application service\"\nSubmit(\"Submit application\")\nend\nApplicant --> Submit", &crate::render::palette::light_theme()).unwrap();
    let doc = Document::parse(&svg).unwrap();
    assert!(
        !doc.descendants()
            .any(|node| node.has_tag_name("foreignObject"))
    );
    let node = doc
        .descendants()
        .find(|node| {
            node.attribute("data-usecase-id") == Some("Submit") && has_class(*node, "node")
        })
        .unwrap();
    let ellipse = node
        .children()
        .find(|node| node.has_tag_name("ellipse"))
        .unwrap();
    let rows = visible_texts(node);
    assert_eq!(rows.len(), 1);
    let text = rows[0];
    let ellipse_center = point_to_global(
        ellipse,
        number(ellipse, "cx", 0.0).unwrap(),
        number(ellipse, "cy", 0.0).unwrap(),
    )
    .unwrap();
    let text_center = text_center(text);
    assert!(
        (ellipse_center.0 - text_center.0).abs() < 1.0
            && (ellipse_center.1 - text_center.1).abs() < 1.0,
        "usecase text center {text_center:?} must match ellipse center {ellipse_center:?}"
    );
    let content: String = text
        .descendants()
        .filter(|node| node.is_text())
        .filter_map(|node| node.text())
        .collect();
    assert!(content.contains("Submit application"));
}

#[test]
fn c4_relationship_captions_do_not_overlap_shape_boxes() {
    use merman::svg::{DeterministicTextMeasurer, TextMeasurer, TextStyle};
    for (name, source) in [
        (
            "context",
            include_str!("../../../tests/fixtures/diagrams/c4-context.mmd"),
        ),
        (
            "container",
            include_str!("../../../tests/fixtures/diagrams/c4-container.mmd"),
        ),
        (
            "component",
            include_str!("../../../tests/fixtures/diagrams/c4-component.mmd"),
        ),
        (
            "dynamic",
            include_str!("../../../tests/fixtures/diagrams/c4-dynamic.mmd"),
        ),
        (
            "deployment",
            include_str!("../../../tests/fixtures/diagrams/c4-deployment.mmd"),
        ),
    ] {
        let svg = render(source, &crate::render::palette::light_theme()).unwrap();
        let doc = Document::parse(&svg).unwrap();
        let mut nodes: Vec<_> = doc
            .descendants()
            .filter(|node| has_class(*node, "node"))
            .filter_map(|node| {
                node.descendants()
                    .filter(|child| {
                        !child
                            .ancestors()
                            .take_while(|ancestor| *ancestor != node)
                            .any(|ancestor| has_class(ancestor, "label"))
                    })
                    .filter_map(shape_global_bounds)
                    .reduce(Bounds::union)
            })
            .collect();
        assert!(!nodes.is_empty());
        // Boundaries reserve a centered header separately from their
        // contained nodes. Incoming captions must also clear that header.
        nodes.extend(
            doc.descendants()
                .filter(|node| {
                    node.has_tag_name("text")
                        && node.parent_element().is_some_and(|parent| {
                            parent.children().any(|child| {
                                child.has_tag_name("rect")
                                    && child.attribute("stroke-dasharray").is_some()
                            })
                        })
                })
                .map(|text| {
                    let content: String = text
                        .descendants()
                        .filter(Node::is_text)
                        .filter_map(|node| node.text())
                        .collect();
                    let style = text.attribute("style").unwrap_or_default();
                    let metrics = DeterministicTextMeasurer::default().measure(
                        &content,
                        &TextStyle {
                            font_size: inline_property(style, "font-size")
                                .unwrap()
                                .trim_end_matches("px")
                                .parse()
                                .unwrap(),
                            font_weight: inline_property(style, "font-weight").map(str::to_owned),
                            ..Default::default()
                        },
                    );
                    let (x, y) = point_to_global(
                        text,
                        number(text, "x", 0.0).unwrap(),
                        number(text, "y", 0.0).unwrap(),
                    )
                    .unwrap();
                    Bounds {
                        min_x: x - metrics.width * 0.575,
                        max_x: x + metrics.width * 0.575,
                        min_y: y - metrics.height / 2.0,
                        max_y: y + metrics.height / 2.0,
                    }
                }),
        );
        let mut count = 0;
        for text in doc.descendants().filter(|node| {
            node.has_tag_name("text")
                && node.parent_element().is_some_and(|parent| {
                    parent.children().any(|child| {
                        child.has_tag_name("line")
                            || (child.has_tag_name("path")
                                && child.attribute("marker-end").is_some())
                    })
                })
        }) {
            let content: String = text
                .descendants()
                .filter(Node::is_text)
                .filter_map(|node| node.text())
                .collect();
            let metrics = DeterministicTextMeasurer::default().measure(
                &content,
                &TextStyle {
                    font_size: 12.0,
                    ..Default::default()
                },
            );
            let (x, y) = point_to_global(
                text,
                number(text, "x", 0.0).unwrap(),
                number(text, "y", 0.0).unwrap(),
            )
            .unwrap();
            let bounds = Bounds {
                min_x: x - metrics.width * 0.575,
                max_x: x + metrics.width * 0.575,
                min_y: y - metrics.height / 2.0,
                max_y: y + metrics.height / 2.0,
            };
            for node in &nodes {
                assert!(
                    overlap_area(bounds, *node) < 0.1,
                    "C4 {name} caption {content:?} {bounds:?} overlaps a shape {node:?}"
                );
            }
            count += 1;
        }
        assert!(
            count >= 2,
            "C4 {name} fixture must exercise multiple relationships"
        );
    }
}

#[test]
fn c4_multiline_and_technology_captions_keep_authored_offsets_from_the_connector() {
    let source = "C4Container\nContainer(a, \"A\", \"Rust\")\nContainer(b, \"B\", \"Rust\")\nRel(a, b, \"Reads<br/>records\", \"JSON over HTTPS\")\nUpdateRelStyle(a, b, $offsetX=\"19\", $offsetY=\"7\")";
    let svg = render(source, &crate::render::palette::light_theme()).unwrap();
    let doc = Document::parse(&svg).unwrap();
    let connector = doc
        .descendants()
        .find(|node| node.has_tag_name("line") && node.attribute("marker-end").is_some())
        .unwrap();
    let midpoint_x =
        (number(connector, "x1", 0.0).unwrap() + number(connector, "x2", 0.0).unwrap()) / 2.0;
    let midpoint_y =
        (number(connector, "y1", 0.0).unwrap() + number(connector, "y2", 0.0).unwrap()) / 2.0;
    let labels: Vec<_> = connector
        .parent_element()
        .unwrap()
        .children()
        .filter(|node| node.has_tag_name("text"))
        .collect();
    assert_eq!(
        labels.len(),
        3,
        "both label rows and technology caption must survive export"
    );
    for text in &labels {
        assert!(
            (number(*text, "x", 0.0).unwrap() - midpoint_x - 19.0).abs() < 1.0,
            "caption must retain the authored offset without a second half-width offset"
        );
    }
    for text in &labels[..2] {
        assert!((number(*text, "y", 0.0).unwrap() - midpoint_y - 7.0).abs() < 0.1);
    }
}

#[test]
fn architecture_bottom_connector_leaves_caption_clear_without_changing_icon() {
    let svg = render(
        include_str!("../../../tests/fixtures/diagrams/architecture.mmd"),
        &crate::render::palette::light_theme(),
    )
    .unwrap();
    let doc = Document::parse(&svg).unwrap();
    let api = doc
        .descendants()
        .find(|node| {
            has_class(*node, "architecture-service")
                && node.attribute("id").is_some_and(|id| id.ends_with("-api"))
        })
        .unwrap();
    let caption = api
        .descendants()
        .find(|node| node.has_tag_name("text"))
        .unwrap();
    let center = point_to_global(caption, 0.0, 0.0).unwrap();
    let connector = doc
        .descendants()
        .find(|node| {
            node.has_tag_name("path")
                && node
                    .attribute("id")
                    .is_some_and(|id| id.ends_with("L_api_database_0"))
        })
        .unwrap();
    let clip_id = connector
        .attribute("clip-path")
        .expect("caption must have transparent connector clearance")
        .strip_prefix("url(#")
        .unwrap()
        .strip_suffix(')')
        .unwrap();
    let clip = doc
        .descendants()
        .find(|node| node.attribute("id") == Some(clip_id))
        .unwrap();
    let path = clip
        .children()
        .find(|node| node.has_tag_name("path"))
        .unwrap();
    let mut boxes = Vec::new();
    let mut bounds = Bounds::empty();
    for segment in SimplifyingPathParser::from(path.attribute("d").unwrap()) {
        match segment.unwrap() {
            SimplePathSegment::MoveTo { x, y } => {
                bounds = Bounds::empty();
                bounds.point(x, y);
            }
            SimplePathSegment::LineTo { x, y } => bounds.point(x, y),
            SimplePathSegment::ClosePath => boxes.push(bounds),
            _ => {}
        }
    }
    assert!(
        boxes
            .iter()
            .skip(1)
            .any(|bounds| contains_point(*bounds, center)),
        "transparent hole must cover the actual API caption"
    );
    assert!(
        api.descendants()
            .filter(|node| node.has_tag_name("path"))
            .all(|node| node.attribute("clip-path").is_none()),
        "service icon geometry must stay intact"
    );
    assert!(!svg.contains("<mask"));
}
