use std::fs;
use std::path::Path;
use std::process::Command;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use roxmltree::{Document, Node};
use svgtypes::{SimplePathSegment, SimplifyingPathParser, Transform};
use tempfile::TempDir;

const WORKFLOWS: &str = include_str!("fixtures/project/docs/workflows.md");
const FLOWCHART: &str = include_str!("fixtures/diagrams/flowchart-directed-edges.mmd");
const STATE: &str = include_str!("fixtures/diagrams/state-directed-edges.mmd");
const SEQUENCE: &str = include_str!("fixtures/diagrams/sequence-directed-edges.mmd");

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl Rect {
    fn center(self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    fn boundary_distance(self, (x, y): (f64, f64)) -> f64 {
        let dx = (self.x - x).max(0.0).max(x - self.x - self.width);
        let dy = (self.y - y).max(0.0).max(y - self.y - self.height);
        if dx > 0.0 || dy > 0.0 {
            dx.hypot(dy)
        } else {
            (x - self.x)
                .min(self.x + self.width - x)
                .min(y - self.y)
                .min(self.y + self.height - y)
        }
    }

    fn overlap_area(self, other: Self) -> f64 {
        let width = (self.x + self.width).min(other.x + other.width) - self.x.max(other.x);
        let height = (self.y + self.height).min(other.y + other.height) - self.y.max(other.y);
        width.max(0.0) * height.max(0.0)
    }

    fn contains(self, other: Self, tolerance: f64) -> bool {
        other.x >= self.x - tolerance
            && other.y >= self.y - tolerance
            && other.x + other.width <= self.x + self.width + tolerance
            && other.y + other.height <= self.y + self.height + tolerance
    }
}

fn class(node: Node<'_, '_>, name: &str) -> bool {
    node.attribute("class")
        .is_some_and(|value| value.split_whitespace().any(|token| token == name))
}

fn numbers(value: &str) -> Vec<f64> {
    value
        .split(|character: char| character.is_ascii_whitespace() || character == ',')
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse()
                .unwrap_or_else(|_| panic!("invalid SVG number: {value}"))
        })
        .collect()
}

fn translation(node: Node<'_, '_>) -> (f64, f64) {
    let Some(transform) = node.attribute("transform") else {
        return (0.0, 0.0);
    };
    let coordinates = transform
        .trim()
        .strip_prefix("translate(")
        .and_then(|value| value.strip_suffix(')'))
        .unwrap_or_else(|| panic!("expected a translation in state geometry: {transform}"));
    let coordinates = numbers(coordinates);
    assert!((1..=2).contains(&coordinates.len()));
    (coordinates[0], coordinates.get(1).copied().unwrap_or(0.0))
}

fn global_translation(node: Node<'_, '_>) -> (f64, f64) {
    node.ancestors().fold((0.0, 0.0), |(x, y), ancestor| {
        let (dx, dy) = translation(ancestor);
        (x + dx, y + dy)
    })
}

fn attribute_number(node: Node<'_, '_>, attribute: &str) -> f64 {
    node.attribute(attribute).unwrap_or("0").parse().unwrap()
}

fn rect_bounds(node: Node<'_, '_>) -> Rect {
    let (x, y) = global_translation(node);
    Rect {
        x: x + attribute_number(node, "x"),
        y: y + attribute_number(node, "y"),
        width: attribute_number(node, "width"),
        height: attribute_number(node, "height"),
    }
}

fn label_bounds(node: Node<'_, '_>) -> Rect {
    if let Some(viewport) = node.descendants().find(|child| child.has_tag_name("svg")) {
        return rect_bounds(viewport);
    }
    let (half_width, half_height) = translation(node);
    let (x, y) = global_translation(node);
    let rect = Rect {
        x,
        y,
        width: -2.0 * half_width,
        height: -2.0 * half_height,
    };
    assert!(
        rect.width > 0.0 && rect.height > 0.0,
        "missing measured label bounds: {rect:?}"
    );
    rect
}

