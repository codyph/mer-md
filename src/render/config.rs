//! Family colour defaults overridden by Mermaid source configuration.

use crate::render::palette::Theme;
use serde_json::{Map, Value, json};

/// Merge `themeVariables` with the common palette; replacing it discards paints.
pub fn family_options(theme: &Theme) -> Map<String, Value> {
    let mut options = json!({
        "packet": {
            "startByteColor": theme.text_color,
            "endByteColor": theme.text_color,
            "labelColor": theme.primary_text_color,
            "titleColor": theme.text_color,
            "blockStrokeColor": theme.primary_border_color,
            "blockFillColor": theme.primary_color
        },
        "c4": {
            "boundary_bg_color": "none",
            "boundary_border_color": theme.cluster_border,
            "boundary_font_color": theme.text_color,
            "relationship_text_color": theme.text_color,
            "relationship_line_color": theme.line_color
        },
        "themeVariables": {
            "commitLabelColor": theme.git_commit_label_color,
            "commitLabelBackground": "transparent",
            "tagLabelColor": theme.git_tag_label_color,
            "tagLabelBackground": "transparent",
            "tagLabelBorder": "transparent",
            "treeView": {
                "labelColor": theme.text_color,
                "lineColor": theme.line_color,
                "iconColor": theme.primary_border_color,
                "descriptionColor": theme.text_color,
                "highlightBg": theme.sequence_note_fill,
                "highlightStroke": theme.sequence_note_border
            },
            "zenuml": {
                "backgroundColor": "transparent",
                "borderColor": theme.primary_border_color,
                "titleTextColor": theme.text_color,
                "textColor": theme.text_color,
                "messageTextColor": theme.text_color,
                "lineColor": theme.line_color,
                "messageLineColor": theme.line_color,
                "lifelineColor": theme.line_color,
                "sequenceNumberTextColor": theme.text_color,
                "fragmentBorderColor": theme.cluster_border,
                "actorBackgroundColor": theme.sequence_actor_fill,
                "actorBorderColor": theme.sequence_actor_border,
                "actorTextColor": theme.text_color,
                "noteBackgroundColor": theme.sequence_note_fill,
                "noteBorderColor": theme.sequence_note_border,
                "noteTextColor": theme.text_color,
                "groupBackgroundColor": theme.cluster_background,
                "groupBorderColor": theme.cluster_border,
                "groupTextColor": theme.text_color,
                "activationBackgroundColor": theme.sequence_activation_fill,
                "activationBorderColor": theme.sequence_activation_border,
                "mutedSurfaceColor": theme.tertiary_color,
                "fragmentTextColor": theme.text_color,
                "subtleTextColor": theme.text_color
            },
            "emUiFill": theme.secondary_color,
            "emUiStroke": theme.primary_border_color,
            "emProcessorFill": theme.primary_color,
            "emProcessorStroke": theme.primary_border_color,
            "emReadModelFill": theme.tertiary_color,
            "emReadModelStroke": theme.cluster_border,
            "emCommandFill": theme.secondary_color,
            "emCommandStroke": theme.primary_border_color,
            "emEventFill": theme.sequence_note_fill,
            "emEventStroke": theme.sequence_note_border,
            "emSwimlaneBackgroundOdd": "transparent",
            "emSwimlaneBackground": "transparent",
            "emSwimlaneBackgroundStroke": theme.cluster_border,
            "emRelationStroke": theme.line_color,
            "emArrowhead": theme.line_color
        }
    })
    .as_object()
    .expect("family defaults form an object")
    .clone();

    // C4 shape paints use a separate configuration namespace.
    let c4 = options.get_mut("c4").unwrap().as_object_mut().unwrap();
    for kind in [
        "person",
        "system",
        "system_db",
        "system_queue",
        "container",
        "container_db",
        "container_queue",
        "component",
        "component_db",
        "component_queue",
    ] {
        let fill = if kind.starts_with("system") {
            &theme.secondary_color
        } else if kind.starts_with("container") {
            &theme.tertiary_color
        } else {
            &theme.primary_color
        };
        for (prefix, fill) in [("", fill), ("external_", &theme.cluster_background)] {
            c4.insert(format!("{prefix}{kind}_bg_color"), json!(fill));
            c4.insert(
                format!("{prefix}{kind}_border_color"),
                json!(theme.primary_border_color),
            );
            c4.insert(
                format!("{prefix}{kind}_font_color"),
                json!(theme.text_color),
            );
        }
    }
    options
}

