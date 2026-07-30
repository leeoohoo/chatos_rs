// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};
use quick_xml::events::BytesStart;
use quick_xml::Reader;
use serde_json::{json, Value};

use super::chart_series_style_inspection::{
    pptx_chart_series_marker_index, pptx_chart_series_shape_properties_index,
};
use super::limits::{
    MAX_PPTX_CHART_FORMULA_CHARS, MAX_PPTX_CHART_POINTS, MAX_PPTX_CHART_PREVIEW_POINTS,
    MAX_PPTX_CHART_TEXT_CHARS,
};
use super::model::{PptxChartAxisInspection, PptxChartSeriesInspection};
use super::optional_xml_attribute;

pub(super) struct PptxChartTextInspectionState<'a> {
    current_series: &'a mut Option<PptxChartSeriesInspection>,
    current_axis: &'a mut Option<PptxChartAxisInspection>,
    title: &'a mut String,
    title_formula: &'a mut Option<String>,
    cached_points: &'a mut usize,
    total_chars: &'a mut usize,
}

impl<'a> PptxChartTextInspectionState<'a> {
    pub(super) fn new(
        current_series: &'a mut Option<PptxChartSeriesInspection>,
        current_axis: &'a mut Option<PptxChartAxisInspection>,
        title: &'a mut String,
        title_formula: &'a mut Option<String>,
        cached_points: &'a mut usize,
        total_chars: &'a mut usize,
    ) -> Self {
        Self {
            current_series,
            current_axis,
            title,
            title_formula,
            cached_points,
            total_chars,
        }
    }
}

pub(super) fn record_pptx_chart_text(
    value: String,
    stack: &[String],
    state: &mut PptxChartTextInspectionState<'_>,
) -> Result<()> {
    let Some(current) = stack.last().map(String::as_str) else {
        if !value.trim().is_empty() {
            return Err(anyhow!("PPTX chart contains text outside its root"));
        }
        return Ok(());
    };
    if let Some(series) = state.current_series.as_mut() {
        if pptx_chart_series_shape_properties_index(stack).is_some() && !value.trim().is_empty() {
            series.color_style_custom = true;
        }
        if pptx_chart_series_marker_index(stack).is_some() && !value.trim().is_empty() {
            series.marker_style_custom = true;
        }
    }
    if current == "t" && state.current_series.is_none() && pptx_chart_is_chart_level_title(stack) {
        *state.total_chars = add_pptx_chart_text_chars(*state.total_chars, value.as_str())?;
        state.title.push_str(value.as_str());
    } else if current == "t" && state.current_series.is_none() {
        if let Some(axis_type) = pptx_chart_axis_title_context(stack) {
            *state.total_chars = add_pptx_chart_text_chars(*state.total_chars, value.as_str())?;
            let axis = state.current_axis.as_mut().ok_or_else(|| {
                anyhow!("PPTX chart axis title is outside a category or value axis")
            })?;
            if axis.axis_type != axis_type {
                return Err(anyhow!("PPTX chart axis title context is inconsistent"));
            }
            axis.title.push_str(value.as_str());
        }
    } else if current == "f" {
        record_pptx_chart_formula(value.as_str(), stack, state)?;
    } else if current == "v" {
        record_pptx_chart_cached_value(value, stack, state)?;
    }
    Ok(())
}

