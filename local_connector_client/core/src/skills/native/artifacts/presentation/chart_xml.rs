// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

use super::chart_axes_xml::{
    presentation_chart_axes_xml, presentation_chart_secondary_axes_xml,
    presentation_scatter_chart_axes_xml, presentation_scatter_chart_secondary_axes_xml,
};
use super::chart_model::{
    PresentationChart, PresentationChartDataLabels, PresentationChartMarkerStyle,
    PresentationChartSeries, PresentationChartType, PresentationChartValueAxis,
    PresentationChartValueAxisOptions,
};
use super::chart_xml_common::{presentation_chart_number_text, presentation_chart_title_xml};
use super::limits::{
    PPTX_PRIMARY_CATEGORY_AXIS_ID, PPTX_PRIMARY_VALUE_AXIS_ID, PPTX_SECONDARY_CATEGORY_AXIS_ID,
    PPTX_SECONDARY_VALUE_AXIS_ID,
};
use super::{escape_xml, inspect_standard_pptx_chart_xml};

pub(super) fn presentation_chart_xml(chart: &PresentationChart) -> Result<String> {
    let point_count = chart
        .categories
        .as_ref()
        .map(Vec::len)
        .or_else(|| chart.x_values.as_ref().map(Vec::len))
        .ok_or_else(|| anyhow!("PPTX chart has no category or X values"))?;
    let title = if chart.title.is_empty() {
        "<c:autoTitleDeleted val=\"1\"/>".to_string()
    } else {
        format!(
            "{}<c:autoTitleDeleted val=\"0\"/>",
            presentation_chart_title_xml(chart.title.as_str())
        )
    };
    let primary_series = chart
        .series
        .iter()
        .enumerate()
        .filter(|(_, series)| series.value_axis == PresentationChartValueAxis::Primary)
        .map(|(index, series)| presentation_chart_series_xml(chart, index, series))
        .collect::<Result<Vec<_>>>()?
        .concat();
    let secondary_series = chart
        .series
        .iter()
        .enumerate()
        .filter(|(_, series)| series.value_axis == PresentationChartValueAxis::Secondary)
        .map(|(index, series)| presentation_chart_series_xml(chart, index, series))
        .collect::<Result<Vec<_>>>()?
        .concat();
    let has_secondary_axis = !secondary_series.is_empty();
    let data_labels = presentation_chart_data_labels_xml(chart.data_labels);
    let primary_group = presentation_chart_group_xml(
        chart.chart_type,
        primary_series.as_str(),
        data_labels,
        PPTX_PRIMARY_CATEGORY_AXIS_ID,
        PPTX_PRIMARY_VALUE_AXIS_ID,
    );
    let plot = if chart.chart_type.is_part_to_whole() {
        primary_group
    } else {
        let cross_between = if chart.chart_type == PresentationChartType::Area {
            "midCat"
        } else {
            "between"
        };
        let x_options = PresentationChartValueAxisOptions {
            minimum: chart.x_axis_minimum,
            maximum: chart.x_axis_maximum,
            log_base: chart.x_axis_log_base,
            major_tick_mark: chart.x_axis_major_tick_mark,
            minor_tick_mark: chart.x_axis_minor_tick_mark,
            major_unit: chart.x_axis_major_unit,
            minor_unit: chart.x_axis_minor_unit,
            number_format: chart.x_axis_number_format,
        };
        let primary_options = PresentationChartValueAxisOptions {
            minimum: chart.value_axis_minimum,
            maximum: chart.value_axis_maximum,
            log_base: chart.value_axis_log_base,
            major_tick_mark: chart.value_axis_major_tick_mark,
            minor_tick_mark: chart.value_axis_minor_tick_mark,
            major_unit: chart.value_axis_major_unit,
            minor_unit: chart.value_axis_minor_unit,
            number_format: chart.value_axis_number_format,
        };
        let axes = if chart.chart_type.uses_numeric_x_axis() {
            presentation_scatter_chart_axes_xml(
                PPTX_PRIMARY_CATEGORY_AXIS_ID,
                PPTX_PRIMARY_VALUE_AXIS_ID,
                chart.category_axis_title.as_str(),
                chart.value_axis_title.as_str(),
                x_options,
                primary_options,
            )
        } else {
            presentation_chart_axes_xml(
                chart.chart_type,
                PPTX_PRIMARY_CATEGORY_AXIS_ID,
                PPTX_PRIMARY_VALUE_AXIS_ID,
                cross_between,
                chart.category_axis_title.as_str(),
                chart.value_axis_title.as_str(),
                primary_options,
            )
        };
        if has_secondary_axis {
            let secondary_group = presentation_chart_group_xml(
                chart.chart_type,
                secondary_series.as_str(),
                data_labels,
                PPTX_SECONDARY_CATEGORY_AXIS_ID,
                PPTX_SECONDARY_VALUE_AXIS_ID,
            );
            let secondary_options = PresentationChartValueAxisOptions {
                minimum: chart.secondary_value_axis_minimum,
                maximum: chart.secondary_value_axis_maximum,
                log_base: chart.secondary_value_axis_log_base,
                major_tick_mark: chart.secondary_value_axis_major_tick_mark,
                minor_tick_mark: chart.secondary_value_axis_minor_tick_mark,
                major_unit: chart.secondary_value_axis_major_unit,
                minor_unit: chart.secondary_value_axis_minor_unit,
                number_format: chart.secondary_value_axis_number_format,
            };
            let secondary_axes = if chart.chart_type.uses_numeric_x_axis() {
                presentation_scatter_chart_secondary_axes_xml(
                    PPTX_SECONDARY_CATEGORY_AXIS_ID,
                    PPTX_SECONDARY_VALUE_AXIS_ID,
                    chart.secondary_value_axis_title.as_str(),
                    x_options,
                    secondary_options,
                )
            } else {
                presentation_chart_secondary_axes_xml(
                    chart.chart_type,
                    PPTX_SECONDARY_CATEGORY_AXIS_ID,
                    PPTX_SECONDARY_VALUE_AXIS_ID,
                    cross_between,
                    chart.secondary_value_axis_title.as_str(),
                    secondary_options,
                )
            };
            format!("{primary_group}{secondary_group}{axes}{secondary_axes}")
        } else {
            format!("{primary_group}{axes}")
        }
    };
    let legend = if chart.show_legend {
        format!(
            r#"<c:legend><c:legendPos val="{}"/><c:layout/><c:overlay val="0"/></c:legend>"#,
            chart.legend_position.as_ooxml()
        )
    } else {
        String::new()
    };
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:date1904 val="0"/><c:lang val="zh-CN"/><c:roundedCorners val="0"/><c:style val="10"/><c:chart>{title}<c:plotArea><c:layout/>{plot}</c:plotArea>{legend}<c:plotVisOnly val="1"/><c:dispBlanksAs val="gap"/><c:showDLblsOverMax val="0"/></c:chart></c:chartSpace>"#
    );
    let inspection = inspect_standard_pptx_chart_xml(xml.as_str())?;
    if inspection.series.len() != chart.series.len()
        || inspection
            .series
            .iter()
            .zip(chart.series.iter())
            .any(|(inspected, expected)| {
                inspected.color_style_custom
                    || inspected.color != expected.color
                    || inspected.marker_style_custom
                    || inspected.marker_style.as_deref()
                        != expected
                            .marker_style
                            .map(PresentationChartMarkerStyle::as_str)
                    || inspected.marker_size != expected.marker_size
                    || inspected.smooth_custom
                    || inspected.smooth != expected.smooth
            })
        || inspection.chart_types.len() != 1
        || inspection.chart_groups.len() != usize::from(has_secondary_axis) + 1
        || inspection.cached_points
            != chart
                .series
                .len()
                .saturating_mul(point_count)
                .saturating_mul(if chart.chart_type == PresentationChartType::Bubble {
                    3
                } else {
                    2
                })
    {
        return Err(anyhow!(
            "generated PPTX chart did not pass the standard chart inspection contract"
        ));
    }
    Ok(xml)
}

