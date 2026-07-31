// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use quick_xml::events::BytesStart;
use quick_xml::Reader;

use super::chart_model::PresentationChartMarkerStyle;
use super::limits::{MAX_PPTX_CREATE_CHART_MARKER_SIZE, MIN_PPTX_CREATE_CHART_MARKER_SIZE};
use super::model::PptxChartSeriesInspection;
use super::{normalize_presentation_chart_rgb, optional_xml_attribute};

pub(super) fn record_pptx_chart_series_color_element(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    stack: &[String],
    series: Option<&mut PptxChartSeriesInspection>,
    empty: bool,
) -> Result<()> {
    let Some(series) = series else {
        return Ok(());
    };
    let qualified = event.name().as_ref().to_vec();
    let local = event.local_name().as_ref().to_vec();
    if local.as_slice() == b"spPr" && stack.last().map(String::as_str) == Some("ser") {
        series.color_style_present = true;
        series.color_shape_properties_count = series.color_shape_properties_count.saturating_add(1);
        if qualified.as_slice() != b"c:spPr" || empty || !pptx_xml_attributes_match(event, &[])? {
            series.color_style_custom = true;
        }
        return Ok(());
    }
    let Some(shape_properties_index) = pptx_chart_series_shape_properties_index(stack) else {
        return Ok(());
    };
    series.color_style_present = true;
    let relative_ancestors = &stack[shape_properties_index + 1..];
    let expected = match series.chart_type.as_str() {
        "line" | "radar" | "scatter" => match (relative_ancestors, local.as_slice()) {
            ([], b"ln") => Some((b"a:ln".as_slice(), false, &[][..])),
            ([line], b"solidFill") if line == "ln" => {
                Some((b"a:solidFill".as_slice(), false, &[][..]))
            }
            ([line, fill], b"srgbClr") if line == "ln" && fill == "solidFill" => {
                Some((b"a:srgbClr".as_slice(), true, &[b"val".as_slice()][..]))
            }
            _ => None,
        },
        _ => match (relative_ancestors, local.as_slice()) {
            ([], b"solidFill") => Some((b"a:solidFill".as_slice(), false, &[][..])),
            ([fill], b"srgbClr") if fill == "solidFill" => {
                Some((b"a:srgbClr".as_slice(), true, &[b"val".as_slice()][..]))
            }
            _ => None,
        },
    };
    let Some((expected_name, expected_empty, expected_attributes)) = expected else {
        series.color_style_custom = true;
        return Ok(());
    };
    if qualified.as_slice() != expected_name
        || empty != expected_empty
        || !pptx_xml_attributes_match(event, expected_attributes)?
    {
        series.color_style_custom = true;
    }
    match local.as_slice() {
        b"ln" => {
            series.color_line_count = series.color_line_count.saturating_add(1);
        }
        b"solidFill" => {
            series.color_solid_fill_count = series.color_solid_fill_count.saturating_add(1);
        }
        b"srgbClr" => {
            series.color_srgb_count = series.color_srgb_count.saturating_add(1);
            let value = optional_xml_attribute(reader, event, "val")?;
            if series.color_value.is_some() {
                series.color_style_custom = true;
            } else {
                series.color_value = value.clone();
            }
            match value
                .as_deref()
                .and_then(normalize_presentation_chart_rgb)
                .map(|rgb| format!("#{rgb}"))
            {
                Some(color) => series.color = Some(color),
                None => series.color_style_custom = true,
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn pptx_chart_series_shape_properties_index(stack: &[String]) -> Option<usize> {
    stack
        .iter()
        .enumerate()
        .rfind(|(index, item)| {
            item.as_str() == "spPr"
                && index
                    .checked_sub(1)
                    .and_then(|parent| stack.get(parent))
                    .is_some_and(|parent| parent == "ser")
        })
        .map(|(index, _)| index)
}

pub(super) fn finalize_pptx_chart_series_color(series: &mut PptxChartSeriesInspection) {
    if !series.color_style_present {
        return;
    }
    let expected_line_count = usize::from(matches!(
        series.chart_type.as_str(),
        "line" | "radar" | "scatter"
    ));
    if series.color_shape_properties_count != 1
        || series.color_line_count != expected_line_count
        || series.color_solid_fill_count != 1
        || series.color_srgb_count != 1
        || series.color.is_none()
        || series.color_value.is_none()
    {
        series.color_style_custom = true;
    }
}

pub(super) fn record_pptx_chart_series_marker_element(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    stack: &[String],
    series: Option<&mut PptxChartSeriesInspection>,
    empty: bool,
) -> Result<()> {
    let Some(series) = series else {
        return Ok(());
    };
    let qualified = event.name().as_ref().to_vec();
    let local = event.local_name().as_ref().to_vec();
    if local.as_slice() == b"marker" && stack.last().map(String::as_str) == Some("ser") {
        series.marker_count = series.marker_count.saturating_add(1);
        if qualified.as_slice() != b"c:marker" || empty || !pptx_xml_attributes_match(event, &[])? {
            series.marker_style_custom = true;
        }
        return Ok(());
    }
    let Some(marker_index) = pptx_chart_series_marker_index(stack) else {
        return Ok(());
    };
    let relative_ancestors = &stack[marker_index + 1..];
    let expected_name = match local.as_slice() {
        b"symbol" => Some(b"c:symbol".as_slice()),
        b"size" => Some(b"c:size".as_slice()),
        _ => None,
    };
    if !relative_ancestors.is_empty()
        || expected_name.is_none()
        || expected_name.is_some_and(|expected| qualified.as_slice() != expected)
        || !empty
        || !pptx_xml_attributes_match(event, &[b"val"])?
    {
        series.marker_style_custom = true;
    }
    match local.as_slice() {
        b"symbol" => {
            series.marker_symbol_count = series.marker_symbol_count.saturating_add(1);
            let value = optional_xml_attribute(reader, event, "val")?;
            if series.marker_style_value.is_some() {
                series.marker_style_custom = true;
            } else {
                series.marker_style_value = value.clone();
            }
            match value
                .as_deref()
                .and_then(|value| PresentationChartMarkerStyle::from_ooxml(value).ok())
            {
                Some(style) => series.marker_style = Some(style.as_str().to_string()),
                None => series.marker_style_custom = true,
            }
        }
        b"size" => {
            series.marker_size_count = series.marker_size_count.saturating_add(1);
            let value = optional_xml_attribute(reader, event, "val")?;
            if series.marker_size_value.is_some() {
                series.marker_style_custom = true;
            } else {
                series.marker_size_value = value.clone();
            }
            match value.as_deref().and_then(|value| value.parse::<u8>().ok()) {
                Some(size)
                    if (MIN_PPTX_CREATE_CHART_MARKER_SIZE..=MAX_PPTX_CREATE_CHART_MARKER_SIZE)
                        .contains(&size) =>
                {
                    series.marker_size = Some(size);
                }
                _ => series.marker_style_custom = true,
            }
        }
        _ => series.marker_style_custom = true,
    }
    Ok(())
}

pub(super) fn pptx_chart_series_marker_index(stack: &[String]) -> Option<usize> {
    stack
        .iter()
        .enumerate()
        .rfind(|(index, item)| {
            item.as_str() == "marker"
                && index
                    .checked_sub(1)
                    .and_then(|parent| stack.get(parent))
                    .is_some_and(|parent| parent == "ser")
        })
        .map(|(index, _)| index)
}

pub(super) fn finalize_pptx_chart_series_marker(series: &mut PptxChartSeriesInspection) {
    if !matches!(series.chart_type.as_str(), "line" | "scatter") {
        if series.marker_count != 0
            || series.marker_symbol_count != 0
            || series.marker_size_count != 0
            || series.marker_style.is_some()
            || series.marker_style_value.is_some()
            || series.marker_size.is_some()
            || series.marker_size_value.is_some()
        {
            series.marker_style_custom = true;
        }
        return;
    }
    let style_is_none = series.marker_style.as_deref() == Some("none");
    if series.marker_count != 1
        || series.marker_symbol_count != 1
        || series.marker_style.is_none()
        || series.marker_style_value.is_none()
        || if style_is_none {
            series.marker_size_count != 0
                || series.marker_size.is_some()
                || series.marker_size_value.is_some()
        } else {
            series.marker_size_count != 1
                || series.marker_size.is_none()
                || series.marker_size_value.is_none()
        }
    {
        series.marker_style_custom = true;
    }
}

pub(super) fn record_pptx_chart_series_smooth_element(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    stack: &[String],
    series: Option<&mut PptxChartSeriesInspection>,
    empty: bool,
) -> Result<()> {
    let Some(series) = series else {
        return Ok(());
    };
    if event.local_name().as_ref() != b"smooth" || stack.last().map(String::as_str) != Some("ser") {
        return Ok(());
    }
    series.smooth_count = series.smooth_count.saturating_add(1);
    if event.name().as_ref() != b"c:smooth"
        || !empty
        || !pptx_xml_attributes_match(event, &[b"val"])?
    {
        series.smooth_custom = true;
    }
    let value = optional_xml_attribute(reader, event, "val")?;
    if value
        .as_deref()
        .is_some_and(|value| value.is_empty() || value.len() > 128)
    {
        return Err(anyhow!(
            "PPTX chart series smooth value is empty or exceeds the safety limit"
        ));
    }
    if series.smooth_value.is_some() {
        series.smooth_custom = true;
    } else {
        series.smooth_value = value.clone();
    }
    match value.as_deref() {
        Some("1" | "true") => series.smooth = Some(true),
        Some("0" | "false") => series.smooth = Some(false),
        _ => series.smooth_custom = true,
    }
    Ok(())
}

pub(super) fn finalize_pptx_chart_series_smooth(series: &mut PptxChartSeriesInspection) {
    if matches!(series.chart_type.as_str(), "line" | "scatter") {
        if series.smooth_count != 1 || series.smooth.is_none() || series.smooth_value.is_none() {
            series.smooth_custom = true;
        }
    } else if series.smooth_count != 0 || series.smooth.is_some() || series.smooth_value.is_some() {
        series.smooth_custom = true;
    }
}

fn pptx_xml_attributes_match(event: &BytesStart<'_>, expected: &[&[u8]]) -> Result<bool> {
    let mut actual = event
        .attributes()
        .with_checks(false)
        .map(|attribute| {
            attribute
                .context("parse PPTX XML attribute")
                .map(|attribute| attribute.key.as_ref().to_vec())
        })
        .collect::<Result<Vec<_>>>()?;
    let mut expected = expected
        .iter()
        .map(|name| name.to_vec())
        .collect::<Vec<_>>();
    actual.sort();
    expected.sort();
    Ok(actual == expected)
}