fn text(node: Node<'_, '_>) -> String {
    node.descendants()
        .filter(|child| child.is_text())
        .filter_map(|child| child.text())
        .collect()
}

fn render(root: &Path) -> String {
    let binary = std::env::var_os("MER_MD_TEST_BINARY")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_mer-md").into());
    let output = Command::new(binary)
        .current_dir(root)
        .arg("README.md")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let paths: Vec<_> = fs::read_dir(root.join("docs/diagrams"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "svg"))
        .collect();
    assert_eq!(paths.len(), 1);
    fs::read_to_string(&paths[0]).unwrap()
}

fn fixture(markdown: &str) -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("README.md"), markdown).unwrap();
    directory
}

fn viewbox(document: &Document<'_>) -> Rect {
    let coordinates = numbers(document.root_element().attribute("viewBox").unwrap());
    assert_eq!(coordinates.len(), 4);
    Rect {
        x: coordinates[0],
        y: coordinates[1],
        width: coordinates[2],
        height: coordinates[3],
    }
}

fn assert_disjoint(items: &[(String, Rect)], description: &str) {
    for (index, (first_name, first)) in items.iter().enumerate() {
        for (second_name, second) in &items[index + 1..] {
            assert!(
                first.overlap_area(*second) <= 1.0,
                "overlapping {description}: {first_name} {first:?}, {second_name} {second:?}"
            );
        }
    }
}

fn assert_transparent_canvas_and_labels(svg: &str) {
    let document = Document::parse(svg).unwrap();
    let canvas = viewbox(&document);
    for node in document
        .descendants()
        .filter(|node| node.has_tag_name("rect"))
    {
        assert!(
            !node
                .ancestors()
                .any(|ancestor| class(ancestor, "edgeLabel") || class(ancestor, "label")),
            "text label still has a background rectangle: {:?}",
            node.range()
        );
        let bounds = rect_bounds(node);
        let covers_canvas = bounds.contains(canvas, 0.5)
            && bounds.width >= canvas.width - 0.5
            && bounds.height >= canvas.height - 0.5;
        if covers_canvas {
            assert!(
                matches!(node.attribute("fill"), Some("none" | "transparent"))
                    || node.attribute("fill-opacity") == Some("0"),
                "opaque diagram canvas remains: {:?}",
                node.range()
            );
        }
    }
    assert!(
        !document
            .descendants()
            .any(|node| node.has_tag_name("foreignObject"))
    );
    assert!(
        !document
            .descendants()
            .any(|node| node.has_tag_name("script"))
    );
}

#[test]
fn exported_images_have_intrinsic_dimensions_instead_of_filling_the_viewer() {
    let directory = fixture("```mermaid\nflowchart LR\nA[Start] -->|Continue| B[Finish]\n```\n");
    let svg = render(directory.path());
    let document = Document::parse(&svg).unwrap();
    let root = document.root_element();
    let drawing = viewbox(&document);
    let width: f64 = root
        .attribute("width")
        .unwrap()
        .parse()
        .expect("intrinsic pixel width");
    let height: f64 = root
        .attribute("height")
        .unwrap()
        .parse()
        .expect("intrinsic pixel height");
    assert!((width - drawing.width).abs() < 1.0);
    assert!((height - drawing.height).abs() < 1.0);
    assert!(
        width < 800.0 && height < 300.0,
        "a two-node diagram should have a normal intrinsic size"
    );
    let style = root.attribute("style").unwrap();
    assert!(style.contains("max-width:100%") && style.contains("height:auto"));
}

