use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag};
use tempfile::TempDir;

const FENCE: &str = "```mermaid\nflowchart LR\nA[Original] --> B[Finish]\n```\n";

fn fixture() -> TempDir {
    tempfile::tempdir().unwrap()
}

fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn run(root: &Path, args: &[&str]) -> Output {
    let binary = std::env::var_os("MER_MD_TEST_BINARY")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_mer-md").into());
    Command::new(binary)
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}

fn assert_exit(output: &Output, code: i32) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn read(root: &Path, relative: &str) -> String {
    fs::read_to_string(root.join(relative)).unwrap()
}

fn svg_paths(root: &Path) -> Vec<PathBuf> {
    let dir = root.join("docs/diagrams");
    if !dir.exists() {
        return Vec::new();
    }
    let mut paths: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "svg"))
        .collect();
    paths.sort();
    paths
}

fn svg_has_text(svg: &str, expected: &str) -> bool {
    let document = roxmltree::Document::parse(svg).unwrap();
    document.descendants().any(|node| {
        node.has_tag_name("text")
            && node
                .descendants()
                .filter(|child| child.is_text())
                .filter_map(|child| child.text())
                .collect::<String>()
                .contains(expected)
    })
}

fn identified_source(id: &str, label: &str) -> String {
    format!(
        "<!-- portable-mermaid:id {id} -->\n{}",
        FENCE.replace("Original", label)
    )
}

#[test]
fn case_different_ids_are_rejected_before_writing_a_batch() {
    for separate_files in [false, true] {
        let temp = fixture();
        let root = temp.path();
        let first = identified_source("diagram", "First");
        let second = identified_source("DIAGRAM", "Second");
        let original = if separate_files {
            write(root, "other.markdown", &second);
            first
        } else {
            format!("{first}\n{second}")
        };
        write(root, "README.md", &original);

        let output = run(root, &["."]);
        assert_exit(&output, 2);
        assert!(String::from_utf8_lossy(&output.stderr).contains("diagram ID"));
        assert_eq!(read(root, "README.md"), original);
        if separate_files {
            assert_eq!(read(root, "other.markdown"), second);
        }
        assert!(!root.join("docs").exists());
    }
}

#[test]
fn case_different_ids_do_not_alias_an_existing_svg() {
    let temp = fixture();
    let root = temp.path();
    write(root, "README.md", &identified_source("Diagram", "First"));
    assert_exit(&run(root, &["README.md"]), 0);
    let original_markdown = read(root, "README.md");
    let paths = svg_paths(root);
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0].file_name().unwrap(), "Diagram.svg");
    let original_svg = fs::read(&paths[0]).unwrap();

    let second = identified_source("diagram", "Second");
    write(root, "other.markdown", &second);
    let output = run(root, &["other.markdown"]);
    assert_exit(&output, 2);
    assert!(String::from_utf8_lossy(&output.stderr).contains("conflicts"));
    assert_eq!(read(root, "README.md"), original_markdown);
    assert_eq!(read(root, "other.markdown"), second);
    assert_eq!(svg_paths(root), paths);
    assert_eq!(fs::read(&paths[0]).unwrap(), original_svg);
    assert_exit(&run(root, &["README.md"]), 0);
    assert_eq!(fs::read(&paths[0]).unwrap(), original_svg);
}

#[test]
fn svg_ownership_checks_ids_without_filename_case_ambiguity() {
    let temp = fixture();
    let root = temp.path();
    write(root, "README.md", &identified_source("diagram", "First"));
    assert_exit(&run(root, &["README.md"]), 0);
    let paths = svg_paths(root);
    let original_svg = fs::read(&paths[0]).unwrap();
    let original = read(root, "README.md").replace("id=diagram", "id=DIAGRAM");
    write(root, "README.md", &original);
    let second = identified_source("diagram", "Second");
    write(root, "other.markdown", &second);

    let output = run(root, &["other.markdown"]);
    assert_exit(&output, 2);
    assert!(String::from_utf8_lossy(&output.stderr).contains("still belongs"));
    assert_eq!(read(root, "README.md"), original);
    assert_eq!(read(root, "other.markdown"), second);
    assert_eq!(svg_paths(root), paths);
    assert_eq!(fs::read(&paths[0]).unwrap(), original_svg);
}

