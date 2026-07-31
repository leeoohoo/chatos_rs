// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};
use serde_json::Value;

use super::chart_model::{
    PresentationChartAxisTickMark, PresentationChartMarkerStyle, PresentationChartSeries,
    PresentationChartType, PresentationChartValueAxis, PresentationChartValueAxisNumberFormat,
};
use super::limits::{
    DEFAULT_PPTX_CREATE_CHART_MARKER_SIZE, MAX_PPTX_CREATE_CHART_LOG_BASE,
    MAX_PPTX_CREATE_CHART_MARKER_SIZE, MAX_PPTX_CREATE_CHART_VALUE_ABS,
    MIN_PPTX_CREATE_CHART_LOG_BASE, MIN_PPTX_CREATE_CHART_MARKER_SIZE,
};

pub(super) fn parse_presentation_chart_series_color(
    value: Option<&Value>,
    slide_number: usize,
    series_number: usize,
) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let value = value.as_str().ok_or_else(|| {
        anyhow!(
            "slide {slide_number} chart series {series_number} color must be a #RRGGBB string or null"
        )
    })?;
    normalize_presentation_chart_api_color(value)
        .map(Some)
        .ok_or_else(|| {
            anyhow!(
            "slide {slide_number} chart series {series_number} color must use exact #RRGGBB syntax"
        )
        })
}

fn normalize_presentation_chart_api_color(value: &str) -> Option<String> {
    let rgb = value.strip_prefix('#')?;
    normalize_presentation_chart_rgb(rgb).map(|rgb| format!("#{rgb}"))
}

pub(super) fn normalize_presentation_chart_rgb(value: &str) -> Option<String> {
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(value.to_ascii_uppercase())
}

pub(super) fn parse_presentation_chart_series_marker(
    chart_type: PresentationChartType,
    style: Option<&Value>,
    size: Option<&Value>,
    slide_number: usize,
    series_number: usize,
) -> Result<(Option<PresentationChartMarkerStyle>, Option<u8>)> {
    if !matches!(
        chart_type,
        PresentationChartType::Line | PresentationChartType::Scatter
    ) {
        if style.is_some_and(|value| !value.is_null()) || size.is_some_and(|value| !value.is_null())
        {
            return Err(anyhow!(
                "slide {slide_number} chart series {series_number} marker_style and marker_size are supported only for line or scatter charts"
            ));
        }
        return Ok((None, None));
    }
    let style = match style {
        None => PresentationChartMarkerStyle::Circle,
        Some(value) if value.is_null() => PresentationChartMarkerStyle::Circle,
        Some(value) => PresentationChartMarkerStyle::parse(value.as_str().ok_or_else(|| {
            anyhow!(
                "slide {slide_number} chart series {series_number} marker_style must be none, circle, square, diamond, triangle, or null"
            )
        })?)?,
    };
    if style == PresentationChartMarkerStyle::None {
        if size.is_some_and(|value| !value.is_null()) {
            return Err(anyhow!(
                "slide {slide_number} chart series {series_number} marker_size must be null or omitted when marker_style=none"
            ));
        }
        return Ok((Some(style), None));
    }
    let size = match size {
        None => DEFAULT_PPTX_CREATE_CHART_MARKER_SIZE,
        Some(value) if value.is_null() => DEFAULT_PPTX_CREATE_CHART_MARKER_SIZE,
        Some(value) => {
            let value = value.as_u64().ok_or_else(|| {
                anyhow!(
                    "slide {slide_number} chart series {series_number} marker_size must be an integer between {MIN_PPTX_CREATE_CHART_MARKER_SIZE} and {MAX_PPTX_CREATE_CHART_MARKER_SIZE}, or null"
                )
            })?;
            let value = u8::try_from(value).map_err(|_| {
                anyhow!(
                    "slide {slide_number} chart series {series_number} marker_size exceeds the safety limit"
                )
            })?;
            if !(MIN_PPTX_CREATE_CHART_MARKER_SIZE..=MAX_PPTX_CREATE_CHART_MARKER_SIZE)
                .contains(&value)
            {
                return Err(anyhow!(
                    "slide {slide_number} chart series {series_number} marker_size must be between {MIN_PPTX_CREATE_CHART_MARKER_SIZE} and {MAX_PPTX_CREATE_CHART_MARKER_SIZE}"
                ));
            }
            value
        }
    };
    Ok((Some(style), Some(size)))
}