#[test]
fn lifecycle_state_diagram_has_separate_labels_and_readable_node_geometry() {
    let directory = fixture(WORKFLOWS);
    let svg = render(directory.path());
    let document = Document::parse(&svg).unwrap();
    let canvas = viewbox(&document);
    let mut nodes = Vec::new();

    for node in document
        .descendants()
        .filter(|node| class(*node, "node") && class(*node, "statediagram-state"))
    {
        let shape = node
            .children()
            .find(|child| child.has_tag_name("rect") && class(*child, "label-container"))
            .unwrap();
        let label = node
            .children()
            .find(|child| class(*child, "label"))
            .unwrap();
        let name = text(label);
        let body = rect_bounds(shape);
        let label_bounds = label_bounds(label);
        assert!(
            label_bounds.height > 0.0 && body.contains(label_bounds, 1.0),
            "{name} text does not fit vertically in its node: {body:?}, {label_bounds:?}"
        );
        let (body_x, body_y) = body.center();
        let (label_x, label_y) = label_bounds.center();
        assert!(
            (body_x - label_x).abs() <= 1.0 && (body_y - label_y).abs() <= 1.0,
            "{name} text is not centered"
        );
        let rows: Vec<_> = label
            .descendants()
            .filter(|row| row.has_tag_name("text"))
            .collect();
        assert!(!rows.is_empty());
        for row in rows {
            assert_eq!(row.attribute("text-anchor"), Some("middle"));
            assert!(
                (global_point(
                    row,
                    (attribute_number(row, "x"), attribute_number(row, "y"))
                )
                .0 - body_x)
                    .abs()
                    <= 1.0
            );
        }
        assert!(
            canvas.contains(body, 1.0),
            "{name} node is clipped by the viewBox"
        );
        nodes.push((name, body));
    }
    assert_eq!(
        nodes.len(),
        3,
        "the lifecycle sample must render all three states"
    );

    let labels: Vec<_> = document
        .descendants()
        .filter(|node| class(*node, "edgeLabels"))
        .flat_map(|node| node.children())
        .filter_map(|group| {
            let label = group
                .children()
                .find(|child| child.attribute("data-id").is_some())?;
            let content = text(label);
            (!content.is_empty()).then(|| (content, label_bounds(label)))
        })
        .collect();
    assert_eq!(
        labels.len(),
        6,
        "all six transition labels must remain visible"
    );
    assert_disjoint(&nodes, "states");
    assert_disjoint(&labels, "transition labels");
    for (label_name, label) in &labels {
        assert!(
            canvas.contains(*label, 1.0),
            "transition label {label_name} is clipped"
        );
        for (node_name, node) in &nodes {
            assert!(
                label.overlap_area(*node) <= 1.0,
                "transition label {label_name} overlaps state {node_name}"
            );
        }
    }

    let center_y = |name: &str| {
        nodes
            .iter()
            .find(|(label, _)| label == name)
            .unwrap()
            .1
            .center()
            .1
    };
    assert!(center_y("Editable") < center_y("Rendered"));
    assert!(center_y("Rendered") < center_y("Modified"));
    let rendered = nodes.iter().find(|(name, _)| name == "Rendered").unwrap().1;
    let self_loop = document
        .descendants()
        .find(|node| node.has_tag_name("path") && node.attribute("data-id") == Some("edge2"))
        .unwrap();
    let points = STANDARD
        .decode(self_loop.attribute("data-points").unwrap())
        .unwrap();
    let points: serde_json::Value = serde_json::from_slice(&points).unwrap();
    let local_bounds = Rect {
        x: rendered.x - rendered.width,
        y: rendered.y - rendered.height * 2.0,
        width: rendered.width * 3.0,
        height: rendered.height * 5.0,
    };
    for point in points.as_array().unwrap() {
        let point = Rect {
            x: point["x"].as_f64().unwrap(),
            y: point["y"].as_f64().unwrap(),
            width: 0.0,
            height: 0.0,
        };
        assert!(
            local_bounds.contains(point, 1.0),
            "the unchanged transition should loop locally around Rendered: {point:?}"
        );
    }
    let start = document
        .descendants()
        .find(|node| class(*node, "state-start"))
        .unwrap();
    assert!(global_translation(start).1 < center_y("Editable"));
    let end = document
        .descendants()
        .find(|node| {
            class(*node, "node")
                && node
                    .attribute("id")
                    .is_some_and(|id| id.contains("root_end"))
        })
        .unwrap();
    assert!(global_translation(end).1 > center_y("Rendered"));
    assert_transparent_canvas_and_labels(&svg);
}

