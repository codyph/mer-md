//! Light and dark palettes with opaque shapes and transparent label backgrounds.

#[derive(Debug, Clone)]
pub struct Theme {
    pub font_family: String,
    pub font_size: f32,
    pub primary_color: String,
    pub primary_text_color: String,
    pub primary_border_color: String,
    pub line_color: String,
    pub secondary_color: String,
    pub tertiary_color: String,
    pub cluster_background: String,
    pub cluster_border: String,
    pub sequence_actor_fill: String,
    pub sequence_actor_border: String,
    pub sequence_actor_line: String,
    pub sequence_note_fill: String,
    pub sequence_note_border: String,
    pub sequence_activation_fill: String,
    pub sequence_activation_border: String,
    pub text_color: String,
    pub git_colors: [String; 8],
    pub git_inv_colors: [String; 8],
    pub git_branch_label_colors: [String; 8],
    pub git_commit_label_color: String,
    pub git_tag_label_color: String,
    pub git_tag_label_border: String,
    pub pie_colors: [String; 12],
    pub pie_title_text_color: String,
    pub pie_section_text_color: String,
    pub pie_legend_text_color: String,
}

const FONT_FAMILY: &str =
    "system-ui, -apple-system, Segoe UI, DejaVu Sans, Liberation Sans, sans-serif";

pub fn light_theme() -> Theme {
    Theme {
        font_family: FONT_FAMILY.to_owned(),
        font_size: 16.0,
        primary_color: "#EEE9FF".to_owned(),
        primary_text_color: "#263347".to_owned(),
        primary_border_color: "#8B6DD6".to_owned(),
        line_color: "#566176".to_owned(),
        secondary_color: "#E0F2FE".to_owned(),
        tertiary_color: "#DCF4EB".to_owned(),
        cluster_background: "#F5F3FB".to_owned(),
        cluster_border: "#8E7AA9".to_owned(),
        sequence_actor_fill: "#EEE9FF".to_owned(),
        sequence_actor_border: "#8B6DD6".to_owned(),
        sequence_actor_line: "#566176".to_owned(),
        sequence_note_fill: "#FFF0CC".to_owned(),
        sequence_note_border: "#A87226".to_owned(),
        sequence_activation_fill: "#E0F2FE".to_owned(),
        sequence_activation_border: "#5484B4".to_owned(),
        text_color: "#263347".to_owned(),
        git_colors: [
            "#7654B3", "#276D9E", "#287866", "#8D6426", "#9C4D6C", "#5366AB", "#586F31", "#A0583F",
        ]
        .map(str::to_owned),
        git_inv_colors: ["#FFFFFF"; 8].map(str::to_owned),
        git_branch_label_colors: ["#FFFFFF"; 8].map(str::to_owned),
        git_commit_label_color: "#263347".to_owned(),
        git_tag_label_color: "#263347".to_owned(),
        git_tag_label_border: "#8B6DD6".to_owned(),
        pie_colors: [
            "#C5B6F1", "#9DCBEF", "#91D2C1", "#F2C780", "#E8ACBD", "#B8C6F2", "#B8D793", "#EAAE92",
            "#A5D9DC", "#D7BAE8", "#E5D28D", "#ADC6D0",
        ]
        .map(str::to_owned),
        pie_title_text_color: "#263347".to_owned(),
        pie_section_text_color: "#263347".to_owned(),
        pie_legend_text_color: "#263347".to_owned(),
    }
}

pub fn dark_theme() -> Theme {
    Theme {
        font_family: FONT_FAMILY.to_owned(),
        font_size: 16.0,
        primary_color: "#363052".to_owned(),
        primary_text_color: "#E7EDF7".to_owned(),
        primary_border_color: "#AD95EB".to_owned(),
        line_color: "#A9B7CF".to_owned(),
        secondary_color: "#203D54".to_owned(),
        tertiary_color: "#183F3B".to_owned(),
        cluster_background: "#252634".to_owned(),
        cluster_border: "#7C7493".to_owned(),
        sequence_actor_fill: "#363052".to_owned(),
        sequence_actor_border: "#AD95EB".to_owned(),
        sequence_actor_line: "#A9B7CF".to_owned(),
        sequence_note_fill: "#59452D".to_owned(),
        sequence_note_border: "#D4AD67".to_owned(),
        sequence_activation_fill: "#203D54".to_owned(),
        sequence_activation_border: "#73AAD6".to_owned(),
        text_color: "#E7EDF7".to_owned(),
        git_colors: [
            "#B6A0EF", "#83B9E5", "#7CC8B7", "#E0B575", "#DB91B0", "#9CACE9", "#AEC780", "#DEA084",
        ]
        .map(str::to_owned),
        git_inv_colors: ["#263347"; 8].map(str::to_owned),
        git_branch_label_colors: ["#263347"; 8].map(str::to_owned),
        git_commit_label_color: "#E7EDF7".to_owned(),
        git_tag_label_color: "#E7EDF7".to_owned(),
        git_tag_label_border: "#AD95EB".to_owned(),
        pie_colors: [
            "#554486", "#2E527A", "#21625C", "#76552B", "#763F57", "#435082", "#4C602C", "#7D4838",
            "#265D6A", "#634679", "#6D602E", "#3D5B67",
        ]
        .map(str::to_owned),
        pie_title_text_color: "#E7EDF7".to_owned(),
        pie_section_text_color: "#E7EDF7".to_owned(),
        pie_legend_text_color: "#E7EDF7".to_owned(),
    }
}

