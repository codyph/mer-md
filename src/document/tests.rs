use super::*;

fn render(markdown: &str) -> String {
    let diagrams = discover(markdown, "README.md").unwrap();
    let links = diagrams
        .iter()
        .map(|diagram| format!("docs/diagrams/{}.svg", diagram.id))
        .collect::<Vec<_>>();
    render_document(markdown, &diagrams, &links).unwrap()
}

#[test]
fn arrows_and_html_labels_survive_render_restore() {
    let fence = "```mermaid\nflowchart LR\nA[\"<b>start</b>\"] --> B\n```\n";
    let input = format!("# Before\n\n{fence}\nAfter.\n");
    let rendered = render(&input);
    assert!(rendered.contains("A[\"<b>start</b>\"] --&gt; B"));
    assert!(!Parser::new(&rendered).any(|event| matches!(event, Event::Start(Tag::CodeBlock(_)))));
    let diagrams = discover(&rendered, "README.md").unwrap();
    assert_eq!(diagrams.len(), 1);
    assert_eq!(diagrams[0].original_fence, fence);
    assert!(diagrams[0].source.contains("-->"));
    let restored = restore_document(&rendered, &diagrams).unwrap();
    assert!(restored.starts_with("# Before\n\n<!-- portable-mermaid:id "));
    assert!(restored.ends_with(&format!("{fence}\nAfter.\n")));
}

#[test]
fn rerender_is_byte_for_byte_idempotent() {
    let rendered = render("```mermaid\nflowchart LR\nA --> B\n```\n");
    assert_eq!(rendered, render(&rendered));
}

#[test]
fn centered_image_uses_html_without_changing_source_or_identity() {
    let fence = "```mermaid\nflowchart LR\nA --> B\n```\n";
    let original = discover(fence, "README.md").unwrap();
    let rendered = render(fence);
    assert!(rendered.contains(&format!(
            "<p align=\"center\">\n  <img src=\"docs/diagrams/{}.svg\" alt=\"Mermaid diagram\" />\n</p>",
            original[0].id
        )));
    let diagrams = discover(&rendered, "README.md").unwrap();
    assert_eq!(diagrams[0].id, original[0].id);
    assert_eq!(diagrams[0].original_fence, fence);
    assert!(
        restore_document(&rendered, &diagrams)
            .unwrap()
            .ends_with(fence)
    );
}

#[test]
fn centered_image_links_encode_attribute_and_url_characters() {
    let input = "```mermaid\nflowchart LR\nA --> B\n```\n";
    let diagrams = discover(input, "README.md").unwrap();
    let link = "../diagram images/a\"<&'#50%(café).svg";
    let rendered = render_document(input, &diagrams, &[link.to_owned()]).unwrap();
    assert!(
        rendered.contains("src=\"../diagram%20images/a%22%3C%26%27%2350%25%28caf%C3%A9%29.svg\"")
    );
    assert_eq!(discover(&rendered, "README.md").unwrap().len(), 1);
}

#[test]
fn centered_images_refuse_extra_user_content_or_modified_structure() {
    let rendered = render("```mermaid\nflowchart LR\nA --> B\n```\n");
    for (before, after) in [
        ("</p>", "Keep this note.</p>"),
        ("</p>", "<!-- Keep this comment. --></p>"),
        (
            "</p>",
            "<img src=\"other.svg\" alt=\"Mermaid diagram\" /></p>",
        ),
        (
            "<p align=\"center\">",
            "<p align=\"center\" class=\"custom\">",
        ),
        (
            "<p align=\"center\">",
            "<p align=\"center\" xmlns:x=\"urn:test\">",
        ),
        ("alt=\"Mermaid diagram\"", "alt=\"Custom caption\""),
        (
            "alt=\"Mermaid diagram\"",
            "alt=\"Mermaid diagram\" width=\"250\"",
        ),
        (" />", ">Keep this caption.</img>"),
        ("</p>", "</p>\nKeep this note."),
        ("<p align=\"center\">", "<?keep this?><p align=\"center\">"),
    ] {
        let changed = rendered.replace(before, after);
        assert!(discover(&changed, "README.md").is_err(), "accepted {after}");
    }
}

#[test]
fn restore_retains_identity_across_source_edits() {
    let rendered = render("```mermaid\nflowchart LR\nA --> B\n```\n");
    let before = discover(&rendered, "README.md").unwrap();
    let restored = restore_document(&rendered, &before).unwrap();
    let edited = restored.replace("A --> B", "A --> B --> C");
    let after = discover(&edited, "renamed.md").unwrap();
    assert_eq!(before[0].id, after[0].id);
    assert!(after[0].source.contains("B --> C"));
    assert_eq!(discover(&render(&edited), "README.md").unwrap().len(), 1);
}

