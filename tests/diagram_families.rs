//! CLI coverage for the native Mermaid diagram families.
//! Layout-specific overlap and routing assertions live in render_quality.rs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use roxmltree::Document;
use serde_json::Value;
use tempfile::TempDir;

const CATALOG: &str = include_str!("fixtures/diagrams/catalog.json");

fn command(directory: &Path, args: &[&str]) -> Output {
    let binary = std::env::var_os("MER_MD_TEST_BINARY")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_mer-md").into());
    Command::new(binary)
        .current_dir(directory)
        .args(args)
        .output()
        .expect("run portable CLI")
}

fn assert_success(output: &Output, slug: &str) {
    assert!(
        output.status.success(),
        "{slug} failed ({}):\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn compact(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn numbers(value: &str) -> Vec<f64> {
    value
        .split(|character: char| character.is_whitespace() || character == ',')
        .filter(|part| !part.is_empty())
        .map(|part| part.parse().expect("finite SVG geometry number"))
        .collect()
}

fn assert_svg(slug: &str, svg: &str, labels: &[Value]) {
    let doc = Document::parse(svg).unwrap_or_else(|error| panic!("{slug}: invalid SVG: {error}"));
    let root = doc.root_element();
    assert_eq!(root.tag_name().name(), "svg", "{slug}: missing SVG root");
    assert_eq!(
        root.tag_name().namespace(),
        Some("http://www.w3.org/2000/svg")
    );
    let viewbox = root.attribute("viewBox").map(numbers);
    if let Some(bounds) = &viewbox {
        assert_eq!(bounds.len(), 4, "{slug}: malformed viewBox");
        assert!(
            bounds.iter().all(|number| number.is_finite()),
            "{slug}: nonfinite viewBox"
        );
        assert!(bounds[2] > 0.0 && bounds[3] > 0.0, "{slug}: empty viewport");
        for (name, expected) in [("width", bounds[2]), ("height", bounds[3])] {
            let actual: f64 = root
                .attribute(name)
                .unwrap_or_else(|| panic!("{slug}: missing intrinsic {name}"))
                .parse()
                .unwrap_or_else(|_| panic!("{slug}: {name} must be an intrinsic pixel length"));
            assert!(
                (actual - expected).abs() < 1.0,
                "{slug}: {name} must match the drawing size"
            );
        }
    } else {
        // Mermaid's info utility intentionally emits a responsive SVG without
        // a viewBox. Every actual diagram must have explicit drawing bounds.
        assert_eq!(slug, "info", "{slug}: SVG has no viewBox");
    }
    assert!(
        svg.contains("prefers-color-scheme"),
        "{slug}: automatic theme stylesheet is missing"
    );

    let text: String = doc
        .descendants()
        .filter(|node| {
            node.is_text()
                && node
                    .ancestors()
                    .any(|parent| matches!(parent.tag_name().name(), "text" | "foreignObject"))
        })
        .filter_map(|node| node.text())
        .collect();
    let visible_text = compact(&text);
    for expected in labels {
        let expected = expected.as_str().expect("catalog label string");
        assert!(
            visible_text.contains(&compact(expected)),
            "{slug}: visible label {expected:?} is missing; SVG text: {text}"
        );
    }

    for node in doc.descendants().filter(|node| node.is_element()) {
        assert_ne!(
            node.tag_name().name(),
            "script",
            "{slug}: runtime script in SVG"
        );
        for attribute in node.attributes() {
            if matches!(attribute.name(), "d" | "points" | "transform" | "viewBox") {
                let invalid = attribute
                    .value()
                    .split(|character: char| !character.is_ascii_alphabetic() && character != '-')
                    .any(|part| matches!(part, "NaN" | "Infinity" | "-Infinity" | "inf" | "-inf"));
                assert!(
                    !invalid,
                    "{slug}: nonfinite SVG geometry: {}",
                    attribute.value()
                );
            }
        }
        if node.has_tag_name("image") {
            for href in node
                .attributes()
                .filter(|attribute| attribute.name() == "href")
            {
                assert!(
                    href.value().starts_with('#') || href.value().starts_with("data:"),
                    "{slug}: image requires an external resource: {}",
                    href.value()
                );
            }
        }
        if node.has_tag_name("rect")
            && node.parent_element() == Some(root)
            && let Some(bounds) = &viewbox
        {
            let number = |name, fallback| {
                node.attribute(name)
                    .and_then(|value| value.parse::<f64>().ok())
                    .unwrap_or(fallback)
            };
            let covers_canvas = (number("x", 0.0) - bounds[0]).abs() < 0.1
                && (number("y", 0.0) - bounds[1]).abs() < 0.1
                && number("width", 0.0) >= bounds[2] - 0.1
                && number("height", 0.0) >= bounds[3] - 0.1;
            if covers_canvas {
                assert!(
                    matches!(node.attribute("fill"), Some("none" | "transparent"))
                        || node.attribute("fill-opacity") == Some("0"),
                    "{slug}: opaque canvas rectangle"
                );
            }
        }
    }
}

fn managed_image(slug: &str, markdown: &str) -> (String, String) {
    assert!(
        markdown.starts_with(&format!("# {slug}\n\nProse before.\n\n"))
            && markdown.ends_with("\nProse after.\n"),
        "{slug}: surrounding prose changed"
    );
    assert_eq!(
        markdown.matches("<!-- portable-mermaid:source").count(),
        1,
        "{slug}: expected one editable source comment"
    );
    assert!(
        markdown.contains("<!-- portable-mermaid:source\n```mermaid\n"),
        "{slug}: Mermaid fence is missing from the source comment"
    );
    let (id, block) = markdown
        .split_once("<!-- portable-mermaid:begin id=")
        .unwrap_or_else(|| panic!("{slug}: managed diagram start is missing"))
        .1
        .split_once(" -->")
        .unwrap_or_else(|| panic!("{slug}: diagram identity is missing"));
    let image_markup = block
        .split_once("<!-- portable-mermaid:source")
        .unwrap()
        .0
        .trim();
    let image_document = Document::parse(image_markup)
        .unwrap_or_else(|error| panic!("{slug}: invalid image wrapper: {error}"));
    let paragraph = image_document.root_element();
    assert!(
        paragraph.has_tag_name("p"),
        "{slug}: missing image paragraph"
    );
    assert_eq!(paragraph.attribute("align"), Some("center"), "{slug}");
    let images: Vec<_> = paragraph
        .descendants()
        .filter(|node| node.has_tag_name("img"))
        .collect();
    assert_eq!(
        images.len(),
        1,
        "{slug}: missing or duplicate image reference"
    );
    assert_eq!(
        images[0].attribute("alt"),
        Some("Mermaid diagram"),
        "{slug}"
    );
    let image = images[0]
        .attribute("src")
        .unwrap_or_else(|| panic!("{slug}: image source is missing"));
    (id.to_owned(), image.to_owned())
}

fn svg_paths(directory: &Path) -> BTreeSet<PathBuf> {
    fs::read_dir(directory.join("docs/diagrams"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "svg"))
        .collect()
}

fn assert_snapshots(snapshots: &BTreeMap<PathBuf, Vec<u8>>) {
    for (path, bytes) in snapshots {
        assert_eq!(
            fs::read(path).unwrap(),
            *bytes,
            "{} changed",
            path.display()
        );
    }
}

macro_rules! diagram_cases {
    ($( $slug:literal ),* $(,)?) => {
        const CASES: &[(&str, &str)] = &[
            $(($slug, include_str!(concat!("fixtures/diagrams/", $slug, ".mmd")))),*
        ];
    };
}

diagram_cases! {
    "flowchart",
    "graph-alias",
    "flowchart-elk",
    "swimlane",
    "sequence",
    "class",
    "state",
    "state-alias",
    "er",
    "journey",
    "gantt",
    "pie",
    "quadrant",
    "requirement",
    "usecase",
    "gitgraph",
    "c4-context",
    "c4-container",
    "c4-component",
    "c4-dynamic",
    "c4-deployment",
    "mindmap",
    "timeline",
    "zenuml",
    "sankey",
    "xychart",
    "block",
    "packet",
    "kanban",
    "architecture",
    "radar",
    "event-modeling",
    "treemap",
    "venn",
    "ishikawa",
    "wardley",
    "cynefin",
    "treeview",
    "agentflow",
    "railroad",
    "railroad-ebnf",
    "railroad-abnf",
    "railroad-peg",
    "info",
}

#[test]
fn renders_caches_and_restores_all_diagram_families_in_a_project_batch() {
    let catalog: Value = serde_json::from_str(CATALOG).unwrap();
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    let mut originals = BTreeMap::new();
    for &(slug, source) in CASES {
        let markdown =
            format!("# {slug}\n\nProse before.\n\n```mermaid\n{source}```\n\nProse after.\n");
        fs::write(root.join(format!("{slug}.md")), &markdown).unwrap();
        originals.insert(slug, markdown);
    }
    assert_success(&command(root, &["."]), "diagram family batch");
    let mut snapshots = BTreeMap::new();
    let mut identities = BTreeMap::new();
    let mut assets = BTreeSet::new();
    for &(slug, _) in CASES {
        let markdown_path = root.join(format!("{slug}.md"));
        let markdown = fs::read_to_string(&markdown_path).unwrap();
        let (id, image) = managed_image(slug, &markdown);
        let svg_path = root.join(&image);
        assert!(
            svg_path.starts_with(root.join("docs/diagrams")),
            "{slug}: image is outside the diagram directory"
        );
        assert!(
            assets.insert(svg_path.clone()),
            "{slug}: two diagrams share an SVG asset"
        );
        let case = catalog["fixtures"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["slug"] == slug)
            .unwrap();
        let svg = fs::read_to_string(&svg_path)
            .unwrap_or_else(|error| panic!("{slug}: missing linked SVG: {error}"));
        assert_svg(slug, &svg, case["labels"].as_array().unwrap());
        snapshots.insert(markdown_path, markdown.into_bytes());
        snapshots.insert(svg_path, svg.into_bytes());
        identities.insert(slug, id);
    }
    assert_eq!(
        svg_paths(root),
        assets,
        "unexpected or missing diagram assets"
    );

    let repeated = command(root, &["."]);
    assert_success(&repeated, "cached diagram family batch");
    assert!(String::from_utf8_lossy(&repeated.stdout).contains(&format!(
        "{} diagram(s), {} cached.",
        CASES.len(),
        CASES.len()
    )));
    assert_snapshots(&snapshots);
    assert_eq!(svg_paths(root), assets);

    let markdown_path = root.join("flowchart.md");
    let edited = fs::read_to_string(&markdown_path).unwrap().replacen(
        "Receive request",
        "Receive revised request",
        1,
    );
    fs::write(&markdown_path, edited).unwrap();
    let updated = command(root, &["."]);
    assert_success(&updated, "edited diagram family batch");
    assert!(String::from_utf8_lossy(&updated.stdout).contains(&format!(
        "{} diagram(s), {} cached.",
        CASES.len(),
        CASES.len() - 1
    )));
    let markdown = fs::read_to_string(&markdown_path).unwrap();
    let (id, image) = managed_image("flowchart", &markdown);
    assert_eq!(
        id, identities["flowchart"],
        "source edits must retain diagram identity"
    );
    let svg_path = root.join(image);
    assert!(
        assets.contains(&svg_path),
        "source edit created a second SVG"
    );
    let svg = fs::read_to_string(&svg_path).unwrap();
    assert_ne!(svg.as_bytes(), snapshots[&svg_path]);
    assert_svg(
        "flowchart",
        &svg,
        &[Value::String("Receive revised request".to_owned())],
    );
    snapshots.insert(markdown_path, markdown.into_bytes());
    snapshots.insert(svg_path, svg.into_bytes());
    let original = originals.get_mut("flowchart").unwrap();
    *original = original.replacen("Receive request", "Receive revised request", 1);
    assert_snapshots(&snapshots);
    assert_eq!(svg_paths(root), assets);

    assert_success(
        &command(root, &[".", "--restore"]),
        "restore diagram family batch",
    );
    for &(slug, _) in CASES {
        let restored = fs::read_to_string(root.join(format!("{slug}.md"))).unwrap();
        let expected = originals[slug].replacen(
            "```mermaid\n",
            &format!(
                "<!-- portable-mermaid:id {} -->\n```mermaid\n",
                identities[slug]
            ),
            1,
        );
        assert_eq!(
            restored, expected,
            "{slug}: restore changed the source or prose"
        );
    }
    for path in &assets {
        assert_eq!(
            fs::read(path).unwrap(),
            snapshots[path],
            "{} changed during restore",
            path.display()
        );
    }
    assert_eq!(svg_paths(root), assets);

    let rerendered = command(root, &["."]);
    assert_success(&rerendered, "render restored diagram family batch");
    assert!(
        String::from_utf8_lossy(&rerendered.stdout).contains(&format!(
            "{} diagram(s), {} cached.",
            CASES.len(),
            CASES.len()
        ))
    );
    assert_snapshots(&snapshots);
    assert_eq!(
        svg_paths(root),
        assets,
        "restored diagrams must reuse their SVG assets"
    );
}

#[test]
fn catalog_matches_available_fixtures_and_render_cases() {
    let catalog: Value = serde_json::from_str(CATALOG).unwrap();
    let fixtures = catalog["fixtures"].as_array().unwrap();
    assert_eq!(fixtures.len(), 44, "diagram family coverage is incomplete");
    let names: BTreeSet<_> = fixtures
        .iter()
        .map(|case| {
            let slug = case["slug"].as_str().expect("fixture slug");
            assert!(!case["family"].as_str().expect("diagram family").is_empty());
            let labels = case["labels"].as_array().expect("expected visible labels");
            assert!(
                !labels.is_empty(),
                "{slug}: fixture needs visible label checks"
            );
            assert!(
                labels
                    .iter()
                    .all(|label| label.as_str().is_some_and(|value| !value.is_empty()))
            );
            slug
        })
        .collect();
    assert_eq!(names.len(), fixtures.len(), "duplicate fixture slug");
    let compiled_names: BTreeSet<_> = CASES.iter().map(|(slug, _)| *slug).collect();
    assert_eq!(compiled_names, names, "catalog and render cases differ");
    for (slug, source) in CASES {
        assert!(!source.trim().is_empty(), "{slug}: empty diagram fixture");
    }
}

fn assert_invalid_source(source: &str) {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("README.md");
    let original = format!("# Invalid input\n\n```mermaid\n{source}\n```\n");
    fs::write(&path, &original).unwrap();
    let output = command(directory.path(), &["README.md"]);
    assert!(
        !output.status.success(),
        "invalid source was silently rendered"
    );
    assert_eq!(fs::read_to_string(path).unwrap(), original);
    assert!(!directory.path().join("docs/diagrams").exists());
}

#[test]
fn unknown_diagram_is_rejected_without_changing_markdown() {
    assert_invalid_source("not a Mermaid diagram");
}

#[test]
fn malformed_frontmatter_is_rejected_without_changing_markdown() {
    assert_invalid_source("---\nconfig:\n  layout: elk");
}
