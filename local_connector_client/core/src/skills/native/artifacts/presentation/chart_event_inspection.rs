// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use anyhow::{anyhow, Context, Result};
use quick_xml::events::{BytesEnd, BytesStart, BytesText};
use quick_xml::{Reader, XmlVersion};

use super::chart_context_inspection::{
    pptx_chart_boolean_attribute, record_pptx_chart_text, PptxChartTextInspectionState,
};
use super::chart_result_inspection::PptxChartInspectionParts;
use super::chart_series_style_inspection::{
    finalize_pptx_chart_series_color, finalize_pptx_chart_series_marker,
    finalize_pptx_chart_series_smooth, record_pptx_chart_series_color_element,
    record_pptx_chart_series_marker_element, record_pptx_chart_series_smooth_element,
};
use super::chart_structure_inspection::{
    ensure_standard_pptx_chart_namespace, record_pptx_chart_structure_element,
    standard_pptx_chart_type,
};
use super::limits::MAX_PPTX_CHART_SERIES;
use super::model::{PptxChartAxisInspection, PptxChartGroupInspection, PptxChartSeriesInspection};
use super::required_xml_attribute;

#[derive(Default)]
pub(super) struct PptxChartEventInspectionState {
    stack: Vec<String>,
    root_count: usize,
    chart_count: usize,
    plot_area_count: usize,
    chart_types: BTreeSet<String>,
    chart_groups: Vec<PptxChartGroupInspection>,
    current_chart_group: Option<PptxChartGroupInspection>,
    axes: Vec<PptxChartAxisInspection>,
    current_axis: Option<PptxChartAxisInspection>,
    title: String,
    title_formula: Option<String>,
    series: Vec<PptxChartSeriesInspection>,
    current_series: Option<PptxChartSeriesInspection>,
    cached_points: usize,
    legend_count: usize,
    legend_positions: Vec<String>,
    data_label_group_count: usize,
    data_label_show_value_count: usize,
    data_label_show_percentage_count: usize,
    total_chars: usize,
}

impl PptxChartEventInspectionState {
    pub(super) fn into_parts(self) -> Result<PptxChartInspectionParts> {
        if self.root_count != 1
            || self.chart_count != 1
            || self.plot_area_count != 1
            || self.chart_types.is_empty()
            || self.current_series.is_some()
            || self.current_chart_group.is_some()
            || self.current_axis.is_some()
            || !self.stack.is_empty()
        {
            return Err(anyhow!(
                "PPTX chart must contain one complete chart, one plot area, and at least one supported chart type"
            ));
        }
        Ok(PptxChartInspectionParts {
            chart_types: self.chart_types,
            chart_groups: self.chart_groups,
            axes: self.axes,
            title: self.title,
            title_formula: self.title_formula,
            series: self.series,
            cached_points: self.cached_points,
            legend_count: self.legend_count,
            legend_positions: self.legend_positions,
            data_label_group_count: self.data_label_group_count,
            data_label_show_value_count: self.data_label_show_value_count,
            data_label_show_percentage_count: self.data_label_show_percentage_count,
        })
    }
}