fn record_pptx_chart_formula(
    value: &str,
    stack: &[String],
    state: &mut PptxChartTextInspectionState<'_>,
) -> Result<()> {
    let formula = value.trim();
    if formula.is_empty() || formula.chars().count() > MAX_PPTX_CHART_FORMULA_CHARS {
        return Err(anyhow!(
            "PPTX chart formula is empty or exceeds the safety limit"
        ));
    }
    *state.total_chars = add_pptx_chart_text_chars(*state.total_chars, formula)?;
    if state.current_series.is_none() && pptx_chart_is_chart_level_title(stack) {
        return set_unique_pptx_chart_formula(state.title_formula, formula);
    }
    if state.current_series.is_none() {
        if let Some(axis_type) = pptx_chart_axis_title_context(stack) {
            let axis = state.current_axis.as_mut().ok_or_else(|| {
                anyhow!("PPTX chart axis title formula is outside a category or value axis")
            })?;
            if axis.axis_type != axis_type {
                return Err(anyhow!(
                    "PPTX chart axis title formula context is inconsistent"
                ));
            }
            set_unique_pptx_chart_formula(&mut axis.title_formula, formula)?;
        }
        return Ok(());
    }
    let context = pptx_chart_series_data_context(stack)
        .ok_or_else(|| anyhow!("PPTX chart formula is outside a supported series field"))?;
    let series = state
        .current_series
        .as_mut()
        .ok_or_else(|| anyhow!("PPTX chart formula is outside a series"))?;
    let slot = match context {
        "tx" => &mut series.name_formula,
        "cat" | "xVal" => &mut series.category_formula,
        "val" | "yVal" => &mut series.value_formula,
        "bubbleSize" => &mut series.bubble_size_formula,
        _ => {
            return Err(anyhow!(
                "PPTX chart formula context dispatch is inconsistent"
            ));
        }
    };
    set_unique_pptx_chart_formula(slot, formula)
}

fn record_pptx_chart_cached_value(
    value: String,
    stack: &[String],
    state: &mut PptxChartTextInspectionState<'_>,
) -> Result<()> {
    if state.current_series.is_none() && pptx_chart_is_chart_level_title(stack) {
        *state.total_chars = add_pptx_chart_text_chars(*state.total_chars, value.as_str())?;
        state.title.push_str(value.as_str());
        return Ok(());
    }
    if state.current_series.is_none() {
        if let Some(axis_type) = pptx_chart_axis_title_context(stack) {
            *state.total_chars = add_pptx_chart_text_chars(*state.total_chars, value.as_str())?;
            let axis = state.current_axis.as_mut().ok_or_else(|| {
                anyhow!("PPTX chart cached axis title is outside a category or value axis")
            })?;
            if axis.axis_type != axis_type {
                return Err(anyhow!(
                    "PPTX chart cached axis title context is inconsistent"
                ));
            }
            axis.title.push_str(value.as_str());
        }
        return Ok(());
    }
    let context = pptx_chart_series_data_context(stack)
        .ok_or_else(|| anyhow!("PPTX chart cache value is outside a supported series field"))?;
    *state.total_chars = add_pptx_chart_text_chars(*state.total_chars, value.as_str())?;
    let series = state
        .current_series
        .as_mut()
        .ok_or_else(|| anyhow!("PPTX chart cache value is outside a series"))?;
    match context {
        "tx" => {
            if !series.name.is_empty() {
                return Err(anyhow!("PPTX chart series contains multiple cached names"));
            }
            series.name = value;
        }
        "cat" | "xVal" => {
            series.categories.push(value);
            *state.cached_points = state.cached_points.saturating_add(1);
        }
        "val" | "yVal" => {
            series.values.push(value);
            *state.cached_points = state.cached_points.saturating_add(1);
        }
        "bubbleSize" => {
            series.bubble_sizes.push(value);
            *state.cached_points = state.cached_points.saturating_add(1);
        }
        _ => {
            return Err(anyhow!(
                "PPTX chart cache value context dispatch is inconsistent"
            ));
        }
    }
    if *state.cached_points > MAX_PPTX_CHART_POINTS {
        return Err(anyhow!(
            "PPTX chart cached points exceed the {MAX_PPTX_CHART_POINTS} item safety limit"
        ));
    }
    Ok(())
}

pub(super) fn pptx_chart_series_data_context(stack: &[String]) -> Option<&str> {
    for item in stack.iter().rev() {
        match item.as_str() {
            "tx" | "cat" | "val" | "xVal" | "yVal" | "bubbleSize" => {
                return Some(item.as_str());
            }
            "ser" => break,
            _ => {}
        }
    }
    None
}

pub(super) fn pptx_chart_is_chart_level_title(stack: &[String]) -> bool {
    stack.iter().any(|item| item == "title") && !stack.iter().any(|item| item == "plotArea")
}

pub(super) fn pptx_chart_axis_title_context(stack: &[String]) -> Option<&'static str> {
    let title_index = stack.iter().rposition(|item| item == "title")?;
    if !stack[..title_index].iter().any(|item| item == "plotArea") {
        return None;
    }
    stack[..title_index]
        .iter()
        .rev()
        .find_map(|item| match item.as_str() {
            "catAx" => Some("category"),
            "valAx" => Some("value"),
            _ => None,
        })
}