#[test]
fn single_file_render_is_idempotent_and_restore_edit_reuses_the_svg() {
    let temp = fixture();
    let root = temp.path();
    let original = format!("# Diagram\n\n{FENCE}\nUnrelated prose.\n");
    write(root, "README.md", &original);
    write(root, "untouched.markdown", FENCE);

    assert_exit(&run(root, &["README.md"]), 0);
    assert_eq!(read(root, "untouched.markdown"), FENCE);
    let rendered = read(root, "README.md");
    assert!(rendered.contains("<p align=\"center\">"));
    assert!(rendered.contains("alt=\"Mermaid diagram\""));
    assert!(!rendered.contains("A[Original] -->"));
    assert!(rendered.starts_with("# Diagram\n\n"));
    assert!(rendered.ends_with("\nUnrelated prose.\n"));
    let paths = svg_paths(root);
    assert_eq!(paths.len(), 1);
    let first_svg = fs::read(&paths[0]).unwrap();

    let repeated = run(root, &["README.md"]);
    assert_exit(&repeated, 0);
    assert_eq!(read(root, "README.md"), rendered);
    assert_eq!(svg_paths(root), paths);
    assert_eq!(fs::read(&paths[0]).unwrap(), first_svg);
    assert!(String::from_utf8_lossy(&repeated.stdout).contains("1 cached"));

    assert_exit(&run(root, &["--restore", "README.md"]), 0);
    let restored = read(root, "README.md");
    assert!(restored.contains(FENCE));
    assert!(!restored.contains("alt=\"Mermaid diagram\""));
    let edited = restored.replace("A[Original]", "A[Changed]");
    write(root, "README.md", &edited);
    assert_exit(&run(root, &["README.md"]), 0);

    assert_eq!(
        svg_paths(root),
        paths,
        "editing must reuse the existing SVG path"
    );
    let changed_svg = fs::read_to_string(&paths[0]).unwrap();
    assert!(svg_has_text(&changed_svg, "Changed"));
    assert!(!svg_has_text(&changed_svg, "Original"));
    assert_ne!(changed_svg.as_bytes(), first_svg.as_slice());
}

#[test]
fn unknown_option_leaves_project_unchanged() {
    let temp = fixture();
    let root = temp.path();
    write(root, "README.md", FENCE);
    assert_exit(&run(root, &["README.md"]), 0);
    let markdown = read(root, "README.md");
    let paths = svg_paths(root);
    let svg = fs::read(&paths[0]).unwrap();

    assert_exit(&run(root, &["README.md", "--unknown"]), 2);
    assert_eq!(read(root, "README.md"), markdown);
    assert_eq!(svg_paths(root), paths);
    assert_eq!(fs::read(&paths[0]).unwrap(), svg);
}

#[test]
fn omitted_path_renders_and_restores_the_current_directory() {
    let temp = fixture();
    let root = temp.path();
    write(root, "README.md", FENCE);
    write(root, "guides/design.markdown", FENCE);

    let help = run(root, &["--help"]);
    assert_exit(&help, 0);
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(help.contains("mer-md"));
    assert!(help.contains("--restore"));
    assert_eq!(read(root, "README.md"), FENCE);
    assert!(!root.join("docs").exists());

    assert_exit(&run(root, &[]), 0);
    let paths = svg_paths(root);
    assert_eq!(paths.len(), 2);
    assert!(read(root, "README.md").contains("<p align=\"center\">"));
    assert!(read(root, "guides/design.markdown").contains("src=\"../docs/diagrams/"));

    assert_exit(&run(root, &["--restore"]), 0);
    assert!(read(root, "README.md").contains(FENCE));
    assert!(read(root, "guides/design.markdown").contains(FENCE));
    assert_eq!(svg_paths(root), paths);
}

#[test]
fn editing_readable_source_and_adding_raw_arrows_updates_the_same_svg_without_restore() {
    let temp = fixture();
    let root = temp.path();
    write(root, "README.md", FENCE);
    assert_exit(&run(root, &["README.md"]), 0);
    let paths = svg_paths(root);
    let rendered = read(root, "README.md");
    assert!(rendered.contains("<!-- portable-mermaid:source\n```mermaid\nflowchart LR\n"));
    assert!(rendered.contains("A[Original] --&gt; B[Finish]"));

    // Raw arrows close HTML comments; rerendering must still read the source.
    let edited = rendered.replace("A[Original]", "A[Changed]").replacen(
        "\n```\n",
        "\nB --> C[Added]\n```\n",
        1,
    );
    write(root, "README.md", &edited);
    assert_exit(&run(root, &["README.md"]), 0);
    assert_eq!(svg_paths(root), paths);
    let updated = read(root, "README.md");
    assert_eq!(updated.matches("alt=\"Mermaid diagram\"").count(), 1);
    assert!(updated.contains("A[Changed] --&gt; B[Finish]"));
    assert!(updated.contains("B --&gt; C[Added]"));
    assert!(!Parser::new(&updated).any(|event| matches!(
        event,
        Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info)))
            if info.eq_ignore_ascii_case("mermaid")
    )));
    let svg = fs::read_to_string(&paths[0]).unwrap();
    assert!(svg_has_text(&svg, "Changed"));
    assert!(svg_has_text(&svg, "Added"));
    assert_exit(&run(root, &["README.md"]), 0);
}