pub(super) fn record_pptx_chart_start_event(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    state: &mut PptxChartEventInspectionState,
) -> Result<()> {
    let qualified = event.name().as_ref().to_vec();
    let local = String::from_utf8_lossy(event.local_name().as_ref()).into_owned();
    ensure_standard_pptx_chart_namespace(qualified.as_slice(), local.as_str())?;
    if state.stack.is_empty() {
        if qualified.as_slice() != b"c:chartSpace" {
            return Err(anyhow!(
                "PPTX chart part requires a standard c:chartSpace root"
            ));
        }
        state.root_count = state.root_count.saturating_add(1);
    }
    if qualified.as_slice() == b"c:chart" {
        state.chart_count = state.chart_count.saturating_add(1);
    } else if qualified.as_slice() == b"c:plotArea" {
        state.plot_area_count = state.plot_area_count.saturating_add(1);
    }
    record_pptx_chart_metadata_element(reader, event, qualified.as_slice(), state)?;
    if let Some(chart_type) = standard_pptx_chart_type(qualified.as_slice()) {
        state.chart_types.insert(chart_type.to_string());
        if state.current_chart_group.is_some() {
            return Err(anyhow!("PPTX chart contains nested chart groups"));
        }
        state.current_chart_group = Some(PptxChartGroupInspection {
            chart_type: chart_type.to_string(),
            bar_direction: None,
            radar_style: None,
            scatter_style: None,
            bubble_scale: None,
            show_negative_bubbles: None,
            bubble_size_represents: None,
            bubble_3d: None,
            axis_ids: Vec::new(),
        });
    }
    if matches!(qualified.as_slice(), b"c:catAx" | b"c:valAx") {
        if state.current_axis.is_some() {
            return Err(anyhow!("PPTX chart contains nested axes"));
        }
        state.current_axis = Some(PptxChartAxisInspection {
            axis_type: if qualified.as_slice() == b"c:catAx" {
                "category".to_string()
            } else {
                "value".to_string()
            },
            ..PptxChartAxisInspection::default()
        });
    }
    record_pptx_chart_structure_element(
        reader,
        event,
        state.stack.as_slice(),
        state.current_chart_group.as_mut(),
        state.current_axis.as_mut(),
    )?;
    if qualified.as_slice() == b"c:ser" {
        if state.current_series.is_some() {
            return Err(anyhow!("PPTX chart contains nested series"));
        }
        let chart_group = state
            .current_chart_group
            .as_ref()
            .ok_or_else(|| anyhow!("PPTX chart series has no supported chart group"))?;
        state.current_series = Some(PptxChartSeriesInspection {
            chart_type: chart_group.chart_type.clone(),
            chart_group_index: state.chart_groups.len(),
            ..PptxChartSeriesInspection::default()
        });
    }
    record_pptx_chart_series_style_elements(reader, event, state, false)?;
    state.stack.push(local);
    if state.stack.len() > 256 {
        return Err(anyhow!("PPTX chart XML nesting exceeds the safety limit"));
    }
    Ok(())
}

pub(super) fn record_pptx_chart_empty_event(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    state: &mut PptxChartEventInspectionState,
) -> Result<()> {
    let qualified = event.name().as_ref().to_vec();
    let local = String::from_utf8_lossy(event.local_name().as_ref()).into_owned();
    ensure_standard_pptx_chart_namespace(qualified.as_slice(), local.as_str())?;
    if standard_pptx_chart_type(qualified.as_slice()).is_some() || qualified.as_slice() == b"c:ser"
    {
        return Err(anyhow!(
            "PPTX chart types and series must not be empty elements"
        ));
    }
    record_pptx_chart_metadata_element(reader, event, qualified.as_slice(), state)?;
    record_pptx_chart_structure_element(
        reader,
        event,
        state.stack.as_slice(),
        state.current_chart_group.as_mut(),
        state.current_axis.as_mut(),
    )?;
    record_pptx_chart_series_style_elements(reader, event, state, true)
}

pub(super) fn record_pptx_chart_text_event(
    text: BytesText<'_>,
    state: &mut PptxChartEventInspectionState,
) -> Result<()> {
    let value = text
        .xml_content(XmlVersion::Explicit1_0)
        .context("decode PPTX chart text")?
        .into_owned();
    let mut text_state = PptxChartTextInspectionState::new(
        &mut state.current_series,
        &mut state.current_axis,
        &mut state.title,
        &mut state.title_formula,
        &mut state.cached_points,
        &mut state.total_chars,
    );
    record_pptx_chart_text(value, state.stack.as_slice(), &mut text_state)
}