fn presentation_chart_group_xml(
    chart_type: PresentationChartType,
    series: &str,
    data_labels: &str,
    category_axis_id: u32,
    value_axis_id: u32,
) -> String {
    match chart_type {
        PresentationChartType::Column => format!(
            r#"<c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:varyColors val="0"/>{series}{data_labels}<c:gapWidth val="150"/><c:overlap val="0"/><c:axId val="{category_axis_id}"/><c:axId val="{value_axis_id}"/></c:barChart>"#
        ),
        PresentationChartType::Bar => format!(
            r#"<c:barChart><c:barDir val="bar"/><c:grouping val="clustered"/><c:varyColors val="0"/>{series}{data_labels}<c:gapWidth val="150"/><c:overlap val="0"/><c:axId val="{category_axis_id}"/><c:axId val="{value_axis_id}"/></c:barChart>"#
        ),
        PresentationChartType::Line => format!(
            r#"<c:lineChart><c:grouping val="standard"/><c:varyColors val="0"/>{series}{data_labels}<c:marker val="1"/><c:smooth val="0"/><c:axId val="{category_axis_id}"/><c:axId val="{value_axis_id}"/></c:lineChart>"#
        ),
        PresentationChartType::Pie => format!(
            r#"<c:pieChart><c:varyColors val="1"/>{series}{data_labels}<c:firstSliceAng val="0"/></c:pieChart>"#
        ),
        PresentationChartType::Area => format!(
            r#"<c:areaChart><c:grouping val="standard"/><c:varyColors val="0"/>{series}{data_labels}<c:axId val="{category_axis_id}"/><c:axId val="{value_axis_id}"/></c:areaChart>"#
        ),
        PresentationChartType::Doughnut => format!(
            r#"<c:doughnutChart><c:varyColors val="1"/>{series}{data_labels}<c:firstSliceAng val="0"/><c:holeSize val="50"/></c:doughnutChart>"#
        ),
        PresentationChartType::Radar => format!(
            r#"<c:radarChart><c:radarStyle val="standard"/><c:varyColors val="0"/>{series}{data_labels}<c:axId val="{category_axis_id}"/><c:axId val="{value_axis_id}"/></c:radarChart>"#
        ),
        PresentationChartType::Scatter => format!(
            r#"<c:scatterChart><c:scatterStyle val="lineMarker"/><c:varyColors val="0"/>{series}{data_labels}<c:axId val="{category_axis_id}"/><c:axId val="{value_axis_id}"/></c:scatterChart>"#
        ),
        PresentationChartType::Bubble => format!(
            r#"<c:bubbleChart><c:varyColors val="0"/>{series}{data_labels}<c:bubbleScale val="100"/><c:showNegBubbles val="0"/><c:sizeRepresents val="area"/><c:axId val="{category_axis_id}"/><c:axId val="{value_axis_id}"/></c:bubbleChart>"#
        ),
    }
}