#[cfg(test)]
mod tests {
    #[test]
    fn family_paints_adapt_without_changing_native_geometry() {
        for (name, source) in [
            (
                "packet",
                include_str!("../../tests/fixtures/diagrams/packet.mmd"),
            ),
            (
                "treeview",
                include_str!("../../tests/fixtures/diagrams/treeview.mmd"),
            ),
            (
                "zenuml",
                include_str!("../../tests/fixtures/diagrams/zenuml.mmd"),
            ),
            (
                "c4",
                include_str!("../../tests/fixtures/diagrams/c4-context.mmd"),
            ),
            (
                "eventmodeling",
                include_str!("../../tests/fixtures/diagrams/event-modeling.mmd"),
            ),
            (
                "gitgraph",
                include_str!("../../tests/fixtures/diagrams/gitgraph.mmd"),
            ),
            (
                "kanban",
                include_str!("../../tests/fixtures/diagrams/kanban.mmd"),
            ),
        ] {
            let light =
                crate::render::graph::render(source, &crate::render::palette::light_theme())
                    .unwrap_or_else(|error| panic!("{name} light: {error:#}"));
            let dark = crate::render::graph::render(source, &crate::render::palette::dark_theme())
                .unwrap_or_else(|error| panic!("{name} dark: {error:#}"));
            let adaptive = crate::render::theme::adaptive(&light, &dark)
                .unwrap_or_else(|error| panic!("{name} palette geometry differs: {error:#}"));
            assert!(
                adaptive.contains("prefers-color-scheme:dark"),
                "{name} did not adapt"
            );
            assert!(dark.contains("#E7EDF7"), "{name} kept a fixed text default");
        }
    }

    #[test]
    fn source_family_colours_override_site_defaults_in_both_palettes() {
        for (settings, source, paint) in [
            (
                r##"{"packet":{"labelColor":"#123456"}}"##,
                include_str!("../../tests/fixtures/diagrams/packet.mmd"),
                "#123456",
            ),
            (
                r##"{"themeVariables":{"treeView":{"labelColor":"#234567"}}}"##,
                include_str!("../../tests/fixtures/diagrams/treeview.mmd"),
                "#234567",
            ),
            (
                r##"{"themeVariables":{"zenuml":{"actorTextColor":"#345678"}}}"##,
                include_str!("../../tests/fixtures/diagrams/zenuml.mmd"),
                "#345678",
            ),
            (
                r##"{"c4":{"relationship_text_color":"#456789"}}"##,
                include_str!("../../tests/fixtures/diagrams/c4-context.mmd"),
                "#456789",
            ),
        ] {
            let source = format!("---\nconfig: {settings}\n---\n{source}");
            let light =
                crate::render::graph::render(&source, &crate::render::palette::light_theme())
                    .unwrap();
            let dark = crate::render::graph::render(&source, &crate::render::palette::dark_theme())
                .unwrap();
            let adaptive = crate::render::theme::adaptive(&light, &dark).unwrap();
            assert!(
                light.contains(paint) && dark.contains(paint) && adaptive.contains(paint),
                "source override {settings} did not survive all palettes"
            );
            let palette_entry =
                regex::Regex::new(&format!(r"--pm-paint-\d+:{paint}[;}}]")).unwrap();
            assert!(
                !palette_entry.is_match(&adaptive),
                "authored paint became a theme variable"
            );
        }
    }

    #[test]
    fn git_commit_and_tag_labels_have_no_painted_backgrounds() {
        let source = include_str!("../../tests/fixtures/diagrams/gitgraph.mmd");
        for theme in [
            crate::render::palette::light_theme(),
            crate::render::palette::dark_theme(),
        ] {
            let svg = crate::render::graph::render(source, &theme).unwrap();
            for class in ["commit-label-bkg", "tag-label-bkg"] {
                let marker = format!(".{class}{{");
                let rules = svg
                    .split(&marker)
                    .nth(1)
                    .unwrap()
                    .split('}')
                    .next()
                    .unwrap();
                assert!(rules.contains("fill:transparent"), "{class}: {rules}");
            }
        }
    }
}
