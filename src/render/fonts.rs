//! Portable text measurement using embedded, Arial-compatible fonts.

use anyhow::{Result, anyhow};
use merman::OperationControl;
use merman::svg::export::{NativeFontContext, NativeFontLimits};
use merman::svg::{
    DeterministicTextMeasurer, HostTextMeasurement, HostTextMeasurementRequest, HostTextMeasurer,
    TextMeasurementOperation, TextMeasurementPhase, TextMeasurer, TextStyle, WrapMode,
};
use std::sync::LazyLock;

static FONTS: LazyLock<Result<NativeFontContext, String>> = LazyLock::new(|| {
    let fonts: [&[u8]; 4] = [
        include_bytes!("../../assets/fonts/LiberationSans-Regular.ttf"),
        include_bytes!("../../assets/fonts/LiberationSans-Bold.ttf"),
        include_bytes!("../../assets/fonts/LiberationSans-Italic.ttf"),
        include_bytes!("../../assets/fonts/LiberationSans-BoldItalic.ttf"),
    ];
    NativeFontContext::from_font_bytes(
        fonts.into_iter().map(<[u8]>::to_vec),
        NativeFontLimits::default(),
        &OperationControl::new(),
    )
    .map_err(|error| error.to_string())
});

pub fn environment() -> Result<merman::SvgEnvironment> {
    let context = FONTS
        .as_ref()
        .map_err(|error| anyhow!("cannot load embedded fonts: {error}"))?;
    merman::SvgEnvironment::deterministic()
        .with_native_font_context(context.clone())
        .map_err(Into::into)
}

pub fn text_width(text: &str, style: &TextStyle) -> Result<f64> {
    let context = FONTS
        .as_ref()
        .map_err(|error| anyhow!("cannot load embedded fonts: {error}"))?
        .for_operation(&OperationControl::new());
    let measured = context.measure(HostTextMeasurementRequest {
        operation: TextMeasurementOperation::Measure,
        phase: TextMeasurementPhase::Layout,
        text,
        style,
        max_width: None,
        wrap_mode: WrapMode::SvgLike,
    })?;
    match measured {
        Some(HostTextMeasurement::Metrics(metrics)) => Ok(metrics.width),
        None => Ok(DeterministicTextMeasurer::default()
            .measure(text, style)
            .width),
        _ => anyhow::bail!("font measurer returned an unexpected result"),
    }
}
