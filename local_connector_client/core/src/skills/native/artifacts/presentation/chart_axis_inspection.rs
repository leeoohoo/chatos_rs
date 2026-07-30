// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use super::chart_model::{
    PresentationChartAxisTickMark, PresentationChartValueAxisNumberFormat,
    PresentationChartValueAxisOptions,
};
use super::limits::{
    MAX_PPTX_CREATE_CHART_LOG_BASE, MAX_PPTX_CREATE_CHART_VALUE_ABS, MIN_PPTX_CREATE_CHART_LOG_BASE,
};
use super::model::{
    PptxChartAxisInspection, PptxChartGroupInspection, PptxChartInspection,
    PptxChartSeriesInspection,
};

pub(super) fn resolve_pptx_chart_series_value_axis(
    series: &PptxChartSeriesInspection,
    chart_groups: &[PptxChartGroupInspection],
    axes: &[PptxChartAxisInspection],
) -> String {
    if matches!(series.chart_type.as_str(), "pie" | "doughnut") {
        return "primary".to_string();
    }
    let Some(group) = chart_groups.get(series.chart_group_index) else {
        return "unknown".to_string();
    };
    if matches!(series.chart_type.as_str(), "scatter" | "bubble") {
        let Some(y_axis_id) = group.axis_ids.get(1) else {
            return "unknown".to_string();
        };
        let Some(position) = axes
            .iter()
            .find(|axis| {
                axis.axis_type == "value" && axis.axis_id.as_deref() == Some(y_axis_id.as_str())
            })
            .and_then(|axis| axis.position.as_deref())
        else {
            return "unknown".to_string();
        };
        return match position {
            "l" => "primary".to_string(),
            "r" => "secondary".to_string(),
            _ => "unknown".to_string(),
        };
    }
    let positions = group
        .axis_ids
        .iter()
        .filter_map(|axis_id| {
            axes.iter().find(|axis| {
                axis.axis_type == "value" && axis.axis_id.as_deref() == Some(axis_id.as_str())
            })
        })
        .filter_map(|axis| axis.position.as_deref())
        .collect::<BTreeSet<_>>();
    if positions.len() != 1 {
        return "unknown".to_string();
    }
    let horizontal_bar = group.chart_type == "bar" && group.bar_direction.as_deref() == Some("bar");
    match (horizontal_bar, positions.iter().next().copied()) {
        (false, Some("l")) | (true, Some("b")) => "primary".to_string(),
        (false, Some("r")) | (true, Some("t")) => "secondary".to_string(),
        _ => "unknown".to_string(),
    }
}

pub(super) fn pptx_chart_is_horizontal_bar(
    chart_types: &BTreeSet<String>,
    chart_groups: &[PptxChartGroupInspection],
) -> bool {
    chart_types.len() == 1
        && chart_types.contains("bar")
        && !chart_groups.is_empty()
        && chart_groups
            .iter()
            .all(|group| group.chart_type == "bar" && group.bar_direction.as_deref() == Some("bar"))
}

pub(super) fn pptx_chart_inspection_is_horizontal_bar(inspection: &PptxChartInspection) -> bool {
    inspection.chart_types.len() == 1
        && inspection.chart_types[0] == "bar"
        && !inspection.chart_groups.is_empty()
        && inspection
            .chart_groups
            .iter()
            .all(|group| group.chart_type == "bar" && group.bar_direction.as_deref() == Some("bar"))
}

pub(super) fn pptx_chart_axis_title_fields(
    axis: Option<&PptxChartAxisInspection>,
) -> (String, Option<String>, bool) {
    axis.map(|axis| {
        (
            axis.title.clone(),
            axis.title_formula.clone(),
            axis.title_truncated,
        )
    })
    .unwrap_or_default()
}

pub(super) fn pptx_chart_value_axis_by_position<'a>(
    axes: &'a [PptxChartAxisInspection],
    position: &str,
) -> Option<&'a PptxChartAxisInspection> {
    axes.iter()
        .find(|axis| axis.axis_type == "value" && axis.position.as_deref() == Some(position))
}

pub(super) fn insert_pptx_chart_axis_metadata(
    metadata: &mut serde_json::Map<String, Value>,
    prefix: &str,
    axis: Option<&PptxChartAxisInspection>,
) {
    metadata.insert(
        format!("{prefix}_minimum"),
        json!(axis.and_then(|axis| axis.minimum.clone())),
    );
    metadata.insert(
        format!("{prefix}_maximum"),
        json!(axis.and_then(|axis| axis.maximum.clone())),
    );
    metadata.insert(
        format!("{prefix}_log_base"),
        json!(axis.and_then(|axis| axis.log_base.clone())),
    );
    metadata.insert(
        format!("{prefix}_major_tick_mark"),
        json!(pptx_chart_axis_tick_mark_name(axis, true)),
    );
    metadata.insert(
        format!("{prefix}_major_tick_mark_value"),
        json!(axis.and_then(|axis| axis.major_tick_mark.clone())),
    );
    metadata.insert(
        format!("{prefix}_minor_tick_mark"),
        json!(pptx_chart_axis_tick_mark_name(axis, false)),
    );
    metadata.insert(
        format!("{prefix}_minor_tick_mark_value"),
        json!(axis.and_then(|axis| axis.minor_tick_mark.clone())),
    );
    metadata.insert(
        format!("{prefix}_major_unit"),
        json!(axis.and_then(|axis| axis.major_unit.clone())),
    );
    metadata.insert(
        format!("{prefix}_minor_unit"),
        json!(axis.and_then(|axis| axis.minor_unit.clone())),
    );
    metadata.insert(
        format!("{prefix}_number_format"),
        json!(pptx_chart_axis_number_format_name(axis)),
    );
    metadata.insert(
        format!("{prefix}_number_format_code"),
        json!(axis.and_then(|axis| axis.number_format_code.clone())),
    );
    metadata.insert(
        format!("{prefix}_number_format_source_linked"),
        json!(axis.and_then(|axis| axis.number_format_source_linked)),
    );
}

