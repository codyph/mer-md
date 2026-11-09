use std::fs;
use std::path::Path;
use std::process::Command;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use roxmltree::{Document, Node};
use tempfile::TempDir;

const WORKFLOWS: &str = include_str!("fixtures/project/docs/workflows.md");

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
    // Merman retains the measured text block's half-size translation when
    // decorative label backgrounds are removed from the exported SVG.
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
        let (_, half_height) = translation(label);
        let (label_x, label_top) = global_translation(label);
        let label_bounds = Rect {
            x: body.x,
            y: label_top,
            width: body.width,
            height: -2.0 * half_height,
        };
        assert!(
            label_bounds.height > 0.0 && body.contains(label_bounds, 1.0),
            "{name} text does not fit vertically in its node: {body:?}, {label_bounds:?}"
        );
        let (body_x, body_y) = body.center();
        let label_y = label_bounds.center().1;
        assert!(
            (body_x - label_x).abs() <= 1.0 && (body_y - label_y).abs() <= 1.0,
            "{name} text is not centered"
        );
        let rows: Vec<_> = label
            .descendants()
            .filter(|row| class(*row, "text-outer-tspan"))
            .collect();
        assert!(!rows.is_empty());
        for row in rows {
            assert_eq!(row.attribute("text-anchor"), Some("middle"));
            assert!(attribute_number(row, "x").abs() <= 1.0);
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
