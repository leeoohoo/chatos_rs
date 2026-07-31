// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use super::chart_model::{
    PresentationChart, PresentationChartAxisTickMark, PresentationChartDataLabels,
    PresentationChartLegendPosition, PresentationChartMarkerStyle, PresentationChartSeries,
    PresentationChartType, PresentationChartValueAxis, PresentationChartValueAxisNumberFormat,
    PresentationChartValueAxisOptions,
};
use super::chart_xml::presentation_chart_xml;
use super::limits::{MAX_PPTX_CREATE_CHART_SERIES, MAX_PPTX_CREATE_CHART_VALUE_ABS};
use super::model::{PptxChartInspection, PptxChartRelationshipInspection};
use super::{
    canonical_pptx_chart_axis_options, parse_presentation_chart, pptx_chart_value_axis_by_position,
};

pub(super) fn presentation_chart_snapshot(chart: &PresentationChart) -> Value {
    json!({
        "type": chart.chart_type.as_str(),
        "title": chart.title,
        "categories": chart.categories,
        "x_values": chart.x_values,
        "x_axis_minimum": chart.x_axis_minimum,
        "x_axis_maximum": chart.x_axis_maximum,
        "x_axis_log_base": chart.x_axis_log_base,
        "x_axis_major_tick_mark": chart.x_axis_major_tick_mark.as_str(),
        "x_axis_minor_tick_mark": chart.x_axis_minor_tick_mark.as_str(),
        "x_axis_major_unit": chart.x_axis_major_unit,
        "x_axis_minor_unit": chart.x_axis_minor_unit,
        "x_axis_number_format": chart.x_axis_number_format.as_str(),
        "series": chart.series.iter().map(|series| json!({
            "name": series.name,
            "values": series.values,
            "bubble_sizes": series.bubble_sizes,
            "value_axis": series.value_axis.as_str(),
            "color": series.color,
            "marker_style": series.marker_style.map(PresentationChartMarkerStyle::as_str),
            "marker_size": series.marker_size,
            "smooth": series.smooth,
        })).collect::<Vec<_>>(),
        "show_legend": chart.show_legend,
        "legend_position": chart.legend_position.as_str(),
        "data_labels": chart.data_labels.as_str(),
        "category_axis_title": chart.category_axis_title,
        "value_axis_title": chart.value_axis_title,
        "secondary_value_axis_title": chart.secondary_value_axis_title,
        "value_axis_minimum": chart.value_axis_minimum,
        "value_axis_maximum": chart.value_axis_maximum,
        "value_axis_log_base": chart.value_axis_log_base,
        "value_axis_major_tick_mark": chart.value_axis_major_tick_mark.as_str(),
        "value_axis_minor_tick_mark": chart.value_axis_minor_tick_mark.as_str(),
        "value_axis_major_unit": chart.value_axis_major_unit,
        "value_axis_minor_unit": chart.value_axis_minor_unit,
        "value_axis_number_format": chart.value_axis_number_format.as_str(),
        "secondary_value_axis_minimum": chart.secondary_value_axis_minimum,
        "secondary_value_axis_maximum": chart.secondary_value_axis_maximum,
        "secondary_value_axis_log_base": chart.secondary_value_axis_log_base,
        "secondary_value_axis_major_tick_mark": chart.secondary_value_axis_major_tick_mark.as_str(),
        "secondary_value_axis_minor_tick_mark": chart.secondary_value_axis_minor_tick_mark.as_str(),
        "secondary_value_axis_major_unit": chart.secondary_value_axis_major_unit,
        "secondary_value_axis_minor_unit": chart.secondary_value_axis_minor_unit,
        "secondary_value_axis_number_format": chart.secondary_value_axis_number_format.as_str(),
    })
}

