// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use super::chart_axis_inspection::{
    pptx_chart_axis_title_fields, pptx_chart_is_horizontal_bar,
    resolve_pptx_chart_series_value_axis,
};
use super::model::{
    PptxChartAxisInspection, PptxChartGroupInspection, PptxChartInspection,
    PptxChartSeriesInspection,
};

pub(super) struct PptxChartInspectionParts {
    pub(super) chart_types: BTreeSet<String>,
    pub(super) chart_groups: Vec<PptxChartGroupInspection>,
    pub(super) axes: Vec<PptxChartAxisInspection>,
    pub(super) title: String,
    pub(super) title_formula: Option<String>,
    pub(super) series: Vec<PptxChartSeriesInspection>,
    pub(super) cached_points: usize,
    pub(super) legend_count: usize,
    pub(super) legend_positions: Vec<String>,
    pub(super) data_label_group_count: usize,
    pub(super) data_label_show_value_count: usize,
    pub(super) data_label_show_percentage_count: usize,
}

pub(super) fn finalize_pptx_chart_inspection(
    mut parts: PptxChartInspectionParts,
) -> PptxChartInspection {
    for item in &mut parts.series {
        item.value_axis = resolve_pptx_chart_series_value_axis(
            item,
            parts.chart_groups.as_slice(),
            parts.axes.as_slice(),
        );
    }
    let title_truncated = parts.title.chars().count() > 1_000;
    let horizontal_bar = pptx_chart_is_horizontal_bar(&parts.chart_types, &parts.chart_groups);
    let numeric_x = parts.chart_types.len() == 1
        && (parts.chart_types.contains("scatter") || parts.chart_types.contains("bubble"));
    let category_axis_position = if horizontal_bar { "l" } else { "b" };
    let value_axis_position = if horizontal_bar { "b" } else { "l" };
    let secondary_value_axis_position = if horizontal_bar { "t" } else { "r" };
    let category_axis = if numeric_x {
        parts.axes.iter().find(|axis| {
            axis.axis_type == "value" && axis.position.as_deref() == Some(category_axis_position)
        })
    } else {
        parts
            .axes
            .iter()
            .find(|axis| {
                axis.axis_type == "category"
                    && axis.position.as_deref() == Some(category_axis_position)
            })
            .or_else(|| parts.axes.iter().find(|axis| axis.axis_type == "category"))
    };
    let value_axis = parts
        .axes
        .iter()
        .find(|axis| {
            axis.axis_type == "value" && axis.position.as_deref() == Some(value_axis_position)
        })
        .or_else(|| {
            parts.axes.iter().find(|axis| {
                axis.axis_type == "value"
                    && axis.position.as_deref() != Some(secondary_value_axis_position)
            })
        })
        .or_else(|| parts.axes.iter().find(|axis| axis.axis_type == "value"));
    let secondary_value_axis = parts.axes.iter().find(|axis| {
        axis.axis_type == "value" && axis.position.as_deref() == Some(secondary_value_axis_position)
    });
    let (category_axis_title, category_axis_title_formula, category_axis_title_truncated) =
        pptx_chart_axis_title_fields(category_axis);
    let (value_axis_title, value_axis_title_formula, value_axis_title_truncated) =
        pptx_chart_axis_title_fields(value_axis);
    let (
        secondary_value_axis_title,
        secondary_value_axis_title_formula,
        secondary_value_axis_title_truncated,
    ) = pptx_chart_axis_title_fields(secondary_value_axis);
    PptxChartInspection {
        chart_types: parts.chart_types.into_iter().collect(),
        chart_groups: parts.chart_groups,
        axes: parts.axes,
        title: parts.title.chars().take(1_000).collect(),
        title_formula: parts.title_formula,
        title_truncated,
        series: parts.series,
        cached_points: parts.cached_points,
        legend_count: parts.legend_count,
        legend_positions: parts.legend_positions,
        data_label_group_count: parts.data_label_group_count,
        data_label_show_value_count: parts.data_label_show_value_count,
        data_label_show_percentage_count: parts.data_label_show_percentage_count,
        category_axis_title,
        category_axis_title_formula,
        category_axis_title_truncated,
        value_axis_title,
        value_axis_title_formula,
        value_axis_title_truncated,
        secondary_value_axis_title,
        secondary_value_axis_title_formula,
        secondary_value_axis_title_truncated,
    }
}