pub(super) fn parse_presentation_chart_series_smooth(
    chart_type: PresentationChartType,
    value: Option<&Value>,
    slide_number: usize,
    series_number: usize,
) -> Result<Option<bool>> {
    if !matches!(
        chart_type,
        PresentationChartType::Line | PresentationChartType::Scatter
    ) {
        if value.is_some_and(|value| !value.is_null()) {
            return Err(anyhow!(
                "slide {slide_number} chart series {series_number} smooth is supported only for line or scatter charts"
            ));
        }
        return Ok(None);
    }
    match value {
        None => Ok(Some(false)),
        Some(value) if value.is_null() => Ok(Some(false)),
        Some(value) => value.as_bool().map(Some).ok_or_else(|| {
            anyhow!(
                "slide {slide_number} chart series {series_number} smooth must be a boolean or null"
            )
        }),
    }
}

pub(super) fn parse_presentation_chart_axis_bound(
    value: Option<&Value>,
    slide_number: usize,
    field: &str,
) -> Result<Option<f64>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let value = value.as_f64().ok_or_else(|| {
        anyhow!("slide {slide_number} chart {field} must be a finite number or null")
    })?;
    if !value.is_finite() || value.abs() > MAX_PPTX_CREATE_CHART_VALUE_ABS {
        return Err(anyhow!(
            "slide {slide_number} chart {field} exceeds the numeric safety limit"
        ));
    }
    Ok(Some(value))
}

pub(super) fn parse_presentation_chart_axis_number_format(
    value: Option<&Value>,
    slide_number: usize,
    field: &str,
) -> Result<PresentationChartValueAxisNumberFormat> {
    let value = value
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| anyhow!("slide {slide_number} chart {field} must be a string"))
        })
        .transpose()?
        .unwrap_or("general");
    PresentationChartValueAxisNumberFormat::parse(value)
}

pub(super) fn parse_presentation_chart_axis_tick_mark(
    value: Option<&Value>,
    slide_number: usize,
    field: &str,
) -> Result<PresentationChartAxisTickMark> {
    let value = value
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| anyhow!("slide {slide_number} chart {field} must be a string"))
        })
        .transpose()?
        .unwrap_or("none");
    PresentationChartAxisTickMark::parse(value)
}

pub(super) fn parse_presentation_chart_axis_log_base(
    value: Option<&Value>,
    slide_number: usize,
    field: &str,
) -> Result<Option<f64>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let value = value.as_f64().ok_or_else(|| {
        anyhow!(
            "slide {slide_number} chart {field} must be a finite number between {MIN_PPTX_CREATE_CHART_LOG_BASE} and {MAX_PPTX_CREATE_CHART_LOG_BASE}, or null"
        )
    })?;
    if !value.is_finite()
        || !(MIN_PPTX_CREATE_CHART_LOG_BASE..=MAX_PPTX_CREATE_CHART_LOG_BASE).contains(&value)
    {
        return Err(anyhow!(
            "slide {slide_number} chart {field} must be between {MIN_PPTX_CREATE_CHART_LOG_BASE} and {MAX_PPTX_CREATE_CHART_LOG_BASE}"
        ));
    }
    Ok(Some(value))
}