pub(super) fn canonical_pptx_chart_snapshot(
    inspection: &PptxChartInspection,
    relationships: &PptxChartRelationshipInspection,
    xml: &str,
) -> Result<(PresentationChart, Value)> {
    if relationships.relationship_count != 0 || relationships.relationships_part_present {
        return Err(anyhow!(
            "canonical self-contained charts must not have a chart relationships part"
        ));
    }
    if inspection.chart_types.len() != 1 {
        return Err(anyhow!(
            "canonical self-contained charts require exactly one supported chart type"
        ));
    }
    if inspection.chart_groups.is_empty() || inspection.chart_groups.len() > 2 {
        return Err(anyhow!(
            "canonical self-contained charts require one primary chart group and at most one secondary chart group"
        ));
    }
    let chart_type = match inspection.chart_types[0].as_str() {
        "bar" => {
            let directions = inspection
                .chart_groups
                .iter()
                .map(|group| group.bar_direction.as_deref())
                .collect::<BTreeSet<_>>();
            if directions.len() == 1 && directions.contains(&Some("col")) {
                PresentationChartType::Column
            } else if directions.len() == 1 && directions.contains(&Some("bar")) {
                PresentationChartType::Bar
            } else {
                return Err(anyhow!(
                    "canonical bar chart groups require one consistent col or bar direction"
                ));
            }
        }
        "line" => PresentationChartType::Line,
        "pie" => PresentationChartType::Pie,
        "area" => PresentationChartType::Area,
        "doughnut" => PresentationChartType::Doughnut,
        "radar" => {
            let styles = inspection
                .chart_groups
                .iter()
                .map(|group| group.radar_style.as_deref())
                .collect::<BTreeSet<_>>();
            if styles.len() == 1 && styles.contains(&Some("standard")) {
                PresentationChartType::Radar
            } else {
                return Err(anyhow!(
                    "canonical radar chart groups require one consistent standard style"
                ));
            }
        }
        "scatter" => {
            let styles = inspection
                .chart_groups
                .iter()
                .map(|group| group.scatter_style.as_deref())
                .collect::<BTreeSet<_>>();
            if styles.len() == 1 && styles.contains(&Some("lineMarker")) {
                PresentationChartType::Scatter
            } else {
                return Err(anyhow!(
                    "canonical scatter chart groups require one consistent lineMarker style"
                ));
            }
        }
        "bubble" => {
            if inspection.chart_groups.iter().all(|group| {
                group.bubble_scale.as_deref() == Some("100")
                    && group.show_negative_bubbles.as_deref() == Some("0")
                    && group.bubble_size_represents.as_deref() == Some("area")
                    && group.bubble_3d.is_none()
            }) {
                PresentationChartType::Bubble
            } else {
                return Err(anyhow!(
                    "canonical bubble chart groups require bubbleScale=100, showNegBubbles=0, sizeRepresents=area, and no bubble3D"
                ));
            }
        }
        _ => {
            return Err(anyhow!(
                "chart type is outside the canonical column, bar, line, pie, area, doughnut, radar, scatter, or bubble contract"
            ));
        }
    };
    if inspection.title_formula.is_some() || inspection.title_truncated {
        return Err(anyhow!(
            "canonical self-contained chart titles must be complete literal text"
        ));
    }
    if inspection.legend_count > 1 {
        return Err(anyhow!(
            "canonical self-contained charts contain at most one legend"
        ));
    }
    let legend_position = match inspection.legend_count {
        0 if inspection.legend_positions.is_empty() => PresentationChartLegendPosition::Right,
        1 if inspection.legend_positions.len() == 1 => {
            PresentationChartLegendPosition::from_ooxml(inspection.legend_positions[0].as_str())?
        }
        _ => {
            return Err(anyhow!(
                "canonical self-contained charts require exactly one legend position per visible legend"
            ));
        }
    };
    let chart_group_count = inspection.chart_groups.len();
    let data_labels = match (
        inspection.data_label_group_count,
        inspection.data_label_show_value_count,
        inspection.data_label_show_percentage_count,
    ) {
        (0, 0, 0) => PresentationChartDataLabels::None,
        (groups, values, 0) if groups == chart_group_count && values == chart_group_count => {
            PresentationChartDataLabels::Value
        }
        (groups, 0, percentages)
            if groups == chart_group_count && percentages == chart_group_count =>
        {
            PresentationChartDataLabels::Percentage
        }
        _ => {
            return Err(anyhow!(
                "chart data labels are outside the canonical none, value, or percentage contract"
            ));
        }
    };
    if inspection.category_axis_title_formula.is_some()
        || inspection.category_axis_title_truncated
        || inspection.value_axis_title_formula.is_some()
        || inspection.value_axis_title_truncated
        || inspection.secondary_value_axis_title_formula.is_some()
        || inspection.secondary_value_axis_title_truncated
    {
        return Err(anyhow!(
            "canonical self-contained chart axis titles must be complete literal text"
        ));
    }
    let default_axis_options = || PresentationChartValueAxisOptions {
        minimum: None,
        maximum: None,
        log_base: None,
        major_tick_mark: PresentationChartAxisTickMark::None,
        minor_tick_mark: PresentationChartAxisTickMark::None,
        major_unit: None,
        minor_unit: None,
        number_format: PresentationChartValueAxisNumberFormat::General,
    };
    let x_axis_options = if chart_type.uses_numeric_x_axis() {
        let bottom_axis = pptx_chart_value_axis_by_position(inspection.axes.as_slice(), "b")
            .ok_or_else(|| anyhow!("canonical numeric-X chart is missing its bottom X axis"))?;
        let bottom_options = canonical_pptx_chart_axis_options(bottom_axis, "numeric X")?;
        if let Some(top_axis) = pptx_chart_value_axis_by_position(inspection.axes.as_slice(), "t") {
            let top_options = canonical_pptx_chart_axis_options(top_axis, "secondary numeric X")?;
            if top_options != bottom_options {
                return Err(anyhow!(
                    "canonical numeric-X chart bottom and hidden top X axes must use identical bounds, logarithmic scale, tick marks, units, and number format"
                ));
            }
        }
        bottom_options
    } else {
        default_axis_options()
    };
    let (value_axis_options, secondary_value_axis_options) = if chart_type.is_part_to_whole() {
        (default_axis_options(), default_axis_options())
    } else {
        let primary_axis = pptx_chart_value_axis_by_position(
            inspection.axes.as_slice(),
            chart_type.primary_value_axis_position(),
        )
        .ok_or_else(|| anyhow!("canonical chart is missing its primary value axis"))?;
        let primary_options = canonical_pptx_chart_axis_options(primary_axis, "primary")?;
        let secondary_options = match pptx_chart_value_axis_by_position(
            inspection.axes.as_slice(),
            chart_type.secondary_value_axis_position(),
        ) {
            Some(axis) => canonical_pptx_chart_axis_options(axis, "secondary")?,
            None => default_axis_options(),
        };
        (primary_options, secondary_options)
    };
    if inspection.series.is_empty() || inspection.series.len() > MAX_PPTX_CREATE_CHART_SERIES {
        return Err(anyhow!(
            "chart series count is outside the canonical creation contract"
        ));
    }

    let expected_series_type = inspection.chart_types[0].as_str();
    let cached_categories = inspection.series[0].categories.clone();
    let mut series = Vec::with_capacity(inspection.series.len());
    for item in &inspection.series {
        if item.chart_type != expected_series_type {
            return Err(anyhow!(
                "canonical self-contained chart series must all use the selected chart type"
            ));
        }
        if item.name_formula.is_some()
            || item.category_formula.is_some()
            || item.value_formula.is_some()
            || item.bubble_size_formula.is_some()
        {
            return Err(anyhow!(
                "canonical self-contained charts must not contain formulas"
            ));
        }
        let bubble_sizes = if chart_type == PresentationChartType::Bubble {
            if item.bubble_sizes.len() != item.categories.len() || item.bubble_sizes.is_empty() {
                return Err(anyhow!(
                    "canonical bubble chart series require one bubble-size value per X value"
                ));
            }
            Some(
                item.bubble_sizes
                    .iter()
                    .map(|value| {
                        let value = value.parse::<f64>().map_err(|_| {
                            anyhow!(
                                "canonical bubble sizes must be finite positive decimal numbers"
                            )
                        })?;
                        if !value.is_finite()
                            || value <= 0.0
                            || value > MAX_PPTX_CREATE_CHART_VALUE_ABS
                        {
                            return Err(anyhow!(
                                "canonical bubble sizes must be positive and within the numeric safety limit"
                            ));
                        }
                        Ok(value)
                    })
                    .collect::<Result<Vec<_>>>()?,
            )
        } else {
            if !item.bubble_sizes.is_empty() {
                return Err(anyhow!(
                    "canonical non-bubble charts must not contain bubble-size caches"
                ));
            }
            None
        };
        if item.categories != cached_categories {
            return Err(anyhow!(
                "canonical self-contained chart series must share identical categories or X values"
            ));
        }
        let values = item
            .values
            .iter()
            .map(|value| {
                value.parse::<f64>().map_err(|_| {
                    anyhow!("canonical self-contained chart values must be finite decimal numbers")
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let value_axis = PresentationChartValueAxis::parse(item.value_axis.as_str())
            .context("chart series axis is outside the canonical primary or secondary contract")?;
        if item.color_style_custom {
            return Err(anyhow!(
                "canonical self-contained chart series color styling is outside the exact solid RGB contract"
            ));
        }
        if item.marker_style_custom {
            return Err(anyhow!(
                "canonical self-contained chart series marker styling is outside the exact bounded line-marker contract"
            ));
        }
        let marker_style = item
            .marker_style
            .as_deref()
            .map(PresentationChartMarkerStyle::from_ooxml)
            .transpose()
            .context("chart series marker style is outside the canonical bounded contract")?;
        if matches!(
            chart_type,
            PresentationChartType::Line | PresentationChartType::Scatter
        ) {
            if marker_style.is_none() {
                return Err(anyhow!(
                    "canonical line or scatter chart series requires one marker style"
                ));
            }
        } else if marker_style.is_some() || item.marker_size.is_some() {
            return Err(anyhow!(
                "canonical chart series outside line or scatter must not contain marker styling"
            ));
        }
        if item.smooth_custom {
            return Err(anyhow!(
                "canonical self-contained chart series smoothing is outside the exact bounded line-smoothing contract"
            ));
        }
        if matches!(
            chart_type,
            PresentationChartType::Line | PresentationChartType::Scatter
        ) {
            if item.smooth.is_none() {
                return Err(anyhow!(
                    "canonical line or scatter chart series requires one smooth value"
                ));
            }
        } else if item.smooth.is_some() {
            return Err(anyhow!(
                "canonical chart series outside line or scatter must not contain smoothing"
            ));
        }
        series.push(PresentationChartSeries {
            name: item.name.clone(),
            values,
            bubble_sizes,
            value_axis,
            color: item.color.clone(),
            marker_style,
            marker_size: item.marker_size,
            smooth: item.smooth,
        });
    }
    let (categories, x_values) = if chart_type.uses_numeric_x_axis() {
        let x_values = cached_categories
            .iter()
            .map(|value| {
                value.parse::<f64>().map_err(|_| {
                    anyhow!(
                        "canonical self-contained numeric X values must be finite decimal numbers"
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;
        (None, Some(x_values))
    } else {
        (Some(cached_categories), None)
    };
    let candidate = PresentationChart {
        chart_type,
        title: inspection.title.clone(),
        categories,
        x_values,
        x_axis_minimum: x_axis_options.minimum,
        x_axis_maximum: x_axis_options.maximum,
        x_axis_log_base: x_axis_options.log_base,
        x_axis_major_tick_mark: x_axis_options.major_tick_mark,
        x_axis_minor_tick_mark: x_axis_options.minor_tick_mark,
        x_axis_major_unit: x_axis_options.major_unit,
        x_axis_minor_unit: x_axis_options.minor_unit,
        x_axis_number_format: x_axis_options.number_format,
        series,
        show_legend: inspection.legend_count == 1,
        legend_position,
        data_labels,
        category_axis_title: inspection.category_axis_title.clone(),
        value_axis_title: inspection.value_axis_title.clone(),
        secondary_value_axis_title: inspection.secondary_value_axis_title.clone(),
        value_axis_minimum: value_axis_options.minimum,
        value_axis_maximum: value_axis_options.maximum,
        value_axis_log_base: value_axis_options.log_base,
        value_axis_major_tick_mark: value_axis_options.major_tick_mark,
        value_axis_minor_tick_mark: value_axis_options.minor_tick_mark,
        value_axis_major_unit: value_axis_options.major_unit,
        value_axis_minor_unit: value_axis_options.minor_unit,
        value_axis_number_format: value_axis_options.number_format,
        secondary_value_axis_minimum: secondary_value_axis_options.minimum,
        secondary_value_axis_maximum: secondary_value_axis_options.maximum,
        secondary_value_axis_log_base: secondary_value_axis_options.log_base,
        secondary_value_axis_major_tick_mark: secondary_value_axis_options.major_tick_mark,
        secondary_value_axis_minor_tick_mark: secondary_value_axis_options.minor_tick_mark,
        secondary_value_axis_major_unit: secondary_value_axis_options.major_unit,
        secondary_value_axis_minor_unit: secondary_value_axis_options.minor_unit,
        secondary_value_axis_number_format: secondary_value_axis_options.number_format,
    };
    let snapshot = presentation_chart_snapshot(&candidate);
    let validated = parse_presentation_chart(&snapshot, 1)
        .context("chart data is outside the canonical self-contained creation contract")?;
    if presentation_chart_xml(&validated)? != xml {
        return Err(anyhow!(
            "chart XML is not the byte-exact canonical form generated by ChatOS"
        ));
    }
    Ok((validated, snapshot))
}