#[test]
fn preserves_crlf_tilde_fence_indentation_and_missing_final_newline() {
    let fence = "  ~~~~mermaid title\r\n  graph TD\r\n  A --> B\r\n  ~~~~";
    let input = format!("# Heading\r\n\r\n{fence}");
    let diagrams = discover(&input, "README.md").unwrap();
    assert_eq!(diagrams[0].original_fence, fence);
    let rendered = render(&input);
    assert!(!rendered.ends_with('\n'));
    let managed = discover(&rendered, "README.md").unwrap();
    let restored = restore_document(&rendered, &managed).unwrap();
    assert!(restored.ends_with(fence));
    assert!(!restored.replace("\r\n", "").contains('\n'));
}

#[test]
fn code_examples_and_other_languages_are_ignored() {
    let input = "````markdown\n```mermaid\nA --> B\n```\n<!-- portable-mermaid:begin id=fake -->\n````\n\n```rust\nlet mermaid = 1;\n```\n\n```MERMAID\ngraph TD\nA --> B\n```\n";
    let diagrams = discover(input, "README.md").unwrap();
    assert_eq!(diagrams.len(), 1);
    assert!(diagrams[0].source.contains("graph TD"));
}

#[test]
fn two_fences_have_independent_ids_and_preserve_prose() {
    let input = "前言\n\n```mermaid\ngraph LR\nA-->B\n```\n\nMiddle text.\n\n~~~mermaid\nsequenceDiagram\nA->>B: Hi\n~~~\n\nEnd.\n";
    let diagrams = discover(input, "notes/guide.markdown").unwrap();
    assert_eq!(diagrams.len(), 2);
    assert_ne!(diagrams[0].id, diagrams[1].id);
    let rendered = render(input);
    assert!(rendered.starts_with("前言\n\n"));
    assert!(rendered.contains("\n\nMiddle text.\n\n"));
    assert!(rendered.ends_with("\n\nEnd.\n"));
}

#[test]
fn malformed_and_duplicate_owned_regions_fail_safely() {
    assert!(discover("<!-- portable-mermaid:begin id=x -->\n", "README.md").is_err());
    assert!(discover("<!-- portable-mermaid:end -->\n", "README.md").is_err());
    assert!(discover("<!-- portable-mermaid:id x -->\ntext\n", "README.md").is_err());
    let rendered = render("```mermaid\ngraph TD\nA-->B\n```\n");
    assert!(discover(&format!("{rendered}\n{rendered}"), "README.md").is_err());
    let corrupted = rendered.replace(SOURCE, &format!("{SOURCE} unexpected"));
    assert!(discover(&corrupted, "README.md").is_err());
}

#[test]
fn nested_mermaid_fences_are_explicitly_rejected() {
    for input in [
        "> ```mermaid\n> graph LR\n> A-->B\n> ```\n",
        "- Example\n\n  ```mermaid\n  graph LR\n  A-->B\n  ```\n",
    ] {
        assert!(
            discover(input, "README.md")
                .unwrap_err()
                .to_string()
                .contains("lists or blockquotes")
        );
    }
}

#[test]
fn restore_on_editable_markdown_is_a_noop() {
    let input = "```mermaid\ngraph LR\nA-->B\n```\n";
    let diagrams = discover(input, "README.md").unwrap();
    assert_eq!(restore_document(input, &diagrams).unwrap(), input);
}

#[test]
fn trailing_fence_whitespace_round_trips_without_eating_blank_lines() {
    for (delimiter, newline) in [("```", "\n"), ("~~~~", "\r\n")] {
        for whitespace in ["   ", "\t", " \t \t"] {
            let fence = format!(
                "{delimiter}mermaid{newline}graph LR{newline}A-->B{newline}{delimiter}{whitespace}{newline}"
            );
            let input = format!("{fence}{newline}{newline}After.{newline}");
            let diagrams = discover(&input, "README.md").unwrap_or_else(|error| {
                panic!(
                    "{delimiter:?} {newline:?} {whitespace:?}: {error:#}; {:?}",
                    parse_markdown(&input)
                )
            });
            assert_eq!(diagrams[0].original_fence, fence);
            let rendered = render(&input);
            assert_eq!(rendered, render(&rendered));
            let restored =
                restore_document(&rendered, &discover(&rendered, "README.md").unwrap()).unwrap();
            assert!(restored.ends_with(&input));
        }
    }
}