pub(super) fn pptx_chart_boolean_attribute(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    label: &str,
) -> Result<bool> {
    match optional_xml_attribute(reader, event, "val")?.as_deref() {
        None | Some("1" | "true") => Ok(true),
        Some("0" | "false") => Ok(false),
        Some(value) => Err(anyhow!(
            "PPTX chart {label} contains an invalid boolean value: {value}"
        )),
    }
}

pub(super) fn set_unique_pptx_chart_formula(
    slot: &mut Option<String>,
    formula: &str,
) -> Result<()> {
    if slot.replace(formula.to_string()).is_some() {
        return Err(anyhow!(
            "PPTX chart series contains multiple formulas for one field"
        ));
    }
    Ok(())
}

pub(super) fn add_pptx_chart_text_chars(current: usize, value: &str) -> Result<usize> {
    let updated = current
        .checked_add(value.chars().count())
        .ok_or_else(|| anyhow!("PPTX chart text size overflow"))?;
    if updated > MAX_PPTX_CHART_TEXT_CHARS {
        return Err(anyhow!(
            "PPTX chart text exceeds the {MAX_PPTX_CHART_TEXT_CHARS} character safety limit"
        ));
    }
    Ok(updated)
}

pub(super) fn pptx_chart_series_json(series: &PptxChartSeriesInspection) -> Value {
    let numeric_x = matches!(series.chart_type.as_str(), "scatter" | "bubble");
    let categories_truncated = series.categories.len() > MAX_PPTX_CHART_PREVIEW_POINTS;
    let values_truncated = series.values.len() > MAX_PPTX_CHART_PREVIEW_POINTS;
    let bubble_sizes_truncated = series.bubble_sizes.len() > MAX_PPTX_CHART_PREVIEW_POINTS;
    let color = if series.color_style_custom {
        Some("custom".to_string())
    } else {
        series.color.clone()
    };
    let marker_style = if series.marker_style_custom {
        Some("custom".to_string())
    } else {
        series.marker_style.clone()
    };
    let smooth = if series.smooth_custom {
        json!("custom")
    } else {
        series.smooth.map(Value::Bool).unwrap_or(Value::Null)
    };
    json!({
        "chart_type": series.chart_type,
        "chart_group": series.chart_group_index + 1,
        "value_axis": series.value_axis,
        "color": color,
        "color_value": series.color_value,
        "marker_style": marker_style,
        "marker_style_value": series.marker_style_value,
        "marker_size": if series.marker_style_custom { None } else { series.marker_size },
        "marker_size_value": series.marker_size_value,
        "smooth": smooth,
        "smooth_value": series.smooth_value,
        "name": series.name,
        "name_formula": series.name_formula,
        "category_formula": series.category_formula,
        "value_formula": series.value_formula,
        "bubble_size_formula": series.bubble_size_formula,
        "cached_category_points": series.categories.len(),
        "cached_value_points": series.values.len(),
        "cached_x_value_points": if numeric_x { series.categories.len() } else { 0 },
        "cached_y_value_points": if numeric_x { series.values.len() } else { 0 },
        "cached_bubble_size_points": series.bubble_sizes.len(),
        "categories_preview": series.categories.iter().take(MAX_PPTX_CHART_PREVIEW_POINTS).collect::<Vec<_>>(),
        "values_preview": series.values.iter().take(MAX_PPTX_CHART_PREVIEW_POINTS).collect::<Vec<_>>(),
        "x_values_preview": if numeric_x { series.categories.iter().take(MAX_PPTX_CHART_PREVIEW_POINTS).collect::<Vec<_>>() } else { Vec::<&String>::new() },
        "y_values_preview": if numeric_x { series.values.iter().take(MAX_PPTX_CHART_PREVIEW_POINTS).collect::<Vec<_>>() } else { Vec::<&String>::new() },
        "bubble_sizes_preview": series.bubble_sizes.iter().take(MAX_PPTX_CHART_PREVIEW_POINTS).collect::<Vec<_>>(),
        "preview_truncated": categories_truncated || values_truncated || bubble_sizes_truncated,
    })
}