#[test]
fn transparent_diagram_contains_both_automatic_colour_palettes() {
    let directory = fixture(WORKFLOWS);
    let svg = render(directory.path());
    assert_transparent_canvas_and_labels(&svg);
    assert!(
        svg.contains("prefers-color-scheme:dark") || svg.contains("prefers-color-scheme: dark")
    );
    assert!(svg.contains("#EEE9FF") && svg.contains("#363052"));
}

#[test]
fn fixed_node_fills_have_readable_labels_in_the_adaptive_svg() {
    let directory = fixture(
        "```mermaid\nflowchart LR\nA[Light] --> B[Dark]\nB --> C[Explicit]\nC --> D[Default]\nstyle A fill:#ffffff\nstyle B fill:#000000\nstyle C fill:#ffffff,color:#123456\n```\n",
    );
    let svg = render(directory.path());
    let document = Document::parse(&svg).unwrap();
    for (label, colour) in [
        ("Light", "#263347"),
        ("Dark", "#E7EDF7"),
        ("Explicit", "#123456"),
    ] {
        let label = document
            .descendants()
            .find(|node| node.has_tag_name("text") && text(*node) == label)
            .unwrap();
        assert!(label.attribute("style").unwrap().contains(colour));
    }
    let default = document
        .descendants()
        .find(|node| node.has_tag_name("text") && text(*node) == "Default")
        .unwrap();
    let default_style = default.attribute("style").unwrap_or("");
    assert!(
        !default_style.contains("fill:") || default_style.contains("fill:var(--pm-paint-"),
        "default text must follow the adaptive theme: {default_style}"
    );
    assert!(svg.contains("#EEE9FF") && svg.contains("#363052"));
    assert_transparent_canvas_and_labels(&svg);
}

#[test]
fn explicit_node_colours_survive_automatic_theme_adaptation() {
    let directory = fixture(
        "```mermaid\nflowchart LR\nA[Custom] --> B[Default]\nstyle A fill:#FF5733,stroke:#1565C0,color:#FAFAFA\n```\n",
    );
    let svg = render(directory.path());
    let lowercase = svg.to_ascii_lowercase();
    for colour in ["#ff5733", "#1565c0", "#fafafa"] {
        assert!(
            lowercase.contains(colour),
            "explicit style {colour} was lost"
        );
    }
    let document = Document::parse(&svg).unwrap();
    assert!(
        document
            .descendants()
            .any(|node| node.has_tag_name("text") && text(node) == "Custom")
    );
    assert_transparent_canvas_and_labels(&svg);
}

fn render_source(source: &str) -> String {
    let directory = fixture(&format!("```mermaid\n{source}\n```\n"));
    render(directory.path())
}

fn global_point(node: Node<'_, '_>, mut point: (f64, f64)) -> (f64, f64) {
    for ancestor in node.ancestors().filter(Node::is_element) {
        if let Some(value) = ancestor.attribute("transform") {
            let transform: Transform = value.parse().expect("valid SVG transform");
            point = (
                transform.a * point.0 + transform.c * point.1 + transform.e,
                transform.b * point.0 + transform.d * point.1 + transform.f,
            );
        }
    }
    point
}

fn node_bounds(document: &Document<'_>, family: &str, id: &str) -> Rect {
    let prefix = format!("mer-md-{family}-{id}-");
    let node = document
        .descendants()
        .find(|node| {
            class(*node, "node")
                && node
                    .attribute("id")
                    .is_some_and(|id| id.starts_with(&prefix))
        })
        .unwrap_or_else(|| panic!("missing diagram node {prefix}"));
    let shape = node
        .children()
        .find(|child| child.has_tag_name("rect") && class(*child, "label-container"))
        .unwrap_or_else(|| panic!("missing rectangle for node {id}"));
    rect_bounds(shape)
}