#[test]
fn readable_comments_preserve_ampersands_and_literal_entities_through_edit_and_restore() {
    let temp = fixture();
    let root = temp.path();
    let original = "```mermaid\nflowchart LR\nA[\"Fish & Chips &amp; More\"] --> B[Finish]\n```\n";
    write(root, "README.md", original);
    assert_exit(&run(root, &["README.md"]), 0);
    let paths = svg_paths(root);
    let rendered = read(root, "README.md");
    assert!(rendered.contains("Fish &amp; Chips &amp;amp; More"));
    let edited = rendered.replace("Chips", "Salad");
    write(root, "README.md", &edited);
    assert_exit(&run(root, &["README.md"]), 0);
    assert_eq!(svg_paths(root), paths);
    assert_exit(&run(root, &["--restore", "README.md"]), 0);
    assert!(read(root, "README.md").contains(&original.replace("Chips", "Salad")));
}

#[test]
fn directory_render_finds_both_extensions_recursively_with_relative_links() {
    let temp = fixture();
    let root = temp.path();
    for name in [
        "README.md",
        "guides/design.markdown",
        "guides/deep/usage.MARKDOWN",
    ] {
        write(root, name, FENCE);
    }
    write(root, "plain.txt", FENCE);

    assert_exit(&run(root, &["."]), 0);
    assert_eq!(svg_paths(root).len(), 3);
    assert!(read(root, "README.md").contains("src=\"docs/diagrams/"));
    assert!(read(root, "guides/design.markdown").contains("src=\"../docs/diagrams/"));
    assert!(read(root, "guides/deep/usage.MARKDOWN").contains("src=\"../../docs/diagrams/"));
    assert_eq!(read(root, "plain.txt"), FENCE);

    assert_exit(&run(root, &["--restore", "."]), 0);
    for name in [
        "README.md",
        "guides/design.markdown",
        "guides/deep/usage.MARKDOWN",
    ] {
        assert!(read(root, name).contains(FENCE));
    }
    assert_eq!(svg_paths(root).len(), 3, "restore retains reusable images");
}

#[test]
fn selected_directory_is_the_output_root_inside_a_git_repository() {
    let temp = fixture();
    let repository = temp.path();
    fs::create_dir(repository.join(".git")).unwrap();
    write(repository, "README.md", FENCE);
    write(repository, "project/README.md", FENCE);
    write(repository, "project/guides/design.markdown", FENCE);
    let project = repository.join("project");

    assert_exit(&run(&project, &["."]), 0);
    let paths = svg_paths(&project);
    assert_eq!(paths.len(), 2);
    assert!(svg_paths(repository).is_empty());
    assert_eq!(read(repository, "README.md"), FENCE);
    assert!(read(&project, "README.md").contains("src=\"docs/diagrams/"));
    assert!(read(&project, "guides/design.markdown").contains("src=\"../docs/diagrams/"));

    // Repeated rendering keeps the selected directory as the output root.
    assert_exit(&run(&project, &["."]), 0);
    assert_eq!(svg_paths(&project), paths);
    assert_exit(&run(&project, &["--restore", "."]), 0);
    assert!(read(&project, "README.md").contains(FENCE));
    assert!(read(&project, "guides/design.markdown").contains(FENCE));
    assert_exit(&run(&project, &["."]), 0);
    assert_eq!(svg_paths(&project), paths);
}

