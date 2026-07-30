// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};
use serde_json::{Map, Value};

use super::chart_data_parse::{parse_presentation_chart_data, ParsedPresentationChartData};
use super::chart_model::{
    PresentationChart, PresentationChartAxisTickMark, PresentationChartDataLabels,
    PresentationChartLegendPosition, PresentationChartSeries, PresentationChartType,
    PresentationChartValueAxis, PresentationChartValueAxisNumberFormat,
    PresentationChartValueAxisOptions,
};
use super::chart_parse::{
    parse_presentation_chart_axis_bound, parse_presentation_chart_axis_log_base,
    parse_presentation_chart_axis_number_format, parse_presentation_chart_axis_tick_mark,
    parse_presentation_chart_axis_unit, validate_presentation_chart_axis_bounds,
};
use super::text_validation::validate_slide_text;

struct ParsedPresentationChartInput {
    chart_type: PresentationChartType,
    title: String,
    category_axis_title: String,
    value_axis_title: String,
    secondary_value_axis_title: String,
    x_axis: PresentationChartValueAxisOptions,
    value_axis: PresentationChartValueAxisOptions,
    secondary_value_axis: PresentationChartValueAxisOptions,
}

pub(super) fn parse_presentation_chart(
    value: &Value,
    slide_number: usize,
) -> Result<PresentationChart> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("slide {slide_number} chart must be an object"))?;
    ensure_supported_presentation_chart_properties(object, slide_number)?;
    let input = parse_presentation_chart_input(object, slide_number)?;
    let data = parse_presentation_chart_data(object, input.chart_type, input.x_axis, slide_number)?;
    validate_presentation_chart_series_axes(&input, data.series.as_slice(), slide_number)?;
    let (show_legend, legend_position, data_labels) =
        parse_chart_legend_and_labels(object, input.chart_type, slide_number)?;
    Ok(build_presentation_chart(
        input,
        data,
        show_legend,
        legend_position,
        data_labels,
    ))
}

fn parse_presentation_chart_input(
    object: &Map<String, Value>,
    slide_number: usize,
) -> Result<ParsedPresentationChartInput> {
    let chart_type = PresentationChartType::parse(
        object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("slide {slide_number} chart type is required"))?,
    )?;
    let input = ParsedPresentationChartInput {
        chart_type,
        title: parse_presentation_chart_text(object, "title", slide_number)?,
        category_axis_title: parse_presentation_chart_text(
            object,
            "category_axis_title",
            slide_number,
        )?,
        value_axis_title: parse_presentation_chart_text(object, "value_axis_title", slide_number)?,
        secondary_value_axis_title: parse_presentation_chart_text(
            object,
            "secondary_value_axis_title",
            slide_number,
        )?,
        x_axis: parse_presentation_chart_axis_options(object, "x_axis", slide_number)?,
        value_axis: parse_presentation_chart_axis_options(object, "value_axis", slide_number)?,
        secondary_value_axis: parse_presentation_chart_axis_options(
            object,
            "secondary_value_axis",
            slide_number,
        )?,
    };
    validate_presentation_chart_type_axes(&input, slide_number)?;
    Ok(input)
}

fn parse_presentation_chart_text(
    object: &Map<String, Value>,
    key: &str,
    slide_number: usize,
) -> Result<String> {
    let value = object
        .get(key)
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| anyhow!("slide {slide_number} chart {key} must be a string"))
        })
        .transpose()?
        .unwrap_or_default()
        .to_string();
    validate_slide_text(
        value.as_str(),
        format!("slide {slide_number} chart {key}").as_str(),
        1_000,
    )?;
    if !value.is_empty() && value.trim().is_empty() {
        return Err(anyhow!(
            "slide {slide_number} chart {key} cannot contain only whitespace"
        ));
    }
    Ok(value)
}

fn parse_presentation_chart_axis_options(
    object: &Map<String, Value>,
    prefix: &str,
    slide_number: usize,
) -> Result<PresentationChartValueAxisOptions> {
    let minimum = format!("{prefix}_minimum");
    let maximum = format!("{prefix}_maximum");
    let log_base = format!("{prefix}_log_base");
    let major_tick_mark = format!("{prefix}_major_tick_mark");
    let minor_tick_mark = format!("{prefix}_minor_tick_mark");
    let major_unit = format!("{prefix}_major_unit");
    let minor_unit = format!("{prefix}_minor_unit");
    let number_format = format!("{prefix}_number_format");
    Ok(PresentationChartValueAxisOptions {
        minimum: parse_presentation_chart_axis_bound(
            object.get(minimum.as_str()),
            slide_number,
            minimum.as_str(),
        )?,
        maximum: parse_presentation_chart_axis_bound(
            object.get(maximum.as_str()),
            slide_number,
            maximum.as_str(),
        )?,
        log_base: parse_presentation_chart_axis_log_base(
            object.get(log_base.as_str()),
            slide_number,
            log_base.as_str(),
        )?,
        major_tick_mark: parse_presentation_chart_axis_tick_mark(
            object.get(major_tick_mark.as_str()),
            slide_number,
            major_tick_mark.as_str(),
        )?,
        minor_tick_mark: parse_presentation_chart_axis_tick_mark(
            object.get(minor_tick_mark.as_str()),
            slide_number,
            minor_tick_mark.as_str(),
        )?,
        major_unit: parse_presentation_chart_axis_unit(
            object.get(major_unit.as_str()),
            slide_number,
            major_unit.as_str(),
        )?,
        minor_unit: parse_presentation_chart_axis_unit(
            object.get(minor_unit.as_str()),
            slide_number,
            minor_unit.as_str(),
        )?,
        number_format: parse_presentation_chart_axis_number_format(
            object.get(number_format.as_str()),
            slide_number,
            number_format.as_str(),
        )?,
    })
}