fn path_endpoints(path: Node<'_, '_>) -> ((f64, f64), (f64, f64)) {
    let mut start = None;
    let mut end = None;
    for segment in SimplifyingPathParser::from(path.attribute("d").expect("SVG path data")) {
        let point = match segment.expect("valid SVG path segment") {
            SimplePathSegment::MoveTo { x, y }
            | SimplePathSegment::LineTo { x, y }
            | SimplePathSegment::CurveTo { x, y, .. }
            | SimplePathSegment::Quadratic { x, y, .. } => (x, y),
            SimplePathSegment::ClosePath => continue,
        };
        start.get_or_insert(point);
        end = Some(point);
    }
    (
        global_point(path, start.expect("path start")),
        global_point(path, end.expect("path end")),
    )
}

fn edge<'a>(document: &'a Document<'a>, id: &str) -> Node<'a, 'a> {
    document
        .descendants()
        .find(|node| node.has_tag_name("path") && node.attribute("data-id") == Some(id))
        .unwrap_or_else(|| panic!("missing edge {id}"))
}

fn assert_marker(document: &Document<'_>, node: Node<'_, '_>, attribute: &str, present: bool) {
    let value = node.attribute(attribute);
    assert_eq!(value.is_some(), present, "{attribute}: {:?}", node.range());
    if let Some(value) = value {
        let id = value
            .strip_prefix("url(#")
            .and_then(|value| value.strip_suffix(')'))
            .unwrap_or_else(|| panic!("marker must reference an SVG definition: {value}"));
        assert!(
            document
                .descendants()
                .any(|node| node.has_tag_name("marker") && node.attribute("id") == Some(id))
        );
    }
}

fn assert_connects(path: Node<'_, '_>, source: Rect, target: Rect) {
    let (start, end) = path_endpoints(path);
    // Mermaid leaves a small gap for the arrowhead between its path and node.
    assert!(
        source.boundary_distance(start) < 8.0,
        "edge {:?} starts away from its source: {start:?}, {source:?}",
        path.attribute("data-id")
    );
    assert!(
        target.boundary_distance(end) < 8.0,
        "edge {:?} ends away from its target: {end:?}, {target:?}",
        path.attribute("data-id")
    );
}

#[test]
fn flowchart_arrows_keep_their_sources_targets_and_line_styles() {
    let svg = render_source(FLOWCHART);
    let document = Document::parse(&svg).unwrap();
    let cases = [
        ("A", "B", false, true),
        ("B", "C", false, true),
        ("C", "A", false, true),
        ("B", "B", false, true),
        ("D", "E", true, true),
        ("F", "G", false, true),
        ("H", "I", false, true),
        ("J", "K", false, false),
        ("L", "M", true, true),
        ("N", "O", true, true),
    ];
    assert_eq!(
        document
            .descendants()
            .filter(|node| node.attribute("data-edge") == Some("true"))
            .count(),
        cases.len()
    );
    for (source, target, start_marker, end_marker) in cases {
        let path = edge(&document, &format!("L_{source}_{target}_0"));
        assert_marker(&document, path, "marker-start", start_marker);
        assert_marker(&document, path, "marker-end", end_marker);
        assert_connects(
            path,
            node_bounds(&document, "flowchart", source),
            node_bounds(&document, "flowchart", target),
        );
    }
    for id in ["L_F_G_0", "L_L_M_0"] {
        assert!(class(edge(&document, id), "edge-pattern-dotted"));
    }
    for id in ["L_H_I_0", "L_N_O_0"] {
        assert!(class(edge(&document, id), "edge-thickness-thick"));
    }
    let (start, end) = path_endpoints(edge(&document, "L_C_A_0"));
    assert!(
        start.0 > end.0,
        "feedback arrow must return to its source column"
    );
    for label in ["forward", "return", "repeat", "exchange", "dotted", "thick"] {
        assert!(
            document
                .descendants()
                .any(|node| class(node, "edgeLabel") && text(node) == label)
        );
    }
}