/// Native colour roles; both palettes retain identical font and stroke metrics.
pub fn native_variables(theme: &Theme) -> serde_json::Map<String, serde_json::Value> {
    use serde_json::{Value, json};
    let mut variables = json!({
        "darkMode": theme.text_color.eq_ignore_ascii_case("#E7EDF7"),
        "useGradient": false,
        "dropShadow": "none",
        "actorBkg": theme.sequence_actor_fill,
        "actorBorder": theme.sequence_actor_border,
        "actorLineColor": theme.sequence_actor_line,
        "actorTextColor": theme.text_color,
        "signalColor": theme.line_color,
        "signalTextColor": theme.text_color,
        "labelTextColor": theme.text_color,
        "loopTextColor": theme.text_color,
        "labelBoxBkgColor": theme.primary_color,
        "labelBoxBorderColor": theme.primary_border_color,
        "activationBkgColor": theme.sequence_activation_fill,
        "activationBorderColor": theme.sequence_activation_border,
        "pieTitleTextColor": theme.pie_title_text_color,
        "pieSectionTextColor": theme.pie_section_text_color,
        "pieLegendTextColor": theme.pie_legend_text_color,
        "pieTitleTextSize": "22px",
        "pieSectionTextSize": "16px",
        "pieLegendTextSize": "16px",
        "pieStrokeColor": theme.line_color,
        "pieOuterStrokeColor": "transparent",
        "pieOpacity": "1",
        "gitCommitLabelColor": theme.git_commit_label_color,
        "gitCommitLabelBackground": "transparent",
        "gitTagLabelColor": theme.git_tag_label_color,
        "gitTagLabelBackground": "transparent",
        "gitTagLabelBorder": theme.git_tag_label_border,
    })
    .as_object()
    .expect("native palette is an object")
    .clone();
    let chart_variables = json!({
        "gridColor": theme.cluster_border,
        "sectionBkgColor": theme.secondary_color,
        "sectionBkgColor2": theme.tertiary_color,
        "altSectionBkgColor": theme.cluster_background,
        "excludeBkgColor": theme.cluster_background,
        "taskBkgColor": theme.primary_color,
        "taskBorderColor": theme.primary_border_color,
        "activeTaskBkgColor": theme.secondary_color,
        "activeTaskBorderColor": theme.primary_border_color,
        "doneTaskBkgColor": theme.tertiary_color,
        "doneTaskBorderColor": theme.cluster_border,
        "critBkgColor": theme.sequence_note_fill,
        "critBorderColor": theme.sequence_note_border,
        "taskTextColor": theme.text_color,
        "taskTextOutsideColor": theme.text_color,
        "taskTextLightColor": theme.text_color,
        "taskTextDarkColor": theme.text_color,
        "taskTextClickableColor": theme.text_color,
        "scaleLabelColor": theme.text_color,
        "archEdgeColor": theme.line_color,
        "archEdgeArrowColor": theme.line_color,
        "archGroupBorderColor": theme.primary_border_color,
        "quadrantPointTextFill": theme.text_color,
        "quadrantXAxisTextFill": theme.text_color,
        "quadrantYAxisTextFill": theme.text_color,
        "quadrantTitleFill": theme.text_color,
        "quadrantInternalBorderStrokeFill": theme.primary_border_color,
        "quadrantExternalBorderStrokeFill": theme.primary_border_color,
        "relationLabelColor": theme.text_color,
        "radar": {
            "axisColor": theme.line_color,
            "graticuleColor": theme.cluster_border
        },
        "xyChart": {
            "backgroundColor": "transparent",
            "plotColorPalette": theme.pie_colors.join(","),
            "titleColor": theme.text_color,
            "dataLabelColor": theme.text_color,
            "legendTextColor": theme.text_color,
            "xAxisTitleColor": theme.text_color,
            "xAxisLabelColor": theme.text_color,
            "xAxisTickColor": theme.line_color,
            "xAxisLineColor": theme.line_color,
            "yAxisTitleColor": theme.text_color,
            "yAxisLabelColor": theme.text_color,
            "yAxisTickColor": theme.line_color,
            "yAxisLineColor": theme.line_color
        },
        "cynefin": {
            "complexBg": theme.tertiary_color,
            "complicatedBg": theme.secondary_color,
            "chaoticBg": theme.primary_color,
            "clearBg": theme.sequence_note_fill,
            "confusionBg": theme.cluster_background,
            "textColor": theme.text_color,
            "labelColor": theme.text_color,
            "boundaryColor": theme.line_color,
            "arrowColor": theme.line_color
        },
        "wardley": {
            "backgroundColor": "transparent",
            "axisColor": theme.line_color,
            "axisTextColor": theme.text_color,
            "gridColor": theme.cluster_border,
            "componentFill": theme.primary_color,
            "componentStroke": theme.primary_border_color,
            "componentLabelColor": theme.text_color,
            "anchorLabelColor": theme.text_color,
            "stageDividerColor": theme.cluster_border,
            "linkStroke": theme.line_color,
            "annotationFill": "transparent",
            "annotationStroke": theme.line_color,
            "annotationTextColor": theme.text_color
        }
    })
    .as_object()
    .expect("native palette is an object")
    .clone();
    variables.extend(chart_variables);
    for (index, color) in theme.pie_colors.iter().enumerate() {
        for name in [format!("pie{}", index + 1), format!("cScale{index}")] {
            variables.insert(name, Value::String(color.clone()));
        }
        variables.insert(format!("cScaleLabel{index}"), json!(theme.text_color));
        if index < 8 {
            variables.insert(format!("fillType{index}"), json!(color));
        }
    }
    for index in 0..8 {
        variables.insert(format!("git{index}"), json!(theme.git_colors[index]));
        variables.insert(format!("gitInv{index}"), json!(theme.git_inv_colors[index]));
        variables.insert(
            format!("gitBranchLabel{index}"),
            json!(theme.git_branch_label_colors[index]),
        );
        variables.insert(format!("venn{}", index + 1), json!(theme.git_colors[index]));
    }
    for (index, color) in [
        &theme.primary_color,
        &theme.secondary_color,
        &theme.tertiary_color,
        &theme.cluster_background,
    ]
    .into_iter()
    .enumerate()
    {
        variables.insert(format!("quadrant{}Fill", index + 1), json!(color));
        variables.insert(
            format!("quadrant{}TextFill", index + 1),
            json!(theme.text_color),
        );
    }
    variables
}