pub(super) fn parse_presentation_chart_axis_unit(
    value: Option<&Value>,
    slide_number: usize,
    field: &str,
) -> Result<Option<f64>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let value = value.as_f64().ok_or_else(|| {
        anyhow!("slide {slide_number} chart {field} must be a finite positive number or null")
    })?;
    if !value.is_finite() || value <= 0.0 || value > MAX_PPTX_CREATE_CHART_VALUE_ABS {
        return Err(anyhow!(
            "slide {slide_number} chart {field} must be positive and within the numeric safety limit"
        ));
    }
    Ok(Some(value))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_presentation_chart_axis_bounds(
    series: &[PresentationChartSeries],
    axis: PresentationChartValueAxis,
    minimum: Option<f64>,
    maximum: Option<f64>,
    log_base: Option<f64>,
    major_unit: Option<f64>,
    minor_unit: Option<f64>,
    slide_number: usize,
    label: &str,
) -> Result<()> {
    let values = series
        .iter()
        .filter(|series| series.value_axis == axis)
        .flat_map(|series| series.values.iter().copied())
        .collect::<Vec<_>>();
    validate_presentation_chart_numeric_axis_bounds(
        values.as_slice(),
        minimum,
        maximum,
        log_base,
        major_unit,
        minor_unit,
        slide_number,
        format!("{label} value-axis").as_str(),
        format!("{label} logarithmic value-axis").as_str(),
        "series value",
        "series values",
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_presentation_chart_numeric_axis_bounds(
    values: &[f64],
    minimum: Option<f64>,
    maximum: Option<f64>,
    log_base: Option<f64>,
    major_unit: Option<f64>,
    minor_unit: Option<f64>,
    slide_number: usize,
    axis_label: &str,
    logarithmic_axis_label: &str,
    positive_data_item: &str,
    hidden_data_items: &str,
) -> Result<()> {
    if matches!((minimum, maximum), (Some(minimum), Some(maximum)) if minimum >= maximum) {
        return Err(anyhow!(
            "slide {slide_number} chart {axis_label} minimum must be below its maximum"
        ));
    }
    if matches!((major_unit, minor_unit), (Some(major_unit), Some(minor_unit)) if minor_unit >= major_unit)
    {
        return Err(anyhow!(
            "slide {slide_number} chart {axis_label} minor unit must be below its major unit"
        ));
    }
    if let (Some(minimum), Some(maximum)) = (minimum, maximum) {
        let span = maximum - minimum;
        if major_unit.is_some_and(|major_unit| major_unit > span) {
            return Err(anyhow!(
                "slide {slide_number} chart {axis_label} major unit exceeds its explicit range"
            ));
        }
        if minor_unit.is_some_and(|minor_unit| minor_unit > span) {
            return Err(anyhow!(
                "slide {slide_number} chart {axis_label} minor unit exceeds its explicit range"
            ));
        }
    }
    let data_minimum = values
        .iter()
        .copied()
        .reduce(f64::min)
        .ok_or_else(|| anyhow!("slide {slide_number} chart {axis_label} has no data values"))?;
    let data_maximum = values
        .iter()
        .copied()
        .reduce(f64::max)
        .ok_or_else(|| anyhow!("slide {slide_number} chart {axis_label} has no data values"))?;
    if log_base.is_some() {
        if minimum.is_some_and(|minimum| minimum <= 0.0)
            || maximum.is_some_and(|maximum| maximum <= 0.0)
        {
            return Err(anyhow!(
                "slide {slide_number} chart {logarithmic_axis_label} bounds must be positive"
            ));
        }
        if data_minimum <= 0.0 {
            let logarithmic_data_axis_label = logarithmic_axis_label.replace("-axis", " axis");
            return Err(anyhow!(
                "slide {slide_number} chart {logarithmic_data_axis_label} requires every {positive_data_item} to be positive"
            ));
        }
    }
    if minimum.is_some_and(|minimum| minimum > data_minimum) {
        return Err(anyhow!(
            "slide {slide_number} chart {axis_label} minimum would hide {hidden_data_items}"
        ));
    }
    if maximum.is_some_and(|maximum| maximum < data_maximum) {
        return Err(anyhow!(
            "slide {slide_number} chart {axis_label} maximum would hide {hidden_data_items}"
        ));
    }
    Ok(())
}