#[test]
fn state_transitions_keep_feedback_and_self_loop_direction() {
    let svg = render_source(STATE);
    let document = Document::parse(&svg).unwrap();
    assert_eq!(
        document
            .descendants()
            .filter(|node| class(*node, "transition"))
            .count(),
        6
    );
    for (id, source, target) in [
        ("edge1", "Draft", "Review"),
        ("edge2", "Review", "Draft"),
        ("edge3", "Review", "Published"),
        ("edge4", "Review", "Review"),
    ] {
        let path = edge(&document, id);
        assert_marker(&document, path, "marker-start", false);
        assert_marker(&document, path, "marker-end", true);
        assert_connects(
            path,
            node_bounds(&document, "state", source),
            node_bounds(&document, "state", target),
        );
    }
    let (start, end) = path_endpoints(edge(&document, "edge2"));
    assert!(start.1 > end.1, "retry must return from Review to Draft");
    let self_loop = edge(&document, "edge4").attribute("d").unwrap();
    assert!(
        SimplifyingPathParser::from(self_loop).count() > 2,
        "self transition must loop around its state"
    );
    for label in ["submit", "retry", "approve", "inspect"] {
        assert!(
            document
                .descendants()
                .any(|node| class(node, "edgeLabel") && text(node) == label)
        );
    }
}

#[test]
fn sequence_arrows_keep_participant_direction_and_message_types() {
    let svg = render_source(SEQUENCE);
    let document = Document::parse(&svg).unwrap();
    let participant_x = |name| {
        let actor = document
            .descendants()
            .find(|node| {
                node.has_tag_name("rect")
                    && class(*node, "actor-top")
                    && node.attribute("name") == Some(name)
            })
            .expect("top participant box");
        rect_bounds(actor).center().0
    };
    let cases = [
        ("Requester", "Worker", false, "arrowhead"),
        ("Worker", "Requester", false, "arrowhead"),
        ("Worker", "Worker", false, "arrowhead"),
        ("Requester", "Worker", true, "arrowhead"),
        ("Requester", "Worker", false, "crosshead"),
        ("Worker", "Requester", false, "filled-head"),
    ];
    let messages: Vec<_> = document
        .descendants()
        .filter(|node| node.attribute("data-et") == Some("message"))
        .collect();
    assert_eq!(messages.len(), cases.len());
    for (index, (source, target, both, marker)) in cases.into_iter().enumerate() {
        let id = format!("i{index}");
        let message = messages
            .iter()
            .copied()
            .find(|node| node.attribute("data-id") == Some(id.as_str()))
            .unwrap();
        assert_eq!(message.attribute("data-from"), Some(source));
        assert_eq!(message.attribute("data-to"), Some(target));
        assert_marker(&document, message, "marker-start", both);
        assert_marker(&document, message, "marker-end", true);
        assert!(message.attribute("marker-end").unwrap().contains(marker));
        let (start, end) = if message.has_tag_name("line") {
            (
                global_point(
                    message,
                    (
                        attribute_number(message, "x1"),
                        attribute_number(message, "y1"),
                    ),
                ),
                global_point(
                    message,
                    (
                        attribute_number(message, "x2"),
                        attribute_number(message, "y2"),
                    ),
                ),
            )
        } else {
            path_endpoints(message)
        };
        assert!((start.0 - participant_x(source)).abs() < 8.0);
        assert!((end.0 - participant_x(target)).abs() < 8.0);
        if source != target {
            assert_eq!(
                end.0 > start.0,
                participant_x(target) > participant_x(source)
            );
        } else {
            assert!(end.1 > start.1 && message.attribute("d").unwrap().contains('C'));
        }
    }
    for id in ["i1", "i4"] {
        let message = messages
            .iter()
            .find(|node| node.attribute("data-id") == Some(id))
            .unwrap();
        assert!(class(*message, "messageLine1"));
        assert!(
            message
                .attribute("style")
                .unwrap()
                .contains("stroke-dasharray")
        );
    }
}