#[test]
fn relative_project_path_places_assets_in_the_target_project() {
    let temp = fixture();
    let parent = temp.path();
    fs::create_dir(parent.join(".git")).unwrap();
    write(parent, "README.md", FENCE);
    write(parent, "caller/README.md", FENCE);
    write(parent, "project café/README.md", FENCE);
    write(parent, "project café/guides/deep/usage.MARKDOWN", FENCE);
    let caller = parent.join("caller");
    let project = parent.join("project café");

    assert_exit(&run(&caller, &["../project café"]), 0);
    assert_eq!(svg_paths(&project).len(), 2);
    assert!(svg_paths(&caller).is_empty());
    assert!(svg_paths(parent).is_empty());
    assert_eq!(read(&caller, "README.md"), FENCE);
    assert_eq!(read(parent, "README.md"), FENCE);
    assert!(read(&project, "README.md").contains("src=\"docs/diagrams/"));
    assert!(read(&project, "guides/deep/usage.MARKDOWN").contains("src=\"../../docs/diagrams/"));
    assert_exit(&run(&caller, &["../project café"]), 0);
}

#[test]
fn selected_file_uses_its_parent_as_the_output_root() {
    let temp = fixture();
    let root = temp.path();
    fs::create_dir(root.join(".git")).unwrap();
    write(root, "README.md", FENCE);
    write(root, "guides/design.markdown", FENCE);
    write(root, "guides/untouched.md", FENCE);

    assert_exit(&run(root, &["guides/design.markdown"]), 0);
    assert_eq!(svg_paths(&root.join("guides")).len(), 1);
    assert!(svg_paths(root).is_empty());
    assert!(read(root, "guides/design.markdown").contains("src=\"docs/diagrams/"));
    assert_eq!(read(root, "README.md"), FENCE);
    assert_eq!(read(root, "guides/untouched.md"), FENCE);
    assert_exit(&run(root, &["guides/design.markdown"]), 0);
}

#[test]
fn automatic_palette_is_always_embedded() {
    let temp = fixture();
    let root = temp.path();
    write(root, "README.md", FENCE);

    assert_exit(&run(root, &["README.md"]), 0);
    let paths = svg_paths(root);
    let svg = fs::read_to_string(&paths[0]).unwrap();
    assert!(svg.contains("#EEE9FF"));
    assert!(svg.contains("#363052"));
    assert!(svg.contains("transparent"));
    assert!(svg.contains("prefers-color-scheme"));
}

#[test]
fn invalid_second_diagram_leaves_every_markdown_and_svg_unchanged() {
    let temp = fixture();
    let root = temp.path();
    let invalid_fence = "```mermaid\nthis is not a Mermaid diagram\n```\n";
    let original = format!("{FENCE}\n{invalid_fence}");
    write(root, "README.md", &original);
    assert_exit(&run(root, &["."]), 2);
    assert_eq!(read(root, "README.md"), original);
    assert!(
        !root.join("docs").exists(),
        "failed render must not create partial output"
    );

    // Also protect existing assets when an earlier diagram needs regeneration.
    write(root, "README.md", FENCE);
    assert_exit(&run(root, &["."]), 0);
    let paths = svg_paths(root);
    let previous_svg = fs::read(&paths[0]).unwrap();
    assert_exit(&run(root, &["--restore", "."]), 0);
    let attempted = format!(
        "{}\n{invalid_fence}",
        read(root, "README.md").replace("A[Original]", "A[Changed]")
    );
    write(root, "README.md", &attempted);

    assert_exit(&run(root, &["."]), 2);
    assert_eq!(read(root, "README.md"), attempted);
    assert_eq!(svg_paths(root), paths);
    assert_eq!(fs::read(&paths[0]).unwrap(), previous_svg);
}

#[test]
fn selecting_only_a_copied_rendered_document_does_not_overwrite_the_original_svg() {
    let temp = fixture();
    let root = temp.path();
    write(root, "README.md", FENCE);
    assert_exit(&run(root, &["README.md"]), 0);
    let rendered = read(root, "README.md");
    let paths = svg_paths(root);
    let original_svg = fs::read(&paths[0]).unwrap();
    write(root, "copy.markdown", &rendered);

    // The original is outside the selected file, so ownership must be checked
    // against the existing asset, rather than only within the current batch.
    assert_exit(&run(root, &["copy.markdown"]), 2);
    assert_eq!(read(root, "README.md"), rendered);
    assert_eq!(read(root, "copy.markdown"), rendered);
    assert_eq!(svg_paths(root), paths);
    assert_eq!(fs::read(&paths[0]).unwrap(), original_svg);
}