/// Choose readable text for an opaque source fill.
pub fn contrasting_text(fill: &str) -> Option<&'static str> {
    let fill = fill.trim();
    let fill = if fill.to_ascii_lowercase().ends_with("!important") {
        fill[..fill.len() - "!important".len()].trim_end()
    } else {
        fill
    };
    let fill = fill.parse::<svgtypes::Color>().ok()?;
    if fill.alpha != 255 {
        return None;
    }
    let background = luminance(fill);
    let candidates = ["#263347", "#E7EDF7", "#000000", "#FFFFFF"];
    candidates.into_iter().find(|colour| {
        contrast(
            luminance(colour.parse().expect("valid palette colour")),
            background,
        ) >= 4.5
    })
}

fn luminance(colour: svgtypes::Color) -> f64 {
    let channel = |value: u8| {
        let value = f64::from(value) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(colour.red) + 0.7152 * channel(colour.green) + 0.0722 * channel(colour.blue)
}

fn contrast(a: f64, b: f64) -> f64 {
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_contrast(foreground: &str, background: &str, minimum: f64) {
        let a = luminance(foreground.parse().unwrap());
        let b = luminance(background.parse().unwrap());
        let ratio = contrast(a, b);
        assert!(
            ratio >= minimum,
            "{foreground} on {background}: contrast {ratio:.2}:1 is below {minimum}:1"
        );
    }

    #[test]
    fn fixed_fills_receive_contrasting_text_without_guessing_transparency() {
        for red in [0, 51, 102, 153, 204, 255] {
            for green in [0, 51, 102, 153, 204, 255] {
                for blue in [0, 51, 102, 153, 204, 255] {
                    let fill = format!("#{red:02x}{green:02x}{blue:02x}");
                    assert_contrast(contrasting_text(&fill).unwrap(), &fill, 4.5);
                }
            }
        }
        assert_eq!(contrasting_text("white !important"), Some("#263347"));
        assert_eq!(contrasting_text("rgb(0,0,0)"), Some("#E7EDF7"));
        for fill in [
            "transparent",
            "rgba(255,255,255,0.5)",
            "url(#gradient)",
            "currentColor",
            "none",
        ] {
            assert_eq!(contrasting_text(fill), None);
        }
    }

    #[test]
    fn text_is_readable_on_page_and_every_default_shape_fill() {
        for (theme, page) in [(light_theme(), "#FFFFFF"), (dark_theme(), "#1F232B")] {
            assert_contrast(&theme.text_color, page, 4.5);
            assert_contrast(&theme.line_color, page, 3.0);
            for fill in [
                &theme.primary_color,
                &theme.secondary_color,
                &theme.tertiary_color,
                &theme.cluster_background,
                &theme.sequence_actor_fill,
                &theme.sequence_note_fill,
                &theme.sequence_activation_fill,
            ] {
                assert_contrast(&theme.primary_text_color, fill, 4.5);
            }
            for fill in &theme.pie_colors {
                assert_contrast(&theme.pie_section_text_color, fill, 4.5);
            }
            for (fill, text) in theme.git_colors.iter().zip(&theme.git_branch_label_colors) {
                assert_contrast(text, fill, 4.5);
                assert_contrast(fill, page, 3.0);
            }
            assert_contrast(&theme.primary_border_color, &theme.primary_color, 3.0);
        }
    }
}