pub(super) fn pptx_chart_axis_number_format_name(
    axis: Option<&PptxChartAxisInspection>,
) -> Option<&'static str> {
    let axis = axis?;
    match (
        axis.number_format_code.as_deref(),
        axis.number_format_source_linked,
    ) {
        (None, None) => None,
        (Some(format_code), Some(source_linked)) => Some(
            PresentationChartValueAxisNumberFormat::from_ooxml(format_code, source_linked)
                .map(PresentationChartValueAxisNumberFormat::as_str)
                .unwrap_or("custom"),
        ),
        _ => Some("custom"),
    }
}

pub(super) fn pptx_chart_axis_tick_mark_name(
    axis: Option<&PptxChartAxisInspection>,
    major: bool,
) -> Option<&'static str> {
    let axis = axis?;
    let value = if major {
        axis.major_tick_mark.as_deref()
    } else {
        axis.minor_tick_mark.as_deref()
    };
    Some(match value {
        None => PresentationChartAxisTickMark::None.as_str(),
        Some(value) => PresentationChartAxisTickMark::from_ooxml(value)
            .map(PresentationChartAxisTickMark::as_str)
            .unwrap_or("custom"),
    })
}

pub(super) fn canonical_pptx_chart_axis_options(
    axis: &PptxChartAxisInspection,
    label: &str,
) -> Result<PresentationChartValueAxisOptions> {
    let parse_bound = |value: Option<&String>, field: &str| -> Result<Option<f64>> {
        value
            .map(|value| {
                let parsed = value.parse::<f64>().with_context(|| {
                    format!("canonical {label} value-axis {field} must be a finite number")
                })?;
                if !parsed.is_finite() || parsed.abs() > MAX_PPTX_CREATE_CHART_VALUE_ABS {
                    return Err(anyhow!(
                        "canonical {label} value-axis {field} exceeds the numeric safety limit"
                    ));
                }
                Ok(parsed)
            })
            .transpose()
    };
    let minimum = parse_bound(axis.minimum.as_ref(), "minimum")?;
    let maximum = parse_bound(axis.maximum.as_ref(), "maximum")?;
    let log_base = parse_bound(axis.log_base.as_ref(), "log base")?;
    if log_base.is_some_and(|value| {
        !(MIN_PPTX_CREATE_CHART_LOG_BASE..=MAX_PPTX_CREATE_CHART_LOG_BASE).contains(&value)
    }) {
        return Err(anyhow!(
            "canonical {label} value-axis log base must be between {MIN_PPTX_CREATE_CHART_LOG_BASE} and {MAX_PPTX_CREATE_CHART_LOG_BASE}"
        ));
    }
    let major_tick_mark = axis
        .major_tick_mark
        .as_deref()
        .map(PresentationChartAxisTickMark::from_ooxml)
        .transpose()
        .with_context(|| format!("canonical {label} value-axis major tick mark is unsupported"))?
        .unwrap_or(PresentationChartAxisTickMark::None);
    let minor_tick_mark = axis
        .minor_tick_mark
        .as_deref()
        .map(PresentationChartAxisTickMark::from_ooxml)
        .transpose()
        .with_context(|| format!("canonical {label} value-axis minor tick mark is unsupported"))?
        .unwrap_or(PresentationChartAxisTickMark::None);
    let major_unit = parse_bound(axis.major_unit.as_ref(), "major unit")?;
    let minor_unit = parse_bound(axis.minor_unit.as_ref(), "minor unit")?;
    if major_unit.is_some_and(|value| value <= 0.0) || minor_unit.is_some_and(|value| value <= 0.0)
    {
        return Err(anyhow!(
            "canonical {label} value-axis units must be positive"
        ));
    }
    let format_code = axis.number_format_code.as_deref().ok_or_else(|| {
        anyhow!("canonical {label} value axis requires one explicit number format")
    })?;
    let source_linked = axis
        .number_format_source_linked
        .ok_or_else(|| anyhow!("canonical {label} value axis requires sourceLinked metadata"))?;
    let number_format =
        PresentationChartValueAxisNumberFormat::from_ooxml(format_code, source_linked)
            .with_context(|| {
                format!("canonical {label} value-axis number format is unsupported")
            })?;
    Ok(PresentationChartValueAxisOptions {
        minimum,
        maximum,
        log_base,
        major_tick_mark,
        minor_tick_mark,
        major_unit,
        minor_unit,
        number_format,
    })
}