#[test]
fn renaming_a_rendered_document_reuses_its_existing_diagram_identity() {
    let temp = fixture();
    let root = temp.path();
    write(root, "README.md", FENCE);
    assert_exit(&run(root, &["README.md"]), 0);
    let paths = svg_paths(root);
    fs::rename(root.join("README.md"), root.join("renamed.markdown")).unwrap();

    assert_exit(&run(root, &["renamed.markdown"]), 0);
    assert_eq!(svg_paths(root), paths);
    assert!(!root.join("README.md").exists());
    assert!(read(root, "renamed.markdown").contains("alt=\"Mermaid diagram\""));
    assert!(svg_has_text(
        &fs::read_to_string(&paths[0]).unwrap(),
        "Original"
    ));
    assert_exit(&run(root, &["renamed.markdown"]), 0);
}

#[test]
fn cache_ownership_preserves_filename_case_through_source_edits() {
    let temp = fixture();
    let root = temp.path();
    write(root, "Guides/README.md", FENCE);
    assert_exit(&run(root, &["."]), 0);
    let paths = svg_paths(root);
    let svg = fs::read_to_string(&paths[0]).unwrap();
    let owner = STANDARD.encode("Guides/README.md");
    assert!(
        svg.lines()
            .next()
            .unwrap()
            .contains(&format!("owner={owner}"))
    );

    let rendered = read(root, "Guides/README.md");
    let edited = rendered.replace("Original", "Changed");
    write(root, "Guides/README.md", &edited);
    assert_exit(&run(root, &["."]), 0);
    assert_eq!(svg_paths(root), paths);
    let updated_svg = fs::read_to_string(&paths[0]).unwrap();
    assert!(
        updated_svg
            .lines()
            .next()
            .unwrap()
            .contains(&format!("owner={owner}"))
    );
    assert!(svg_has_text(&updated_svg, "Changed"));
}

#[test]
fn case_only_document_rename_reuses_its_svg() {
    let temp = fixture();
    let root = temp.path();
    write(root, "README.md", FENCE);
    assert_exit(&run(root, &["README.md"]), 0);
    let paths = svg_paths(root);
    let rendered = read(root, "README.md");
    fs::rename(root.join("README.md"), root.join("readme.md")).unwrap();

    assert_exit(&run(root, &["readme.md"]), 0);
    assert_eq!(svg_paths(root), paths);
    assert_eq!(read(root, "readme.md"), rendered);
    assert!(
        fs::read_to_string(&paths[0])
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .contains(&format!("owner={}", STANDARD.encode("readme.md")))
    );
}

#[test]
fn a_truncated_svg_with_its_cache_metadata_is_regenerated() {
    let temp = fixture();
    let root = temp.path();
    write(root, "README.md", FENCE);
    assert_exit(&run(root, &["README.md"]), 0);
    let markdown = read(root, "README.md");
    let paths = svg_paths(root);
    let complete_svg = fs::read_to_string(&paths[0]).unwrap();
    let root_start = complete_svg.find("<svg").unwrap();
    // Keep every ownership/cache comment, while simulating an interrupted SVG
    // body write. Matching source fingerprints must not make this a cache hit.
    fs::write(&paths[0], &complete_svg[..root_start]).unwrap();

    assert_exit(&run(root, &["README.md"]), 0);
    assert_eq!(read(root, "README.md"), markdown);
    assert_eq!(svg_paths(root), paths);
    let repaired = fs::read_to_string(&paths[0]).unwrap();
    assert!(repaired.contains("<svg"));
    assert!(repaired.trim_end().ends_with("</svg>"));
    assert!(svg_has_text(&repaired, "Original"));
    assert_exit(&run(root, &["README.md"]), 0);
}

#[test]
fn restore_does_not_require_a_usable_diagram_output_directory() {
    let temp = fixture();
    let root = temp.path();
    write(root, "README.md", FENCE);
    assert_exit(&run(root, &["README.md"]), 0);

    // Both paths are immediate children of this isolated fixture directory.
    // Restore only reads the Markdown's embedded source; assets are irrelevant.
    fs::rename(root.join("docs"), root.join("saved-output")).unwrap();
    fs::write(root.join("docs"), "This path is now a regular file.\n").unwrap();
    assert_exit(&run(root, &["--restore", "README.md"]), 0);
    assert!(read(root, "README.md").contains(FENCE));
    assert!(!read(root, "README.md").contains("alt=\"Mermaid diagram\""));
    assert_eq!(read(root, "docs"), "This path is now a regular file.\n");
    assert!(root.join("saved-output/diagrams").is_dir());
}