fn presentation_chart_data_labels_xml(data_labels: PresentationChartDataLabels) -> &'static str {
    match data_labels {
        PresentationChartDataLabels::None => "",
        PresentationChartDataLabels::Value => {
            r#"<c:dLbls><c:showLegendKey val="0"/><c:showVal val="1"/><c:showCatName val="0"/><c:showSerName val="0"/><c:showPercent val="0"/><c:showBubbleSize val="0"/><c:showLeaderLines val="0"/></c:dLbls>"#
        }
        PresentationChartDataLabels::Percentage => {
            r#"<c:dLbls><c:showLegendKey val="0"/><c:showVal val="0"/><c:showCatName val="0"/><c:showSerName val="0"/><c:showPercent val="1"/><c:showBubbleSize val="0"/><c:showLeaderLines val="1"/></c:dLbls>"#
        }
    }
}

fn presentation_chart_series_xml(
    chart: &PresentationChart,
    index: usize,
    series: &PresentationChartSeries,
) -> Result<String> {
    let color = presentation_chart_series_color_xml(chart.chart_type, series.color.as_deref());
    let marker = presentation_chart_series_marker_xml(series.marker_style, series.marker_size);
    let smooth = series
        .smooth
        .map(|smooth| format!(r#"<c:smooth val="{}"/>"#, u8::from(smooth)))
        .unwrap_or_default();
    if chart.chart_type == PresentationChartType::Scatter {
        let x_values = chart
            .x_values
            .as_deref()
            .ok_or_else(|| anyhow!("PPTX scatter chart is missing validated X values"))?;
        Ok(format!(
            r#"<c:ser><c:idx val="{index}"/><c:order val="{index}"/><c:tx><c:v>{}</c:v></c:tx>{color}{marker}<c:xVal>{}</c:xVal><c:yVal>{}</c:yVal>{smooth}</c:ser>"#,
            escape_xml(series.name.as_str()),
            presentation_chart_number_literal(x_values),
            presentation_chart_number_literal(series.values.as_slice())
        ))
    } else if chart.chart_type == PresentationChartType::Bubble {
        let x_values = chart
            .x_values
            .as_deref()
            .ok_or_else(|| anyhow!("PPTX bubble chart is missing validated X values"))?;
        let bubble_sizes = series
            .bubble_sizes
            .as_deref()
            .ok_or_else(|| anyhow!("PPTX bubble chart series is missing validated bubble sizes"))?;
        Ok(format!(
            r#"<c:ser><c:idx val="{index}"/><c:order val="{index}"/><c:tx><c:v>{}</c:v></c:tx>{color}<c:xVal>{}</c:xVal><c:yVal>{}</c:yVal><c:bubbleSize>{}</c:bubbleSize></c:ser>"#,
            escape_xml(series.name.as_str()),
            presentation_chart_number_literal(x_values),
            presentation_chart_number_literal(series.values.as_slice()),
            presentation_chart_number_literal(bubble_sizes)
        ))
    } else {
        let categories = chart
            .categories
            .as_deref()
            .ok_or_else(|| anyhow!("PPTX chart is missing validated categories"))?;
        Ok(format!(
            r#"<c:ser><c:idx val="{index}"/><c:order val="{index}"/><c:tx><c:v>{}</c:v></c:tx>{color}{marker}<c:cat>{}</c:cat><c:val>{}</c:val>{smooth}</c:ser>"#,
            escape_xml(series.name.as_str()),
            presentation_chart_string_literal(categories),
            presentation_chart_number_literal(series.values.as_slice())
        ))
    }
}

fn presentation_chart_series_marker_xml(
    marker_style: Option<PresentationChartMarkerStyle>,
    marker_size: Option<u8>,
) -> String {
    let Some(marker_style) = marker_style else {
        return String::new();
    };
    let size = marker_size
        .map(|size| format!(r#"<c:size val="{size}"/>"#))
        .unwrap_or_default();
    format!(
        r#"<c:marker><c:symbol val="{}"/>{size}</c:marker>"#,
        marker_style.as_ooxml()
    )
}

fn presentation_chart_series_color_xml(
    chart_type: PresentationChartType,
    color: Option<&str>,
) -> String {
    let Some(rgb) = color.and_then(|value| value.strip_prefix('#')) else {
        return String::new();
    };
    if chart_type.uses_line_color() {
        format!(
            r#"<c:spPr><a:ln><a:solidFill><a:srgbClr val="{rgb}"/></a:solidFill></a:ln></c:spPr>"#
        )
    } else {
        format!(r#"<c:spPr><a:solidFill><a:srgbClr val="{rgb}"/></a:solidFill></c:spPr>"#)
    }
}

fn presentation_chart_string_literal(values: &[String]) -> String {
    let points = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            format!(
                "<c:pt idx=\"{index}\"><c:v>{}</c:v></c:pt>",
                escape_xml(value)
            )
        })
        .collect::<String>();
    format!(
        "<c:strLit><c:ptCount val=\"{}\"/>{points}</c:strLit>",
        values.len()
    )
}

fn presentation_chart_number_literal(values: &[f64]) -> String {
    let points = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            format!(
                "<c:pt idx=\"{index}\"><c:v>{}</c:v></c:pt>",
                presentation_chart_number_text(*value)
            )
        })
        .collect::<String>();
    format!(
        "<c:numLit><c:formatCode>General</c:formatCode><c:ptCount val=\"{}\"/>{points}</c:numLit>",
        values.len()
    )
}