fn validate_presentation_chart_type_axes(
    input: &ParsedPresentationChartInput,
    slide_number: usize,
) -> Result<()> {
    if !input.chart_type.uses_numeric_x_axis() && !chart_axis_options_are_default(input.x_axis) {
        return Err(anyhow!(
            "slide {slide_number} chart without a numeric X axis does not support X-axis bounds, logarithmic scale, tick marks, units, or number format"
        ));
    }
    if input.chart_type.is_part_to_whole()
        && (!input.category_axis_title.is_empty()
            || !input.value_axis_title.is_empty()
            || !input.secondary_value_axis_title.is_empty()
            || !chart_axis_options_are_default(input.value_axis)
            || !chart_axis_options_are_default(input.secondary_value_axis))
    {
        return Err(anyhow!(
            "slide {slide_number} {} chart does not support category/value axis titles, bounds, logarithmic scales, tick marks, units, or number formats",
            input.chart_type.as_str()
        ));
    }
    Ok(())
}

fn validate_presentation_chart_series_axes(
    input: &ParsedPresentationChartInput,
    series: &[PresentationChartSeries],
    slide_number: usize,
) -> Result<()> {
    let secondary_series = series
        .iter()
        .filter(|series| series.value_axis == PresentationChartValueAxis::Secondary)
        .count();
    if input.chart_type.is_part_to_whole() && secondary_series != 0 {
        return Err(anyhow!(
            "slide {slide_number} {} chart series must use the primary value_axis",
            input.chart_type.as_str()
        ));
    }
    if secondary_series == series.len() {
        return Err(anyhow!(
            "slide {slide_number} chart secondary value axis requires at least one primary series"
        ));
    }
    if secondary_series == 0 && !input.secondary_value_axis_title.is_empty() {
        return Err(anyhow!(
            "slide {slide_number} chart secondary_value_axis_title requires at least one secondary series"
        ));
    }
    if secondary_series == 0 && !chart_axis_options_are_default(input.secondary_value_axis) {
        return Err(anyhow!(
            "slide {slide_number} chart secondary value-axis bounds, logarithmic scale, tick marks, units, or number format require at least one secondary series"
        ));
    }
    validate_presentation_chart_axis_bounds(
        series,
        PresentationChartValueAxis::Primary,
        input.value_axis.minimum,
        input.value_axis.maximum,
        input.value_axis.log_base,
        input.value_axis.major_unit,
        input.value_axis.minor_unit,
        slide_number,
        "primary",
    )?;
    if secondary_series != 0 {
        validate_presentation_chart_axis_bounds(
            series,
            PresentationChartValueAxis::Secondary,
            input.secondary_value_axis.minimum,
            input.secondary_value_axis.maximum,
            input.secondary_value_axis.log_base,
            input.secondary_value_axis.major_unit,
            input.secondary_value_axis.minor_unit,
            slide_number,
            "secondary",
        )?;
    }
    Ok(())
}

fn parse_chart_legend_and_labels(
    object: &Map<String, Value>,
    chart_type: PresentationChartType,
    slide_number: usize,
) -> Result<(
    bool,
    PresentationChartLegendPosition,
    PresentationChartDataLabels,
)> {
    let show_legend = object
        .get("show_legend")
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| anyhow!("slide {slide_number} chart show_legend must be a boolean"))
        })
        .transpose()?
        .unwrap_or(true);
    let legend_position = PresentationChartLegendPosition::parse(
        object
            .get("legend_position")
            .map(|value| {
                value.as_str().ok_or_else(|| {
                    anyhow!("slide {slide_number} chart legend_position must be a string")
                })
            })
            .transpose()?
            .unwrap_or("right"),
    )?;
    if !show_legend && legend_position != PresentationChartLegendPosition::Right {
        return Err(anyhow!(
            "slide {slide_number} chart legend_position must be right when show_legend=false"
        ));
    }
    let data_labels = PresentationChartDataLabels::parse(
        object
            .get("data_labels")
            .map(|value| {
                value.as_str().ok_or_else(|| {
                    anyhow!("slide {slide_number} chart data_labels must be a string")
                })
            })
            .transpose()?
            .unwrap_or("none"),
    )?;
    if data_labels == PresentationChartDataLabels::Percentage && !chart_type.is_part_to_whole() {
        return Err(anyhow!(
            "slide {slide_number} percentage data labels are supported only for pie or doughnut charts"
        ));
    }
    Ok((show_legend, legend_position, data_labels))
}