#[test]
fn tab_ended_fences_preserve_body_tabs_and_surrounding_code_examples() {
    let example = "````markdown\n```mermaid\nIgnored example.\n```\t\n````\t\n";
    let first = "```mermaid\nflowchart LR\nA[\"tab\tlabel\"] --> B\n```\t\n";
    let second = "~~~~mermaid\nsequenceDiagram\nA->>B: Hello\tthere\n~~~~ \t\n";
    let input = format!("{example}\nBefore.\n\n{first}\nBetween.\n\n{second}\nAfter.\n");
    let diagrams = discover(&input, "README.md").unwrap();
    assert_eq!(diagrams.len(), 2);
    assert_eq!(diagrams[0].original_fence, first);
    assert_eq!(diagrams[1].original_fence, second);
    assert!(diagrams[0].source.contains("tab\tlabel"));
    assert!(diagrams[1].source.contains("Hello\tthere"));
    let rendered = render(&input);
    assert!(rendered.starts_with(example));
    assert_eq!(rendered, render(&rendered));
    let managed = discover(&rendered, "README.md").unwrap();
    let mut restored = restore_document(&rendered, &managed).unwrap();
    for diagram in &managed {
        restored = restored.replace(&format!("{ID}{} -->\n", diagram.id), "");
    }
    assert_eq!(restored, input);
}

#[test]
fn utf8_bom_before_first_fence_survives_render_edit_and_restore() {
    for newline in ["\n", "\r\n"] {
        for final_newline in ["", newline] {
            let fence = format!(
                "```mermaid{newline}flowchart LR{newline}A --> B{newline}```{final_newline}"
            );
            let input = format!("\u{feff}{fence}");
            let diagrams = discover(&input, "README.md").unwrap();
            assert_eq!(diagrams.len(), 1);
            assert_eq!(diagrams[0].span.start, '\u{feff}'.len_utf8());
            assert_eq!(diagrams[0].original_fence, fence);
            let id = &diagrams[0].id;
            let rendered = render(&input);
            assert!(rendered.starts_with(&format!("\u{feff}{BEGIN}{id} -->")));
            assert_eq!(rendered, render(&rendered));

            let edited = rendered.replace("A --&gt; B", "A --> C");
            let managed = discover(&edited, "README.md").unwrap();
            assert_eq!(managed.len(), 1);
            assert_eq!(&managed[0].id, id);
            assert!(managed[0].source.contains("A --> C"));
            let updated = render(&edited);
            assert_eq!(updated, render(&updated));
            let restored =
                restore_document(&updated, &discover(&updated, "README.md").unwrap()).unwrap();
            let identity = format!("{ID}{id} -->{newline}");
            assert_eq!(
                restored.replacen(&identity, "", 1),
                input.replace("A --> B", "A --> C")
            );
        }
    }
}

#[test]
fn utf8_bom_and_heading_are_preserved_around_diagrams() {
    let input = "\u{feff}# Heading\n\n```mermaid\nflowchart LR\nA --> B\n```\n\nAfter.\n";
    let rendered = render(input);
    assert!(rendered.starts_with("\u{feff}# Heading\n\n"));
    assert!(rendered.ends_with("\n\nAfter.\n"));
    let diagrams = discover(&rendered, "README.md").unwrap();
    let restored = restore_document(&rendered, &diagrams).unwrap();
    let identity = format!("{ID}{} -->\n", diagrams[0].id);
    assert_eq!(restored.replacen(&identity, "", 1), input);
    let plain = "\u{feff}# Heading\n";
    assert_eq!(render(plain), plain);
}

#[test]
fn unexpected_prose_inside_owned_region_is_not_discarded() {
    let rendered = render("```mermaid\ngraph LR\nA-->B\n```\n");
    let changed = rendered.replace(SOURCE, &format!("Keep this note.\n{SOURCE}"));
    assert!(discover(&changed, "README.md").is_err());
}

#[test]
fn inserting_new_fence_at_original_offset_does_not_reuse_persistent_id() {
    let first = "```mermaid\ngraph LR\nA-->B\n```\n";
    let rendered = render(first);
    let persistent = discover(&rendered, "README.md").unwrap()[0].id.clone();
    let inserted = format!("{first}\n{rendered}");
    let diagrams = discover(&inserted, "README.md").unwrap();
    assert_eq!(diagrams.len(), 2);
    assert_ne!(diagrams[0].id, persistent);
    assert_eq!(diagrams[1].id, persistent);

    let editable = restore_document(&rendered, &discover(&rendered, "README.md").unwrap()).unwrap();
    let inserted = format!("{first}\n{editable}");
    let diagrams = discover(&inserted, "README.md").unwrap();
    assert_ne!(diagrams[0].id, persistent);
    assert_eq!(diagrams[1].id, persistent);
}

