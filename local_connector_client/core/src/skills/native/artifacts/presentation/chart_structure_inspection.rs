// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};
use quick_xml::events::BytesStart;
use quick_xml::Reader;

use super::model::{PptxChartAxisInspection, PptxChartGroupInspection};
use super::required_xml_attribute;

pub(super) fn record_pptx_chart_structure_element(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    stack: &[String],
    chart_group: Option<&mut PptxChartGroupInspection>,
    axis: Option<&mut PptxChartAxisInspection>,
) -> Result<()> {
    let qualified = event.name().as_ref().to_vec();
    let parent = stack.last().map(String::as_str);
    if qualified.as_slice() == b"c:barDir" && parent == Some("barChart") {
        let value = required_xml_attribute(reader, event, "val")?;
        if value.is_empty() || value.len() > 128 {
            return Err(anyhow!(
                "PPTX chart bar direction is empty or exceeds the safety limit"
            ));
        }
        let chart_group = chart_group
            .ok_or_else(|| anyhow!("PPTX chart bar direction is outside a bar chart group"))?;
        if chart_group.chart_type != "bar" {
            return Err(anyhow!(
                "PPTX chart bar direction is attached to a non-bar chart group"
            ));
        }
        if chart_group.bar_direction.replace(value).is_some() {
            return Err(anyhow!(
                "PPTX chart bar group contains multiple bar directions"
            ));
        }
    } else if qualified.as_slice() == b"c:radarStyle" && parent == Some("radarChart") {
        let value = required_xml_attribute(reader, event, "val")?;
        if value.is_empty() || value.len() > 128 {
            return Err(anyhow!(
                "PPTX chart radar style is empty or exceeds the safety limit"
            ));
        }
        let chart_group = chart_group
            .ok_or_else(|| anyhow!("PPTX chart radar style is outside a radar chart group"))?;
        if chart_group.chart_type != "radar" {
            return Err(anyhow!(
                "PPTX chart radar style is attached to a non-radar chart group"
            ));
        }
        if chart_group.radar_style.replace(value).is_some() {
            return Err(anyhow!(
                "PPTX chart radar group contains multiple radar styles"
            ));
        }
    } else if qualified.as_slice() == b"c:scatterStyle" && parent == Some("scatterChart") {
        let value = required_xml_attribute(reader, event, "val")?;
        if value.is_empty() || value.len() > 128 {
            return Err(anyhow!(
                "PPTX chart scatter style is empty or exceeds the safety limit"
            ));
        }
        let chart_group = chart_group
            .ok_or_else(|| anyhow!("PPTX chart scatter style is outside a scatter chart group"))?;
        if chart_group.chart_type != "scatter" {
            return Err(anyhow!(
                "PPTX chart scatter style is attached to a non-scatter chart group"
            ));
        }
        if chart_group.scatter_style.replace(value).is_some() {
            return Err(anyhow!(
                "PPTX chart scatter group contains multiple scatter styles"
            ));
        }
    } else if matches!(
        qualified.as_slice(),
        b"c:bubbleScale" | b"c:showNegBubbles" | b"c:sizeRepresents" | b"c:bubble3D"
    ) && parent == Some("bubbleChart")
    {
        let value = required_xml_attribute(reader, event, "val")?;
        if value.is_empty() || value.len() > 128 {
            return Err(anyhow!(
                "PPTX bubble chart group metadata is empty or exceeds the safety limit"
            ));
        }
        let chart_group = chart_group
            .ok_or_else(|| anyhow!("PPTX bubble metadata is outside a bubble chart group"))?;
        if chart_group.chart_type != "bubble" {
            return Err(anyhow!(
                "PPTX bubble metadata is attached to a non-bubble chart group"
            ));
        }
        let slot = match qualified.as_slice() {
            b"c:bubbleScale" => &mut chart_group.bubble_scale,
            b"c:showNegBubbles" => &mut chart_group.show_negative_bubbles,
            b"c:sizeRepresents" => &mut chart_group.bubble_size_represents,
            b"c:bubble3D" => &mut chart_group.bubble_3d,
            _ => {
                return Err(anyhow!(
                    "PPTX bubble chart metadata dispatch is inconsistent"
                ));
            }
        };
        if slot.replace(value).is_some() {
            return Err(anyhow!(
                "PPTX bubble chart group contains duplicate group metadata"
            ));
        }
    } else if qualified.as_slice() == b"c:axId" {
        let value = required_xml_attribute(reader, event, "val")?;
        if parent.and_then(standard_pptx_chart_type_local).is_some() {
            let chart_group = chart_group
                .ok_or_else(|| anyhow!("PPTX chart axis ID is outside a chart group"))?;
            if chart_group.axis_ids.contains(&value) {
                return Err(anyhow!("PPTX chart group contains duplicate axis IDs"));
            }
            chart_group.axis_ids.push(value);
            if chart_group.axis_ids.len() > 8 {
                return Err(anyhow!("PPTX chart group axis IDs exceed the safety limit"));
            }
        } else if matches!(parent, Some("catAx" | "valAx")) {
            let axis = axis.ok_or_else(|| anyhow!("PPTX chart axis ID has no owning axis"))?;
            if axis.axis_id.replace(value).is_some() {
                return Err(anyhow!("PPTX chart axis contains multiple axis IDs"));
            }
        }
    } else if qualified.as_slice() == b"c:axPos" && matches!(parent, Some("catAx" | "valAx")) {
        let value = required_xml_attribute(reader, event, "val")?;
        let axis = axis.ok_or_else(|| anyhow!("PPTX chart axis position has no owning axis"))?;
        if axis.position.replace(value).is_some() {
            return Err(anyhow!("PPTX chart axis contains multiple positions"));
        }
    } else if qualified.as_slice() == b"c:logBase" && parent == Some("scaling") {
        let value = required_xml_attribute(reader, event, "val")?;
        if value.is_empty() || value.len() > 128 {
            return Err(anyhow!(
                "PPTX chart axis log base is empty or exceeds the safety limit"
            ));
        }
        let axis = axis.ok_or_else(|| anyhow!("PPTX chart axis log base has no owning axis"))?;
        if axis.log_base.replace(value).is_some() {
            return Err(anyhow!("PPTX chart axis contains duplicate log bases"));
        }
    } else if matches!(qualified.as_slice(), b"c:min" | b"c:max") && parent == Some("scaling") {
        let value = required_xml_attribute(reader, event, "val")?;
        if value.is_empty() || value.len() > 128 {
            return Err(anyhow!(
                "PPTX chart axis bound is empty or exceeds the safety limit"
            ));
        }
        let axis = axis.ok_or_else(|| anyhow!("PPTX chart axis bound has no owning axis"))?;
        let slot = if qualified.as_slice() == b"c:min" {
            &mut axis.minimum
        } else {
            &mut axis.maximum
        };
        if slot.replace(value).is_some() {
            return Err(anyhow!("PPTX chart axis contains duplicate bounds"));
        }
    } else if matches!(qualified.as_slice(), b"c:majorUnit" | b"c:minorUnit")
        && parent == Some("valAx")
    {
        let value = required_xml_attribute(reader, event, "val")?;
        if value.is_empty() || value.len() > 128 {
            return Err(anyhow!(
                "PPTX chart value-axis unit is empty or exceeds the safety limit"
            ));
        }
        let axis = axis.ok_or_else(|| anyhow!("PPTX chart value-axis unit has no owning axis"))?;
        let slot = if qualified.as_slice() == b"c:majorUnit" {
            &mut axis.major_unit
        } else {
            &mut axis.minor_unit
        };
        if slot.replace(value).is_some() {
            return Err(anyhow!("PPTX chart value axis contains duplicate units"));
        }
    } else if qualified.as_slice() == b"c:numFmt" && parent == Some("valAx") {
        let format_code = required_xml_attribute(reader, event, "formatCode")?;
        if format_code.is_empty() || format_code.chars().count() > 128 {
            return Err(anyhow!(
                "PPTX chart value-axis number format is empty or exceeds the safety limit"
            ));
        }
        let source_linked = match required_xml_attribute(reader, event, "sourceLinked")?.as_str() {
            "1" | "true" => true,
            "0" | "false" => false,
            value => {
                return Err(anyhow!(
                    "PPTX chart value-axis sourceLinked contains an invalid boolean value: {value}"
                ));
            }
        };
        let axis =
            axis.ok_or_else(|| anyhow!("PPTX chart value-axis number format has no owning axis"))?;
        if axis.number_format_code.replace(format_code).is_some()
            || axis
                .number_format_source_linked
                .replace(source_linked)
                .is_some()
        {
            return Err(anyhow!(
                "PPTX chart value axis contains multiple number formats"
            ));
        }
    } else if matches!(
        qualified.as_slice(),
        b"c:majorTickMark" | b"c:minorTickMark"
    ) && parent == Some("valAx")
    {
        let value = required_xml_attribute(reader, event, "val")?;
        if value.is_empty() || value.len() > 128 {
            return Err(anyhow!(
                "PPTX chart value-axis tick mark is empty or exceeds the safety limit"
            ));
        }
        let axis =
            axis.ok_or_else(|| anyhow!("PPTX chart value-axis tick mark has no owning axis"))?;
        let slot = if qualified.as_slice() == b"c:majorTickMark" {
            &mut axis.major_tick_mark
        } else {
            &mut axis.minor_tick_mark
        };
        if slot.replace(value).is_some() {
            return Err(anyhow!(
                "PPTX chart value axis contains duplicate tick marks"
            ));
        }
    }
    Ok(())
}

