// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

use super::chart_model::PresentationChart;
use super::image::PresentationImage;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SlideLayout {
    TitleBody,
    TitleOnly,
    Section,
    TwoColumn,
    ImageRight,
    ImageFull,
    Table,
    Chart,
}

impl SlideLayout {
    pub(super) fn parse(value: Option<&str>) -> Result<Self> {
        match value.unwrap_or("title_body") {
            "title_body" => Ok(Self::TitleBody),
            "title_only" => Ok(Self::TitleOnly),
            "section" => Ok(Self::Section),
            "two_column" => Ok(Self::TwoColumn),
            "image_right" => Ok(Self::ImageRight),
            "image_full" => Ok(Self::ImageFull),
            "table" => Ok(Self::Table),
            "chart" => Ok(Self::Chart),
            value => Err(anyhow!("unsupported PPTX slide layout: {value}")),
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::TitleBody => "title_body",
            Self::TitleOnly => "title_only",
            Self::Section => "section",
            Self::TwoColumn => "two_column",
            Self::ImageRight => "image_right",
            Self::ImageFull => "image_full",
            Self::Table => "table",
            Self::Chart => "chart",
        }
    }
}

#[derive(Debug)]
pub(super) struct PresentationTable {
    pub(super) cells: Vec<Vec<String>>,
    pub(super) header_row: bool,
}

#[derive(Debug)]
pub(super) struct SlideDefinition {
    pub(super) title: String,
    pub(super) body: String,
    pub(super) left_body: String,
    pub(super) right_body: String,
    pub(super) notes: String,
    pub(super) layout: SlideLayout,
    pub(super) image: Option<PresentationImage>,
    pub(super) table: Option<PresentationTable>,
    pub(super) chart: Option<PresentationChart>,
}

#[derive(Clone, Copy)]
pub(super) struct PptxXmlElementRange {
    pub(super) start: usize,
    pub(super) open_end: usize,
    pub(super) close_start: usize,
    pub(super) end: usize,
}

#[derive(Clone)]
pub(super) struct SimplePptxTextRun {
    pub(super) run_start: usize,
    pub(super) run_end: usize,
    pub(super) text_start: usize,
    pub(super) text_open_end: usize,
    pub(super) text_close_end: usize,
    pub(super) formatting: String,
    pub(super) decoded: String,
}

pub(super) struct PptxCrossRunTextMatch {
    pub(super) runs: Vec<SimplePptxTextRun>,
    pub(super) first_offset: usize,
    pub(super) last_offset: usize,
}

pub(super) struct PptxCrossRunScan {
    pub(super) occurrences: usize,
    pub(super) matched: Option<PptxCrossRunTextMatch>,
    pub(super) unsupported_reason: Option<String>,
}

#[derive(Clone)]
pub(super) struct SimplePptxTableCell {
    pub(super) row: usize,
    pub(super) column: usize,
    pub(super) range: PptxXmlElementRange,
    pub(super) text_start: usize,
    pub(super) text_open_end: usize,
    pub(super) text_close_end: usize,
    pub(super) decoded: String,
}

pub(super) struct SimplePptxTable {
    pub(super) range: PptxXmlElementRange,
    pub(super) rows: usize,
    pub(super) columns: usize,
    pub(super) cells: Vec<SimplePptxTableCell>,
}

#[derive(Clone, Copy)]
pub(super) struct SimplePptxTableRow {
    pub(super) range: PptxXmlElementRange,
    pub(super) height: i64,
}

#[derive(Clone, Copy)]
pub(super) struct SimplePptxTableColumn {
    pub(super) range: PptxXmlElementRange,
    pub(super) width: i64,
}

pub(super) struct PptxTableScan {
    pub(super) rows: usize,
    pub(super) columns: usize,
    pub(super) cells: usize,
    pub(super) cell_text: Vec<Vec<String>>,
    pub(super) cell_text_truncated: bool,
    pub(super) simple: Option<SimplePptxTable>,
    pub(super) unsupported_reason: Option<String>,
}

#[derive(Clone, Debug)]
pub(super) struct ResolvedPptxChartReference {
    pub(super) relationship_id: String,
    pub(super) part: String,
}