#[test]
fn unterminated_mermaid_fence_is_refused_instead_of_consuming_later_prose() {
    for ending in ["\nLater prose.\n", "    ```\n", "    ```\t\n"] {
        let input = format!("```mermaid\ngraph LR\nA-->B\n{ending}");
        assert!(discover(&input, "README.md").is_err(), "{ending:?}");
    }
}

#[test]
fn source_can_be_edited_in_place_including_pasted_raw_arrows() {
    let rendered = render("```mermaid\nflowchart LR\nA[Before] --> B\n```\n");
    let id = discover(&rendered, "README.md").unwrap()[0].id.clone();
    let edited = rendered.replace("A[Before] --&gt; B", "A[After] --> B\nB --> C");
    let diagrams = discover(&edited, "README.md").unwrap();
    assert_eq!(diagrams.len(), 1);
    assert_eq!(diagrams[0].id, id);
    assert!(diagrams[0].source.contains("A[After] --> B\nB --> C"));
    let updated = render(&edited);
    assert!(updated.contains("A[After] --&gt; B\nB --&gt; C"));
    assert_eq!(updated, render(&updated));
}

#[test]
fn raw_arrow_edits_do_not_hide_later_diagrams_or_prose_from_discovery() {
    let original = "```mermaid\nflowchart LR\nA --> B\n```\n\nBetween.\n\n```mermaid\nsequenceDiagram\nA->>B: Hello\n```\n\nAfter.\n";
    let rendered = render(original);
    let edited = rendered.replace("A --&gt; B", "A --> B --> C");
    let with_new_diagram = format!("{edited}\n```mermaid\nflowchart LR\nX --> Y\n```\n");
    let diagrams = discover(&with_new_diagram, "README.md").unwrap();
    assert_eq!(diagrams.len(), 3);
    assert!(diagrams[0].source.contains("B --> C"));
    assert!(diagrams[1].source.contains("sequenceDiagram"));
    assert!(diagrams[2].source.contains("X --> Y"));
    let updated = render(&with_new_diagram);
    assert!(updated.contains("\n\nBetween.\n\n"));
    assert!(updated.contains("\n\nAfter.\n"));
}

#[test]
fn comment_delimiters_and_literal_entities_round_trip_exactly() {
    let fence = "```mermaid\nflowchart LR\nA[\"<!-- note --> --!> &amp; &lt;!-- --&gt; Fish & Chips\"] --> B\n```\n";
    let rendered = render(fence);
    assert!(
        rendered.contains(
            "&lt;!-- note --&gt; --!&gt; &amp;amp; &amp;lt;!-- --&amp;gt; Fish &amp; Chips"
        )
    );
    let diagram = discover(&rendered, "README.md").unwrap().remove(0);
    assert_eq!(diagram.original_fence, fence);
    assert_eq!(rendered, render(&rendered));
    let visible_text = Parser::new(&rendered)
        .filter_map(|event| {
            if let Event::Text(text) = event {
                Some(text.to_string())
            } else {
                None
            }
        })
        .collect::<String>();
    assert!(!visible_text.contains("Fish"));
}

#[test]
fn owned_source_markers_in_markdown_code_examples_are_ignored() {
    let input = format!(
        "````markdown\n{BEGIN}example -->\n![Diagram](diagram.svg)\n{SOURCE}\n```mermaid\nA --> B\n```\n-->\n{END}\n````\n\n```mermaid\nflowchart LR\nX --> Y\n```\n"
    );
    let diagrams = discover(&input, "README.md").unwrap();
    assert_eq!(diagrams.len(), 1);
    assert!(diagrams[0].source.contains("X --> Y"));
}

#[test]
fn incomplete_readable_source_is_refused_before_replacing_any_prose() {
    let rendered = render("```mermaid\nflowchart LR\nA --> B\n```\n");
    let malformed = rendered.replace("\n```\n-->\n", "\nLater prose.\n-->\n");
    assert!(discover(&malformed, "README.md").is_err());
}

#[test]
fn incomplete_source_cannot_absorb_a_later_managed_diagram() {
    let input = "```mermaid\nflowchart LR\nA --> B\n```\n\nBetween.\n\n```mermaid\nflowchart LR\nX --> Y\n```\n";
    let rendered = render(input);
    let broken = rendered.replacen("\n```\n-->\n", "\n-->\n", 1);
    assert!(discover(&broken, "README.md").is_err());
}