fn chart_axis_options_are_default(options: PresentationChartValueAxisOptions) -> bool {
    options.minimum.is_none()
        && options.maximum.is_none()
        && options.log_base.is_none()
        && options.major_tick_mark == PresentationChartAxisTickMark::None
        && options.minor_tick_mark == PresentationChartAxisTickMark::None
        && options.major_unit.is_none()
        && options.minor_unit.is_none()
        && options.number_format == PresentationChartValueAxisNumberFormat::General
}

fn build_presentation_chart(
    input: ParsedPresentationChartInput,
    data: ParsedPresentationChartData,
    show_legend: bool,
    legend_position: PresentationChartLegendPosition,
    data_labels: PresentationChartDataLabels,
) -> PresentationChart {
    PresentationChart {
        chart_type: input.chart_type,
        title: input.title,
        categories: data.categories,
        x_values: data.x_values,
        x_axis_minimum: input.x_axis.minimum,
        x_axis_maximum: input.x_axis.maximum,
        x_axis_log_base: input.x_axis.log_base,
        x_axis_major_tick_mark: input.x_axis.major_tick_mark,
        x_axis_minor_tick_mark: input.x_axis.minor_tick_mark,
        x_axis_major_unit: input.x_axis.major_unit,
        x_axis_minor_unit: input.x_axis.minor_unit,
        x_axis_number_format: input.x_axis.number_format,
        series: data.series,
        show_legend,
        legend_position,
        data_labels,
        category_axis_title: input.category_axis_title,
        value_axis_title: input.value_axis_title,
        secondary_value_axis_title: input.secondary_value_axis_title,
        value_axis_minimum: input.value_axis.minimum,
        value_axis_maximum: input.value_axis.maximum,
        value_axis_log_base: input.value_axis.log_base,
        value_axis_major_tick_mark: input.value_axis.major_tick_mark,
        value_axis_minor_tick_mark: input.value_axis.minor_tick_mark,
        value_axis_major_unit: input.value_axis.major_unit,
        value_axis_minor_unit: input.value_axis.minor_unit,
        value_axis_number_format: input.value_axis.number_format,
        secondary_value_axis_minimum: input.secondary_value_axis.minimum,
        secondary_value_axis_maximum: input.secondary_value_axis.maximum,
        secondary_value_axis_log_base: input.secondary_value_axis.log_base,
        secondary_value_axis_major_tick_mark: input.secondary_value_axis.major_tick_mark,
        secondary_value_axis_minor_tick_mark: input.secondary_value_axis.minor_tick_mark,
        secondary_value_axis_major_unit: input.secondary_value_axis.major_unit,
        secondary_value_axis_minor_unit: input.secondary_value_axis.minor_unit,
        secondary_value_axis_number_format: input.secondary_value_axis.number_format,
    }
}

fn ensure_supported_presentation_chart_properties(
    object: &Map<String, Value>,
    slide_number: usize,
) -> Result<()> {
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "type"
                | "title"
                | "categories"
                | "x_values"
                | "x_axis_minimum"
                | "x_axis_maximum"
                | "x_axis_log_base"
                | "x_axis_major_tick_mark"
                | "x_axis_minor_tick_mark"
                | "x_axis_major_unit"
                | "x_axis_minor_unit"
                | "x_axis_number_format"
                | "series"
                | "show_legend"
                | "legend_position"
                | "data_labels"
                | "category_axis_title"
                | "value_axis_title"
                | "secondary_value_axis_title"
                | "value_axis_minimum"
                | "value_axis_maximum"
                | "value_axis_log_base"
                | "value_axis_major_tick_mark"
                | "value_axis_minor_tick_mark"
                | "value_axis_major_unit"
                | "value_axis_minor_unit"
                | "value_axis_number_format"
                | "secondary_value_axis_minimum"
                | "secondary_value_axis_maximum"
                | "secondary_value_axis_log_base"
                | "secondary_value_axis_major_tick_mark"
                | "secondary_value_axis_minor_tick_mark"
                | "secondary_value_axis_major_unit"
                | "secondary_value_axis_minor_unit"
                | "secondary_value_axis_number_format"
        )
    }) {
        return Err(anyhow!(
            "slide {slide_number} chart contains unsupported properties"
        ));
    }
    Ok(())
}