pub(super) fn ensure_standard_pptx_chart_namespace(qualified: &[u8], local: &str) -> Result<()> {
    let requires_chart_namespace = matches!(
        local,
        "chartSpace"
            | "chart"
            | "plotArea"
            | "title"
            | "legend"
            | "legendPos"
            | "dLbls"
            | "showVal"
            | "showPercent"
            | "barDir"
            | "radarStyle"
            | "scatterStyle"
            | "bubbleScale"
            | "showNegBubbles"
            | "sizeRepresents"
            | "bubble3D"
            | "axId"
            | "axPos"
            | "scaling"
            | "orientation"
            | "logBase"
            | "min"
            | "max"
            | "majorTickMark"
            | "minorTickMark"
            | "majorUnit"
            | "minorUnit"
            | "numFmt"
            | "catAx"
            | "valAx"
            | "ser"
            | "tx"
            | "cat"
            | "val"
            | "xVal"
            | "yVal"
            | "bubbleSize"
            | "f"
            | "v"
            | "barChart"
            | "bar3DChart"
            | "lineChart"
            | "line3DChart"
            | "pieChart"
            | "pie3DChart"
            | "doughnutChart"
            | "areaChart"
            | "area3DChart"
            | "radarChart"
            | "scatterChart"
            | "bubbleChart"
            | "stockChart"
            | "surfaceChart"
            | "surface3DChart"
            | "ofPieChart"
    );
    if requires_chart_namespace && !qualified.starts_with(b"c:") {
        return Err(anyhow!(
            "PPTX chart structure must use the standard c namespace"
        ));
    }
    if local == "t" && !qualified.starts_with(b"a:") {
        return Err(anyhow!(
            "PPTX rich chart title text must use the standard a namespace"
        ));
    }
    Ok(())
}