#[derive(Clone, Debug, Default)]
pub(super) struct PptxChartSeriesInspection {
    pub(super) chart_type: String,
    pub(super) chart_group_index: usize,
    pub(super) value_axis: String,
    pub(super) color: Option<String>,
    pub(super) color_value: Option<String>,
    pub(super) color_style_present: bool,
    pub(super) color_style_custom: bool,
    pub(super) color_shape_properties_count: usize,
    pub(super) color_line_count: usize,
    pub(super) color_solid_fill_count: usize,
    pub(super) color_srgb_count: usize,
    pub(super) marker_style: Option<String>,
    pub(super) marker_style_value: Option<String>,
    pub(super) marker_size: Option<u8>,
    pub(super) marker_size_value: Option<String>,
    pub(super) marker_style_custom: bool,
    pub(super) marker_count: usize,
    pub(super) marker_symbol_count: usize,
    pub(super) marker_size_count: usize,
    pub(super) smooth: Option<bool>,
    pub(super) smooth_value: Option<String>,
    pub(super) smooth_custom: bool,
    pub(super) smooth_count: usize,
    pub(super) name: String,
    pub(super) name_formula: Option<String>,
    pub(super) category_formula: Option<String>,
    pub(super) value_formula: Option<String>,
    pub(super) bubble_size_formula: Option<String>,
    pub(super) categories: Vec<String>,
    pub(super) values: Vec<String>,
    pub(super) bubble_sizes: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct PptxChartGroupInspection {
    pub(super) chart_type: String,
    pub(super) bar_direction: Option<String>,
    pub(super) radar_style: Option<String>,
    pub(super) scatter_style: Option<String>,
    pub(super) bubble_scale: Option<String>,
    pub(super) show_negative_bubbles: Option<String>,
    pub(super) bubble_size_represents: Option<String>,
    pub(super) bubble_3d: Option<String>,
    pub(super) axis_ids: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct PptxChartAxisInspection {
    pub(super) axis_type: String,
    pub(super) axis_id: Option<String>,
    pub(super) position: Option<String>,
    pub(super) title: String,
    pub(super) title_formula: Option<String>,
    pub(super) title_truncated: bool,
    pub(super) minimum: Option<String>,
    pub(super) maximum: Option<String>,
    pub(super) log_base: Option<String>,
    pub(super) major_tick_mark: Option<String>,
    pub(super) minor_tick_mark: Option<String>,
    pub(super) major_unit: Option<String>,
    pub(super) minor_unit: Option<String>,
    pub(super) number_format_code: Option<String>,
    pub(super) number_format_source_linked: Option<bool>,
}

pub(super) struct PptxChartInspection {
    pub(super) chart_types: Vec<String>,
    pub(super) chart_groups: Vec<PptxChartGroupInspection>,
    pub(super) axes: Vec<PptxChartAxisInspection>,
    pub(super) title: String,
    pub(super) title_formula: Option<String>,
    pub(super) title_truncated: bool,
    pub(super) series: Vec<PptxChartSeriesInspection>,
    pub(super) cached_points: usize,
    pub(super) legend_count: usize,
    pub(super) legend_positions: Vec<String>,
    pub(super) data_label_group_count: usize,
    pub(super) data_label_show_value_count: usize,
    pub(super) data_label_show_percentage_count: usize,
    pub(super) category_axis_title: String,
    pub(super) category_axis_title_formula: Option<String>,
    pub(super) category_axis_title_truncated: bool,
    pub(super) value_axis_title: String,
    pub(super) value_axis_title_formula: Option<String>,
    pub(super) value_axis_title_truncated: bool,
    pub(super) secondary_value_axis_title: String,
    pub(super) secondary_value_axis_title_formula: Option<String>,
    pub(super) secondary_value_axis_title_truncated: bool,
}

pub(super) struct PptxChartRelationshipInspection {
    pub(super) data_source: &'static str,
    pub(super) relationship_count: usize,
    pub(super) embedded_workbook: Option<String>,
    pub(super) relationships_part_present: bool,
}

pub(super) struct PptxChartOwnership {
    pub(super) ordered_slide_paths: Vec<String>,
    pub(super) charts_by_slide: Vec<Vec<ResolvedPptxChartReference>>,
    pub(super) chart_count: usize,
}

#[derive(Clone, Debug)]
pub(super) struct OwnedNotesPart {
    pub(super) path: String,
    pub(super) relationships_path: String,
}

#[derive(Default)]
pub(super) struct SlideRelationshipInspection {
    pub(super) image_count: usize,
    pub(super) notes_path: Option<String>,
}