pub(super) fn record_pptx_chart_end_event(
    event: &BytesEnd<'_>,
    state: &mut PptxChartEventInspectionState,
) -> Result<()> {
    let local = String::from_utf8_lossy(event.local_name().as_ref()).into_owned();
    let expected = state
        .stack
        .pop()
        .ok_or_else(|| anyhow!("PPTX chart contains an unmatched closing tag"))?;
    if expected != local {
        return Err(anyhow!("PPTX chart contains mismatched element boundaries"));
    }
    if event.name().as_ref() == b"c:ser" {
        let mut item = state
            .current_series
            .take()
            .ok_or_else(|| anyhow!("PPTX chart series boundary is invalid"))?;
        finalize_pptx_chart_series_color(&mut item);
        finalize_pptx_chart_series_marker(&mut item);
        finalize_pptx_chart_series_smooth(&mut item);
        state.series.push(item);
        if state.series.len() > MAX_PPTX_CHART_SERIES {
            return Err(anyhow!(
                "PPTX chart series exceed the {MAX_PPTX_CHART_SERIES} item safety limit"
            ));
        }
    }
    if matches!(event.name().as_ref(), b"c:catAx" | b"c:valAx") {
        let mut axis = state
            .current_axis
            .take()
            .ok_or_else(|| anyhow!("PPTX chart axis boundary is invalid"))?;
        axis.title_truncated = axis.title.chars().count() > 1_000;
        axis.title = axis.title.chars().take(1_000).collect();
        state.axes.push(axis);
        if state.axes.len() > 16 {
            return Err(anyhow!("PPTX chart axes exceed the safety limit"));
        }
    }
    if let Some(chart_type) = standard_pptx_chart_type(event.name().as_ref()) {
        let chart_group = state
            .current_chart_group
            .take()
            .ok_or_else(|| anyhow!("PPTX chart group boundary is invalid"))?;
        if chart_group.chart_type != chart_type {
            return Err(anyhow!("PPTX chart group type boundary is inconsistent"));
        }
        state.chart_groups.push(chart_group);
        if state.chart_groups.len() > 16 {
            return Err(anyhow!("PPTX chart groups exceed the safety limit"));
        }
    }
    Ok(())
}

fn record_pptx_chart_metadata_element(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    qualified: &[u8],
    state: &mut PptxChartEventInspectionState,
) -> Result<()> {
    if qualified == b"c:legend" {
        state.legend_count = state.legend_count.saturating_add(1);
    } else if qualified == b"c:dLbls" {
        state.data_label_group_count = state.data_label_group_count.saturating_add(1);
    } else if qualified == b"c:legendPos" {
        if state.stack.last().map(String::as_str) != Some("legend") {
            return Err(anyhow!(
                "PPTX chart legend position must be a direct legend child"
            ));
        }
        state
            .legend_positions
            .push(required_xml_attribute(reader, event, "val")?);
    } else if qualified == b"c:showVal"
        && state.stack.last().map(String::as_str) == Some("dLbls")
        && pptx_chart_boolean_attribute(reader, event, "showVal")?
    {
        state.data_label_show_value_count = state.data_label_show_value_count.saturating_add(1);
    } else if qualified == b"c:showPercent"
        && state.stack.last().map(String::as_str) == Some("dLbls")
        && pptx_chart_boolean_attribute(reader, event, "showPercent")?
    {
        state.data_label_show_percentage_count =
            state.data_label_show_percentage_count.saturating_add(1);
    }
    Ok(())
}

fn record_pptx_chart_series_style_elements(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    state: &mut PptxChartEventInspectionState,
    empty: bool,
) -> Result<()> {
    record_pptx_chart_series_color_element(
        reader,
        event,
        state.stack.as_slice(),
        state.current_series.as_mut(),
        empty,
    )?;
    record_pptx_chart_series_marker_element(
        reader,
        event,
        state.stack.as_slice(),
        state.current_series.as_mut(),
        empty,
    )?;
    record_pptx_chart_series_smooth_element(
        reader,
        event,
        state.stack.as_slice(),
        state.current_series.as_mut(),
        empty,
    )
}
