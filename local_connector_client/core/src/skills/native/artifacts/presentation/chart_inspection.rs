// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::fs::File;

use anyhow::{anyhow, Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::chart_axis_inspection::{
    insert_pptx_chart_axis_metadata, pptx_chart_inspection_is_horizontal_bar,
    pptx_chart_value_axis_by_position,
};
use super::chart_context_inspection::pptx_chart_series_json;
use super::chart_event_inspection::{
    record_pptx_chart_empty_event, record_pptx_chart_end_event, record_pptx_chart_start_event,
    record_pptx_chart_text_event, PptxChartEventInspectionState,
};
use super::chart_model::PresentationChartLegendPosition;
use super::chart_package_inspection::{
    ensure_standard_pptx_chart_content_type, inspect_pptx_chart_ownership,
    inspect_pptx_chart_relationships,
};
use super::chart_result_inspection::finalize_pptx_chart_inspection;
use super::chart_snapshot::canonical_pptx_chart_snapshot;
use super::model::PptxChartInspection;
use super::package_io::validate_pptx_package;
use super::slide_selection::selected_slide_positions;
use super::{file_size, input_file, read_zip_text, required_text};

pub(super) fn inspect_pptx_charts(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (path, relative) = input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let names = validate_pptx_package(path.as_path())?;
    for required in [
        "[Content_Types].xml",
        "ppt/presentation.xml",
        "ppt/_rels/presentation.xml.rels",
    ] {
        if !names.contains(required) {
            return Err(anyhow!("PPTX is missing required package part: {required}"));
        }
    }
    let mut archive = ZipArchive::new(File::open(path.as_path())?)
        .with_context(|| format!("open PPTX {}", path.display()))?;
    let content_types_xml = read_zip_text(&mut archive, "[Content_Types].xml")?;
    let ownership = inspect_pptx_chart_ownership(&mut archive, &names)?;
    let selected_slide_numbers =
        selected_slide_positions(arguments, ownership.ordered_slide_paths.len())?;
    let selected_slide_set = selected_slide_numbers
        .iter()
        .copied()
        .collect::<HashSet<_>>();

    let mut chart_metadata = Vec::new();
    let mut editable = false;
    for (slide_index, references) in ownership.charts_by_slide.iter().enumerate() {
        let slide_number = slide_index + 1;
        if !selected_slide_set.contains(&slide_number) {
            continue;
        }
        for (chart_index, reference) in references.iter().enumerate() {
            ensure_standard_pptx_chart_content_type(
                content_types_xml.as_str(),
                reference.part.as_str(),
            )?;
            let chart_xml = read_zip_text(&mut archive, reference.part.as_str())?;
            let chart = inspect_standard_pptx_chart_xml(chart_xml.as_str())?;
            let relationships =
                inspect_pptx_chart_relationships(&mut archive, &names, reference.part.as_str())?;
            let (eligible, snapshot, unsupported_reason) =
                match canonical_pptx_chart_snapshot(&chart, &relationships, chart_xml.as_str()) {
                    Ok((_, snapshot)) => (true, snapshot, Value::Null),
                    Err(error) => (false, Value::Null, json!(error.to_string())),
                };
            editable |= eligible;
            let series = chart
                .series
                .iter()
                .map(pptx_chart_series_json)
                .collect::<Vec<_>>();
            let legend_position = chart.legend_positions.first().and_then(|value| {
                PresentationChartLegendPosition::from_ooxml(value.as_str())
                    .ok()
                    .map(PresentationChartLegendPosition::as_str)
            });
            let data_labels = match (
                chart.data_label_group_count,
                chart.data_label_show_value_count,
                chart.data_label_show_percentage_count,
            ) {
                (0, 0, 0) => "none",
                (groups, values, 0)
                    if groups == chart.chart_groups.len() && values == chart.chart_groups.len() =>
                {
                    "value"
                }
                (groups, 0, percentages)
                    if groups == chart.chart_groups.len()
                        && percentages == chart.chart_groups.len() =>
                {
                    "percentage"
                }
                _ => "custom",
            };
            let secondary_axis_series = chart
                .series
                .iter()
                .enumerate()
                .filter(|(_, series)| series.value_axis == "secondary")
                .map(|(index, _)| index + 1)
                .collect::<Vec<_>>();
            let horizontal_bar = pptx_chart_inspection_is_horizontal_bar(&chart);
            let numeric_x = chart.chart_types.len() == 1
                && matches!(chart.chart_types[0].as_str(), "scatter" | "bubble");
            let x_axis = numeric_x
                .then(|| pptx_chart_value_axis_by_position(chart.axes.as_slice(), "b"))
                .flatten();
            let secondary_x_axis = numeric_x
                .then(|| pptx_chart_value_axis_by_position(chart.axes.as_slice(), "t"))
                .flatten();
            let value_axis_position = if horizontal_bar { "b" } else { "l" };
            let secondary_value_axis_position = if horizontal_bar { "t" } else { "r" };
            let value_axis =
                pptx_chart_value_axis_by_position(chart.axes.as_slice(), value_axis_position);
            let secondary_value_axis = pptx_chart_value_axis_by_position(
                chart.axes.as_slice(),
                secondary_value_axis_position,
            );
            let bar_directions = chart
                .chart_groups
                .iter()
                .filter(|group| group.chart_type == "bar")
                .map(|group| group.bar_direction.clone())
                .collect::<Vec<_>>();
            let radar_styles = chart
                .chart_groups
                .iter()
                .filter(|group| group.chart_type == "radar")
                .map(|group| group.radar_style.clone())
                .collect::<Vec<_>>();
            let scatter_styles = chart
                .chart_groups
                .iter()
                .filter(|group| group.chart_type == "scatter")
                .map(|group| group.scatter_style.clone())
                .collect::<Vec<_>>();
            let bubble_scales = chart
                .chart_groups
                .iter()
                .filter(|group| group.chart_type == "bubble")
                .map(|group| group.bubble_scale.clone())
                .collect::<Vec<_>>();
            let show_negative_bubbles = chart
                .chart_groups
                .iter()
                .filter(|group| group.chart_type == "bubble")
                .map(|group| group.show_negative_bubbles.clone())
                .collect::<Vec<_>>();
            let bubble_size_represents = chart
                .chart_groups
                .iter()
                .filter(|group| group.chart_type == "bubble")
                .map(|group| group.bubble_size_represents.clone())
                .collect::<Vec<_>>();
            let bubble_3d = chart
                .chart_groups
                .iter()
                .filter(|group| group.chart_type == "bubble")
                .map(|group| group.bubble_3d.clone())
                .collect::<Vec<_>>();
            let mut metadata = json!({
                "slide_number": slide_number,
                "chart_number": chart_index + 1,
                "relationship_id": reference.relationship_id,
                "part": reference.part,
                "chart_xml_sha256": hex::encode(Sha256::digest(chart_xml.as_bytes())),
                "chart_types": chart.chart_types,
                "bar_directions": bar_directions,
                "radar_styles": radar_styles,
                "scatter_styles": scatter_styles,
                "chart_group_count": chart.chart_groups.len(),
                "axis_count": chart.axes.len(),
                "title": chart.title,
                "title_formula": chart.title_formula,
                "title_truncated": chart.title_truncated,
                "show_legend": chart.legend_count == 1,
                "legend_position": legend_position,
                "data_labels": data_labels,
                "data_label_group_count": chart.data_label_group_count,
                "category_axis_title": chart.category_axis_title,
                "category_axis_title_formula": chart.category_axis_title_formula,
                "category_axis_title_truncated": chart.category_axis_title_truncated,
                "value_axis_title": chart.value_axis_title,
                "value_axis_title_formula": chart.value_axis_title_formula,
                "value_axis_title_truncated": chart.value_axis_title_truncated,
                "secondary_value_axis_title": chart.secondary_value_axis_title,
                "secondary_value_axis_title_formula": chart.secondary_value_axis_title_formula,
                "secondary_value_axis_title_truncated": chart.secondary_value_axis_title_truncated,
                "secondary_axis_series": secondary_axis_series,
                "series_count": chart.series.len(),
                "cached_points": chart.cached_points,
                "series": series,
                "data_source": relationships.data_source,
                "relationship_count": relationships.relationship_count,
                "embedded_workbook": relationships.embedded_workbook,
                "eligible_for_self_contained_chart_replacement": eligible,
                "self_contained_edit_snapshot": snapshot,
                "self_contained_replacement_unsupported_reason": unsupported_reason,
                "read_only": !eligible,
            });
            let metadata_object = metadata
                .as_object_mut()
                .ok_or_else(|| anyhow!("PPTX chart metadata must be an object"))?;
            metadata_object.insert("bubble_scales".to_string(), json!(bubble_scales));
            metadata_object.insert(
                "show_negative_bubbles".to_string(),
                json!(show_negative_bubbles),
            );
            metadata_object.insert(
                "bubble_size_represents".to_string(),
                json!(bubble_size_represents),
            );
            metadata_object.insert("bubble_3d".to_string(), json!(bubble_3d));
            insert_pptx_chart_axis_metadata(metadata_object, "x_axis", x_axis);
            insert_pptx_chart_axis_metadata(metadata_object, "secondary_x_axis", secondary_x_axis);
            insert_pptx_chart_axis_metadata(metadata_object, "value_axis", value_axis);
            insert_pptx_chart_axis_metadata(
                metadata_object,
                "secondary_value_axis",
                secondary_value_axis,
            );
            chart_metadata.push(metadata);
        }
    }
    Ok(json!({
        "path": relative,
        "bytes": file_size(path.as_path())?,
        "slides": ownership.ordered_slide_paths.len(),
        "selected_slide_numbers": selected_slide_numbers,
        "charts": ownership.chart_count,
        "selected_charts": chart_metadata.len(),
        "chart_metadata": chart_metadata,
        "standard_drawingml_only": true,
        "embedded_workbooks_opened": false,
        "editable": editable,
    }))
}

pub(super) fn inspect_standard_pptx_chart_xml(xml: &str) -> Result<PptxChartInspection> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut state = PptxChartEventInspectionState::default();
    loop {
        match reader.read_event().context("inspect PPTX chart XML")? {
            Event::Start(event) => record_pptx_chart_start_event(&reader, &event, &mut state)?,
            Event::Empty(event) => record_pptx_chart_empty_event(&reader, &event, &mut state)?,
            Event::Text(text) => record_pptx_chart_text_event(text, &mut state)?,
            Event::End(event) => record_pptx_chart_end_event(&event, &mut state)?,
            Event::DocType(_) | Event::PI(_) | Event::CData(_) => {
                return Err(anyhow!(
                    "PPTX chart inspection does not support declarations, processing instructions, or CDATA"
                ));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(finalize_pptx_chart_inspection(state.into_parts()?))
}