pub(super) fn standard_pptx_chart_type(qualified: &[u8]) -> Option<&'static str> {
    match qualified {
        b"c:barChart" => Some("bar"),
        b"c:bar3DChart" => Some("bar_3d"),
        b"c:lineChart" => Some("line"),
        b"c:line3DChart" => Some("line_3d"),
        b"c:pieChart" => Some("pie"),
        b"c:pie3DChart" => Some("pie_3d"),
        b"c:doughnutChart" => Some("doughnut"),
        b"c:areaChart" => Some("area"),
        b"c:area3DChart" => Some("area_3d"),
        b"c:radarChart" => Some("radar"),
        b"c:scatterChart" => Some("scatter"),
        b"c:bubbleChart" => Some("bubble"),
        b"c:stockChart" => Some("stock"),
        b"c:surfaceChart" => Some("surface"),
        b"c:surface3DChart" => Some("surface_3d"),
        b"c:ofPieChart" => Some("of_pie"),
        _ => None,
    }
}

fn standard_pptx_chart_type_local(local: &str) -> Option<&'static str> {
    match local {
        "barChart" => Some("bar"),
        "bar3DChart" => Some("bar_3d"),
        "lineChart" => Some("line"),
        "line3DChart" => Some("line_3d"),
        "pieChart" => Some("pie"),
        "pie3DChart" => Some("pie_3d"),
        "doughnutChart" => Some("doughnut"),
        "areaChart" => Some("area"),
        "area3DChart" => Some("area_3d"),
        "radarChart" => Some("radar"),
        "scatterChart" => Some("scatter"),
        "bubbleChart" => Some("bubble"),
        "stockChart" => Some("stock"),
        "surfaceChart" => Some("surface"),
        "surface3DChart" => Some("surface_3d"),
        "ofPieChart" => Some("of_pie"),
        _ => None,
    }
}
