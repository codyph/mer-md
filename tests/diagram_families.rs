//! Representative CLI coverage for the complete Mermaid 12.3 diagram catalog.
//! Layout-specific overlap and routing assertions live in render_quality.rs.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
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

fn render_case(slug: &str, source: &str) {
    let catalog: Value = serde_json::from_str(CATALOG).unwrap();
    let case = catalog["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["slug"] == slug)
        .unwrap_or_else(|| panic!("{slug}: missing catalog entry"));
    let directory = TempDir::new().unwrap();
    let markdown_path = directory.path().join("README.md");
    let original =
        format!("# {slug}\n\nProse before.\n\n```mermaid\n{source}```\n\nProse after.\n");
    fs::write(&markdown_path, &original).unwrap();
    assert_success(&command(directory.path(), &["README.md"]), slug);
    let markdown = fs::read_to_string(&markdown_path).unwrap();
    assert!(markdown.contains("Prose before.") && markdown.contains("Prose after."));
    assert_eq!(markdown.matches("<!-- portable-mermaid:source").count(), 1);
    let image_markup = markdown
        .split_once("<!-- portable-mermaid:begin id=")
        .expect("managed diagram start")
        .1
        .split_once("-->")
        .unwrap()
        .1
        .split_once("<!-- portable-mermaid:source")
        .expect("preserved source")
        .0
        .trim();
    let image_document = Document::parse(image_markup)
        .unwrap_or_else(|error| panic!("{slug}: invalid image wrapper: {error}"));
    let paragraph = image_document.root_element();
    assert!(
        paragraph.has_tag_name("p"),
        "{slug}: missing image paragraph"
    );
    assert_eq!(paragraph.attribute("align"), Some("center"));
    let images: Vec<_> = paragraph
        .descendants()
        .filter(|node| node.has_tag_name("img"))
        .collect();
    assert_eq!(
        images.len(),
        1,
        "{slug}: missing or duplicate image reference"
    );
    assert_eq!(images[0].attribute("alt"), Some("Mermaid diagram"));
    let svg_paths: Vec<_> = fs::read_dir(directory.path().join("docs/diagrams"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "svg"))
        .collect();
    assert_eq!(svg_paths.len(), 1, "{slug}: expected one SVG asset");
    assert_eq!(
        directory
            .path()
            .join(images[0].attribute("src").expect("image source")),
        svg_paths[0],
        "{slug}: image must link to the generated asset"
    );
    let svg = fs::read_to_string(&svg_paths[0]).unwrap();
    assert_svg(slug, &svg, case["labels"].as_array().unwrap());

    // Exercise the actual persisted source/cache/link path, not just a direct
    // renderer call: a second render must need no changes or duplicate assets.
    assert_success(&command(directory.path(), &["README.md"]), slug);
    assert_eq!(fs::read_to_string(&markdown_path).unwrap(), markdown);
    assert_eq!(fs::read_to_string(&svg_paths[0]).unwrap(), svg);
    assert_success(
        &command(directory.path(), &["--restore", "README.md"]),
        slug,
    );
    let restored = fs::read_to_string(&markdown_path).unwrap();
    assert!(
        restored.contains(source),
        "{slug}: restore lost or modified the Mermaid source"
    );
}

macro_rules! diagram_cases {
    ($( $name:ident => $slug:literal ),* $(,)?) => {
        const FIXTURE_NAMES: &[&str] = &[$($slug),*];
        $(
            #[test]
            fn $name() {
                render_case($slug, include_str!(concat!("fixtures/diagrams/", $slug, ".mmd")));
            }
        )*
    };
}

diagram_cases! {
    flowchart_decisions_and_feedback => "flowchart",
    flowchart_graph_alias => "graph-alias",
    flowchart_explicit_elk => "flowchart-elk",
    swimlane_handoffs => "swimlane",
    sequence_alternative_outcomes => "sequence",
    class_relationships => "class",
    state_feedback_and_self_loop => "state",
    state_header_alias => "state-alias",
    entity_relationship_attributes => "er",
    user_journey_sections => "journey",
    gantt_dependencies => "gantt",
    pie_outcomes => "pie",
    quadrant_priorities => "quadrant",
    requirement_traceability => "requirement",
    usecase_actors_and_boundary => "usecase",
    gitgraph_branch_and_merge => "gitgraph",
    c4_system_context => "c4-context",
    c4_containers => "c4-container",
    c4_components => "c4-component",
    c4_dynamic_interactions => "c4-dynamic",
    c4_deployment_nodes => "c4-deployment",
    mindmap_hierarchy => "mindmap",
    timeline_sections => "timeline",
    zenuml_messages => "zenuml",
    sankey_split_and_merge => "sankey",
    xychart_bar_and_line => "xychart",
    block_grid_and_connections => "block",
    packet_fields => "packet",
    kanban_columns_and_tasks => "kanban",
    architecture_group_and_services => "architecture",
    radar_comparison => "radar",
    event_modeling_commands_and_events => "event-modeling",
    treemap_hierarchy => "treemap",
    venn_intersection => "venn",
    ishikawa_causes => "ishikawa",
    wardley_dependencies => "wardley",
    cynefin_domains => "cynefin",
    treeview_hierarchy => "treeview",
    agentflow_decisions => "agentflow",
    railroad_native_dsl => "railroad",
    railroad_ebnf_dialect => "railroad-ebnf",
    railroad_abnf_dialect => "railroad-abnf",
    railroad_peg_dialect => "railroad-peg",
    info_utility => "info",
}

#[test]
fn catalog_covers_every_documented_family_and_registered_extra() {
    let catalog: Value = serde_json::from_str(CATALOG).unwrap();
    assert_eq!(catalog["mermaid_version"], "12.3.0");
    let fixtures = catalog["fixtures"].as_array().unwrap();
    assert_eq!(fixtures.len(), 44);
    let families: BTreeSet<_> = fixtures
        .iter()
        .map(|case| case["family"].as_str().unwrap())
        .collect();
    let expected = [
        "flowchart",
        "swimlane",
        "sequence",
        "class",
        "state",
        "er",
        "journey",
        "gantt",
        "pie",
        "quadrant",
        "requirement",
        "usecase",
        "gitgraph",
        "c4",
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
        "info",
    ]
    .into_iter()
    .collect();
    assert_eq!(families, expected);
    let names: BTreeSet<_> = fixtures
        .iter()
        .map(|case| case["slug"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), fixtures.len(), "duplicate fixture slug");
    let compiled_names: BTreeSet<_> = FIXTURE_NAMES.iter().copied().collect();
    assert_eq!(
        compiled_names, names,
        "catalog and compiled test cases differ"
    );
}

fn rejected_internal(source: &str) {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("README.md");
    let original = format!("# Invalid input\n\n```mermaid\n{source}\n```\n");
    fs::write(&path, &original).unwrap();
    let output = command(directory.path(), &["README.md"]);
    assert!(
        !output.status.success(),
        "internal sentinel was silently rendered"
    );
    assert_eq!(fs::read_to_string(path).unwrap(), original);
    assert!(!directory.path().join("docs/diagrams").exists());
}

#[test]
fn internal_error_sentinel_is_rejected_without_changing_markdown() {
    rejected_internal("error");
}

#[test]
fn malformed_frontmatter_sentinel_is_rejected_without_changing_markdown() {
    rejected_internal("---\nconfig:\n  layout: elk");
}
