// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Component, Path};

use anyhow::{anyhow, Context, Result};
use quick_xml::escape::{resolve_xml_entity, unescape};
use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer, XmlVersion};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::{
    file_size, input_file, input_file_any, optional_bool, read_zip_text, require_extension,
    required_text, safe_workspace_path, MAX_ARTIFACT_BYTES,
};

const SLIDE_WIDTH: i64 = 12_192_000;
const SLIDE_HEIGHT: i64 = 6_858_000;
const MAX_PPTX_SLIDES: usize = 200;
const MAX_PPTX_ZIP_ENTRIES: usize = 10_000;
const MAX_PPTX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_PPTX_IMAGE_PIXELS: u64 = 40_000_000;
const MAX_PPTX_TOTAL_IMAGE_BYTES: usize = 50 * 1024 * 1024;
const MAX_PPTX_TEXT_CHARS: usize = 500_000;
const MAX_SLIDE_TEXT_CHARS: usize = 100_000;
const MAX_SLIDE_LINES: usize = 2_000;
const MAX_PPTX_CROSS_RUNS: usize = 16;
const MAX_PPTX_PARAGRAPHS_PER_SLIDE: usize = 20_000;
const MAX_PPTX_RUNS_PER_PARAGRAPH: usize = 1_000;
const MAX_PPTX_TABLES_PER_SLIDE: usize = 100;
const MAX_PPTX_TABLE_ROWS: usize = 500;
const MAX_PPTX_TABLE_COLUMNS: usize = 64;
const MAX_PPTX_TABLE_CELLS: usize = 10_000;
const MAX_PPTX_TABLE_CELL_TEXT_CHARS: usize = 10_000;
const MAX_PPTX_TABLE_TOTAL_TEXT_CHARS: usize = 100_000;
const MAX_PPTX_TABLE_PREVIEW_CHARS: usize = 1_000;
const MAX_PPTX_CREATE_TABLE_ROWS: usize = 50;
const MAX_PPTX_CREATE_TABLE_COLUMNS: usize = 20;
const MAX_PPTX_CREATE_TABLE_CELLS: usize = 1_000;
const MAX_PPTX_CHARTS_PER_SLIDE: usize = 50;
const MAX_PPTX_CHARTS_TOTAL: usize = 200;
const MAX_PPTX_CHART_SERIES: usize = 100;
const MAX_PPTX_CHART_POINTS: usize = 10_000;
const MAX_PPTX_CHART_PREVIEW_POINTS: usize = 200;
const MAX_PPTX_CHART_TEXT_CHARS: usize = 100_000;
const MAX_PPTX_CHART_FORMULA_CHARS: usize = 10_000;
const MAX_PPTX_CREATE_CHART_CATEGORIES: usize = 50;
const MAX_PPTX_CREATE_CHART_SERIES: usize = 10;
const MAX_PPTX_CREATE_CHART_VALUE_ABS: f64 = 1_000_000_000_000.0;
const MIN_PPTX_CREATE_CHART_LOG_BASE: f64 = 2.0;
const MAX_PPTX_CREATE_CHART_LOG_BASE: f64 = 1_000.0;
const MIN_PPTX_CREATE_CHART_MARKER_SIZE: u8 = 2;
const MAX_PPTX_CREATE_CHART_MARKER_SIZE: u8 = 72;
const DEFAULT_PPTX_CREATE_CHART_MARKER_SIZE: u8 = 5;
const PPTX_PRIMARY_CATEGORY_AXIS_ID: u32 = 45_756_800;
const PPTX_PRIMARY_VALUE_AXIS_ID: u32 = 45_710_656;
const PPTX_SECONDARY_CATEGORY_AXIS_ID: u32 = 45_756_801;
const PPTX_SECONDARY_VALUE_AXIS_ID: u32 = 45_710_657;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SlideLayout {
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
    fn parse(value: Option<&str>) -> Result<Self> {
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

    fn as_str(self) -> &'static str {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ImageFit {
    Contain,
    Cover,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum PresentationImageFormat {
    Png,
    Jpeg,
}

impl PresentationImageFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

#[derive(Debug)]
struct PresentationImage {
    source_path: String,
    bytes: Vec<u8>,
    format: PresentationImageFormat,
    width: u32,
    height: u32,
    alt_text: String,
    fit: ImageFit,
}

#[derive(Debug)]
struct PresentationTable {
    cells: Vec<Vec<String>>,
    header_row: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PresentationChartType {
    Column,
    Line,
    Pie,
    Area,
    Doughnut,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PresentationChartDataLabels {
    None,
    Value,
    Percentage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PresentationChartValueAxis {
    Primary,
    Secondary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PresentationChartValueAxisNumberFormat {
    General,
    Integer,
    Decimal1,
    Decimal2,
    Thousands,
    Thousands2,
    Percentage,
    Percentage1,
    Scientific,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PresentationChartAxisTickMark {
    None,
    Inside,
    Outside,
    Cross,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PresentationChartMarkerStyle {
    None,
    Circle,
    Square,
    Diamond,
    Triangle,
}

#[derive(Clone, Copy, Debug)]
struct PresentationChartValueAxisOptions {
    minimum: Option<f64>,
    maximum: Option<f64>,
    log_base: Option<f64>,
    major_tick_mark: PresentationChartAxisTickMark,
    minor_tick_mark: PresentationChartAxisTickMark,
    major_unit: Option<f64>,
    minor_unit: Option<f64>,
    number_format: PresentationChartValueAxisNumberFormat,
}

impl PresentationChartValueAxis {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "primary" => Ok(Self::Primary),
            "secondary" => Ok(Self::Secondary),
            value => Err(anyhow!("unsupported PPTX chart value_axis: {value}")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
        }
    }
}

impl PresentationChartValueAxisNumberFormat {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "general" => Ok(Self::General),
            "integer" => Ok(Self::Integer),
            "decimal_1" => Ok(Self::Decimal1),
            "decimal_2" => Ok(Self::Decimal2),
            "thousands" => Ok(Self::Thousands),
            "thousands_2" => Ok(Self::Thousands2),
            "percentage" => Ok(Self::Percentage),
            "percentage_1" => Ok(Self::Percentage1),
            "scientific" => Ok(Self::Scientific),
            value => Err(anyhow!(
                "unsupported PPTX chart value-axis number format: {value}"
            )),
        }
    }

    fn from_ooxml(format_code: &str, source_linked: bool) -> Result<Self> {
        match (format_code, source_linked) {
            ("General", true) => Ok(Self::General),
            ("0", false) => Ok(Self::Integer),
            ("0.0", false) => Ok(Self::Decimal1),
            ("0.00", false) => Ok(Self::Decimal2),
            ("#,##0", false) => Ok(Self::Thousands),
            ("#,##0.00", false) => Ok(Self::Thousands2),
            ("0%", false) => Ok(Self::Percentage),
            ("0.0%", false) => Ok(Self::Percentage1),
            ("0.00E+00", false) => Ok(Self::Scientific),
            _ => Err(anyhow!(
                "chart value-axis number format is outside the canonical bounded contract"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Integer => "integer",
            Self::Decimal1 => "decimal_1",
            Self::Decimal2 => "decimal_2",
            Self::Thousands => "thousands",
            Self::Thousands2 => "thousands_2",
            Self::Percentage => "percentage",
            Self::Percentage1 => "percentage_1",
            Self::Scientific => "scientific",
        }
    }

    fn format_code(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Integer => "0",
            Self::Decimal1 => "0.0",
            Self::Decimal2 => "0.00",
            Self::Thousands => "#,##0",
            Self::Thousands2 => "#,##0.00",
            Self::Percentage => "0%",
            Self::Percentage1 => "0.0%",
            Self::Scientific => "0.00E+00",
        }
    }

    fn source_linked(self) -> bool {
        self == Self::General
    }
}

impl PresentationChartAxisTickMark {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "none" => Ok(Self::None),
            "inside" => Ok(Self::Inside),
            "outside" => Ok(Self::Outside),
            "cross" => Ok(Self::Cross),
            value => Err(anyhow!(
                "unsupported PPTX chart value-axis tick mark: {value}"
            )),
        }
    }

    fn from_ooxml(value: &str) -> Result<Self> {
        match value {
            "none" => Ok(Self::None),
            "in" => Ok(Self::Inside),
            "out" => Ok(Self::Outside),
            "cross" => Ok(Self::Cross),
            value => Err(anyhow!(
                "chart value-axis tick mark is outside the canonical bounded contract: {value}"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Inside => "inside",
            Self::Outside => "outside",
            Self::Cross => "cross",
        }
    }

    fn as_ooxml(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Inside => "in",
            Self::Outside => "out",
            Self::Cross => "cross",
        }
    }
}

impl PresentationChartMarkerStyle {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "none" => Ok(Self::None),
            "circle" => Ok(Self::Circle),
            "square" => Ok(Self::Square),
            "diamond" => Ok(Self::Diamond),
            "triangle" => Ok(Self::Triangle),
            value => Err(anyhow!(
                "unsupported PPTX line-chart series marker style: {value}"
            )),
        }
    }

    fn from_ooxml(value: &str) -> Result<Self> {
        Self::parse(value).with_context(|| {
            format!(
                "chart line-series marker style is outside the canonical bounded contract: {value}"
            )
        })
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Circle => "circle",
            Self::Square => "square",
            Self::Diamond => "diamond",
            Self::Triangle => "triangle",
        }
    }

    fn as_ooxml(self) -> &'static str {
        self.as_str()
    }
}

impl PresentationChartDataLabels {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "none" => Ok(Self::None),
            "value" => Ok(Self::Value),
            "percentage" => Ok(Self::Percentage),
            value => Err(anyhow!("unsupported PPTX chart data_labels mode: {value}")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Value => "value",
            Self::Percentage => "percentage",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PresentationChartLegendPosition {
    Right,
    Left,
    Top,
    Bottom,
}

impl PresentationChartLegendPosition {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "right" => Ok(Self::Right),
            "left" => Ok(Self::Left),
            "top" => Ok(Self::Top),
            "bottom" => Ok(Self::Bottom),
            value => Err(anyhow!("unsupported PPTX chart legend_position: {value}")),
        }
    }

    fn from_ooxml(value: &str) -> Result<Self> {
        match value {
            "r" => Ok(Self::Right),
            "l" => Ok(Self::Left),
            "t" => Ok(Self::Top),
            "b" => Ok(Self::Bottom),
            value => Err(anyhow!(
                "chart legend position is outside the canonical right, left, top, or bottom contract: {value}"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Left => "left",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }

    fn as_ooxml(self) -> &'static str {
        match self {
            Self::Right => "r",
            Self::Left => "l",
            Self::Top => "t",
            Self::Bottom => "b",
        }
    }
}

impl PresentationChartType {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "column" => Ok(Self::Column),
            "line" => Ok(Self::Line),
            "pie" => Ok(Self::Pie),
            "area" => Ok(Self::Area),
            "doughnut" => Ok(Self::Doughnut),
            value => Err(anyhow!("unsupported PPTX chart type: {value}")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Column => "column",
            Self::Line => "line",
            Self::Pie => "pie",
            Self::Area => "area",
            Self::Doughnut => "doughnut",
        }
    }

    fn is_part_to_whole(self) -> bool {
        matches!(self, Self::Pie | Self::Doughnut)
    }
}

#[derive(Clone, Debug)]
struct PresentationChartSeries {
    name: String,
    values: Vec<f64>,
    value_axis: PresentationChartValueAxis,
    color: Option<String>,
    marker_style: Option<PresentationChartMarkerStyle>,
    marker_size: Option<u8>,
    smooth: Option<bool>,
}

#[derive(Clone, Debug)]
struct PresentationChart {
    chart_type: PresentationChartType,
    title: String,
    categories: Vec<String>,
    series: Vec<PresentationChartSeries>,
    show_legend: bool,
    legend_position: PresentationChartLegendPosition,
    data_labels: PresentationChartDataLabels,
    category_axis_title: String,
    value_axis_title: String,
    secondary_value_axis_title: String,
    value_axis_minimum: Option<f64>,
    value_axis_maximum: Option<f64>,
    value_axis_log_base: Option<f64>,
    value_axis_major_tick_mark: PresentationChartAxisTickMark,
    value_axis_minor_tick_mark: PresentationChartAxisTickMark,
    value_axis_major_unit: Option<f64>,
    value_axis_minor_unit: Option<f64>,
    value_axis_number_format: PresentationChartValueAxisNumberFormat,
    secondary_value_axis_minimum: Option<f64>,
    secondary_value_axis_maximum: Option<f64>,
    secondary_value_axis_log_base: Option<f64>,
    secondary_value_axis_major_tick_mark: PresentationChartAxisTickMark,
    secondary_value_axis_minor_tick_mark: PresentationChartAxisTickMark,
    secondary_value_axis_major_unit: Option<f64>,
    secondary_value_axis_minor_unit: Option<f64>,
    secondary_value_axis_number_format: PresentationChartValueAxisNumberFormat,
}

#[derive(Debug)]
struct SlideDefinition {
    title: String,
    body: String,
    left_body: String,
    right_body: String,
    notes: String,
    layout: SlideLayout,
    image: Option<PresentationImage>,
    table: Option<PresentationTable>,
    chart: Option<PresentationChart>,
}

#[derive(Debug)]
struct PresentationSlideMetadata {
    slide_ids: Vec<u32>,
    relationship_ids: Vec<String>,
    max_slide_id: u32,
    slide_tag_name: String,
    relationship_attribute_name: String,
}

#[derive(Debug)]
struct PackageRelationship {
    id: String,
    relationship_type: String,
    target: String,
    external: bool,
}

#[derive(Debug)]
struct RelationshipDocument {
    relationships: Vec<PackageRelationship>,
    relationship_tag_name: String,
}

#[derive(Debug)]
struct RelationshipAddition {
    id: String,
    relationship_type: &'static str,
    target: String,
}

#[derive(Debug)]
struct ContentTypesMetadata {
    defaults: HashMap<String, String>,
    overrides: HashSet<String>,
    default_tag_name: String,
    override_tag_name: String,
}

#[derive(Clone, Copy)]
struct PptxXmlElementRange {
    start: usize,
    open_end: usize,
    close_start: usize,
    end: usize,
}

#[derive(Clone)]
struct SimplePptxTextRun {
    run_start: usize,
    run_end: usize,
    text_start: usize,
    text_open_end: usize,
    text_close_end: usize,
    formatting: String,
    decoded: String,
}

struct PptxCrossRunTextMatch {
    runs: Vec<SimplePptxTextRun>,
    first_offset: usize,
    last_offset: usize,
}

struct PptxCrossRunScan {
    occurrences: usize,
    matched: Option<PptxCrossRunTextMatch>,
    unsupported_reason: Option<String>,
}

#[derive(Clone)]
struct SimplePptxTableCell {
    row: usize,
    column: usize,
    range: PptxXmlElementRange,
    text_start: usize,
    text_open_end: usize,
    text_close_end: usize,
    decoded: String,
}

struct SimplePptxTable {
    range: PptxXmlElementRange,
    rows: usize,
    columns: usize,
    cells: Vec<SimplePptxTableCell>,
}

#[derive(Clone, Copy)]
struct SimplePptxTableRow {
    range: PptxXmlElementRange,
    height: i64,
}

#[derive(Clone, Copy)]
struct SimplePptxTableColumn {
    range: PptxXmlElementRange,
    width: i64,
}

struct PptxTableScan {
    rows: usize,
    columns: usize,
    cells: usize,
    cell_text: Vec<Vec<String>>,
    cell_text_truncated: bool,
    simple: Option<SimplePptxTable>,
    unsupported_reason: Option<String>,
}

#[derive(Clone, Debug)]
struct ResolvedPptxChartReference {
    relationship_id: String,
    part: String,
}

#[derive(Clone, Debug, Default)]
struct PptxChartSeriesInspection {
    chart_type: String,
    chart_group_index: usize,
    value_axis: String,
    color: Option<String>,
    color_value: Option<String>,
    color_style_present: bool,
    color_style_custom: bool,
    color_shape_properties_count: usize,
    color_line_count: usize,
    color_solid_fill_count: usize,
    color_srgb_count: usize,
    marker_style: Option<String>,
    marker_style_value: Option<String>,
    marker_size: Option<u8>,
    marker_size_value: Option<String>,
    marker_style_custom: bool,
    marker_count: usize,
    marker_symbol_count: usize,
    marker_size_count: usize,
    smooth: Option<bool>,
    smooth_value: Option<String>,
    smooth_custom: bool,
    smooth_count: usize,
    name: String,
    name_formula: Option<String>,
    category_formula: Option<String>,
    value_formula: Option<String>,
    bubble_size_formula: Option<String>,
    categories: Vec<String>,
    values: Vec<String>,
    bubble_sizes: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct PptxChartGroupInspection {
    chart_type: String,
    axis_ids: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct PptxChartAxisInspection {
    axis_type: String,
    axis_id: Option<String>,
    position: Option<String>,
    title: String,
    title_formula: Option<String>,
    title_truncated: bool,
    minimum: Option<String>,
    maximum: Option<String>,
    log_base: Option<String>,
    major_tick_mark: Option<String>,
    minor_tick_mark: Option<String>,
    major_unit: Option<String>,
    minor_unit: Option<String>,
    number_format_code: Option<String>,
    number_format_source_linked: Option<bool>,
}

struct PptxChartInspection {
    chart_types: Vec<String>,
    chart_groups: Vec<PptxChartGroupInspection>,
    axes: Vec<PptxChartAxisInspection>,
    title: String,
    title_formula: Option<String>,
    title_truncated: bool,
    series: Vec<PptxChartSeriesInspection>,
    cached_points: usize,
    legend_count: usize,
    legend_positions: Vec<String>,
    data_label_group_count: usize,
    data_label_show_value_count: usize,
    data_label_show_percentage_count: usize,
    category_axis_title: String,
    category_axis_title_formula: Option<String>,
    category_axis_title_truncated: bool,
    value_axis_title: String,
    value_axis_title_formula: Option<String>,
    value_axis_title_truncated: bool,
    secondary_value_axis_title: String,
    secondary_value_axis_title_formula: Option<String>,
    secondary_value_axis_title_truncated: bool,
}

struct PptxChartRelationshipInspection {
    data_source: &'static str,
    relationship_count: usize,
    embedded_workbook: Option<String>,
    relationships_part_present: bool,
}

struct PptxChartOwnership {
    ordered_slide_paths: Vec<String>,
    charts_by_slide: Vec<Vec<ResolvedPptxChartReference>>,
    chart_count: usize,
}

#[derive(Clone, Debug)]
struct OwnedNotesPart {
    path: String,
    relationships_path: String,
}

pub(super) fn create_pptx(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let target = required_text(arguments, "target_path")?;
    require_extension(target, ".pptx")?;
    let slides = parse_slides(arguments, state, request)?;
    let entries = presentation_entries(slides.as_slice())?;
    let (path, relative) = safe_workspace_path(state, request, target)?;
    let bytes = write_new_pptx(
        path.as_path(),
        entries,
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "path": relative,
        "bytes": bytes,
        "slides": slides.len(),
        "layouts": slides.iter().map(|slide| slide.layout.as_str()).collect::<Vec<_>>(),
        "images": slides.iter().filter(|slide| slide.image.is_some()).count(),
        "charts": slides.iter().filter(|slide| slide.chart.is_some()).count(),
        "chart_types": slides.iter().filter_map(|slide| slide.chart.as_ref().map(|chart| chart.chart_type.as_str())).collect::<Vec<_>>(),
        "speaker_notes": slides.iter().filter(|slide| !slide.notes.is_empty()).count(),
        "widescreen": true,
    }))
}

pub(super) fn append_pptx_slides(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let slides = parse_slides(arguments, state, request)?;
    let names = validate_pptx_package(source.as_path())?;
    for required in [
        "[Content_Types].xml",
        "ppt/presentation.xml",
        "ppt/_rels/presentation.xml.rels",
    ] {
        if !names.contains(required) {
            return Err(anyhow!("PPTX is missing required package part: {required}"));
        }
    }

    let existing_slide_parts = names
        .iter()
        .filter_map(|name| slide_part_number(name.as_str()).map(|number| (number, name.clone())))
        .collect::<Vec<_>>();
    if existing_slide_parts.is_empty()
        || existing_slide_parts.len().saturating_add(slides.len()) > MAX_PPTX_SLIDES
    {
        return Err(anyhow!(
            "appended PPTX must contain between 1 and {MAX_PPTX_SLIDES} slides"
        ));
    }

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open PPTX {}", source.display()))?;
    let content_types_xml = read_zip_text(&mut archive, "[Content_Types].xml")?;
    let presentation_xml = read_zip_text(&mut archive, "ppt/presentation.xml")?;
    let presentation_relationships_xml =
        read_zip_text(&mut archive, "ppt/_rels/presentation.xml.rels")?;
    let slide_metadata = presentation_slide_metadata(presentation_xml.as_str())?;
    if slide_metadata.relationship_ids.len() != existing_slide_parts.len() {
        return Err(anyhow!(
            "PPTX contains unreferenced or missing slide parts; conservative append was refused"
        ));
    }
    let presentation_relationships = parse_relationship_document(
        presentation_relationships_xml.as_str(),
        "ppt/presentation.xml",
    )?;
    let relationships_by_id = presentation_relationships
        .relationships
        .iter()
        .map(|relationship| (relationship.id.as_str(), relationship))
        .collect::<HashMap<_, _>>();
    let mut ordered_slide_paths = Vec::with_capacity(slide_metadata.relationship_ids.len());
    let mut referenced_slide_paths = HashSet::new();
    for relationship_id in &slide_metadata.relationship_ids {
        let relationship = relationships_by_id
            .get(relationship_id.as_str())
            .ok_or_else(|| {
                anyhow!("PPTX presentation references a missing relationship: {relationship_id}")
            })?;
        if relationship.external || !relationship.relationship_type.ends_with("/slide") {
            return Err(anyhow!(
                "PPTX presentation slide relationship is external or has an unexpected type"
            ));
        }
        let path = resolve_part_target("ppt/presentation.xml", relationship.target.as_str())?;
        if !names.contains(path.as_str()) || !referenced_slide_paths.insert(path.clone()) {
            return Err(anyhow!(
                "PPTX presentation contains a missing or duplicate slide reference"
            ));
        }
        ordered_slide_paths.push(path);
    }
    let package_slide_paths = existing_slide_parts
        .iter()
        .map(|(_, path)| path.clone())
        .collect::<HashSet<_>>();
    if referenced_slide_paths != package_slide_paths {
        return Err(anyhow!(
            "PPTX contains unreferenced or missing slide parts; conservative append was refused"
        ));
    }

    let reference_slide = ordered_slide_paths
        .last()
        .ok_or_else(|| anyhow!("PPTX has no slide available for layout inheritance"))?;
    let reference_slide_relationships_path = relationships_part_path(reference_slide.as_str())?;
    if !names.contains(reference_slide_relationships_path.as_str()) {
        return Err(anyhow!(
            "PPTX reference slide is missing its relationship part"
        ));
    }
    let reference_slide_relationships_xml =
        read_zip_text(&mut archive, reference_slide_relationships_path.as_str())?;
    let reference_slide_relationships = parse_relationship_document(
        reference_slide_relationships_xml.as_str(),
        reference_slide.as_str(),
    )?;
    let layout_relationships = reference_slide_relationships
        .relationships
        .iter()
        .filter(|relationship| relationship.relationship_type.ends_with("/slideLayout"))
        .collect::<Vec<_>>();
    if layout_relationships.len() != 1 || layout_relationships[0].external {
        return Err(anyhow!(
            "PPTX reference slide must contain exactly one internal slide layout relationship"
        ));
    }
    let layout_path = resolve_part_target(
        reference_slide.as_str(),
        layout_relationships[0].target.as_str(),
    )?;
    if !names.contains(layout_path.as_str()) {
        return Err(anyhow!(
            "PPTX is missing inherited slide layout: {layout_path}"
        ));
    }

    let notes_requested = slides.iter().any(|slide| !slide.notes.is_empty());
    let notes_master_path = if notes_requested {
        let notes_master_relationships = presentation_relationships
            .relationships
            .iter()
            .filter(|relationship| relationship.relationship_type.ends_with("/notesMaster"))
            .collect::<Vec<_>>();
        if notes_master_relationships.len() != 1 || notes_master_relationships[0].external {
            return Err(anyhow!(
                "appending speaker notes requires exactly one existing internal notes master"
            ));
        }
        let path = resolve_part_target(
            "ppt/presentation.xml",
            notes_master_relationships[0].target.as_str(),
        )?;
        if !names.contains(path.as_str()) {
            return Err(anyhow!("PPTX is missing referenced notes master: {path}"));
        }
        Some(path)
    } else {
        None
    };

    let content_types = content_types_metadata(content_types_xml.as_str())?;
    let mut used_relationship_ids = presentation_relationships
        .relationships
        .iter()
        .map(|relationship| relationship.id.clone())
        .collect::<HashSet<_>>();
    let mut next_slide_number = existing_slide_parts
        .iter()
        .map(|(number, _)| *number)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| anyhow!("PPTX slide part number overflow"))?;
    let mut next_notes_number = names
        .iter()
        .filter_map(|name| numbered_part(name, "ppt/notesSlides/notesSlide", ".xml"))
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| anyhow!("PPTX notes part number overflow"))?;
    let mut next_media_number = 1usize;
    let mut next_chart_number = names
        .iter()
        .filter_map(|name| numbered_part(name, "ppt/charts/chart", ".xml"))
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| anyhow!("PPTX chart part number overflow"))?;
    let mut next_slide_id = slide_metadata
        .max_slide_id
        .max(255)
        .checked_add(1)
        .ok_or_else(|| anyhow!("PPTX slide identifier overflow"))?;
    let mut additions = Vec::<(String, Vec<u8>)>::new();
    let mut addition_names = HashSet::new();
    let mut content_type_overrides = Vec::<(String, &'static str)>::new();
    let mut required_image_defaults = HashMap::<String, &'static str>::new();
    let mut presentation_relationship_additions = Vec::with_capacity(slides.len());
    let mut presentation_slide_additions = Vec::with_capacity(slides.len());
    let mut appended_images = 0usize;
    let mut appended_charts = 0usize;
    let mut appended_notes = 0usize;

    for slide in &slides {
        while names.contains(format!("ppt/slides/slide{next_slide_number}.xml").as_str())
            || names
                .contains(format!("ppt/slides/_rels/slide{next_slide_number}.xml.rels").as_str())
        {
            next_slide_number = next_slide_number
                .checked_add(1)
                .ok_or_else(|| anyhow!("PPTX slide part number overflow"))?;
        }
        let slide_number = next_slide_number;
        next_slide_number = next_slide_number
            .checked_add(1)
            .ok_or_else(|| anyhow!("PPTX slide part number overflow"))?;
        let slide_path = format!("ppt/slides/slide{slide_number}.xml");
        let slide_relationships_path = relationships_part_path(slide_path.as_str())?;
        let layout_target = relative_part_target(slide_path.as_str(), layout_path.as_str())?;

        let image = if let Some(image) = &slide.image {
            let extension = image.format.extension();
            let media_path = loop {
                let candidate = format!("ppt/media/chatosImage{next_media_number}.{extension}");
                next_media_number = next_media_number
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("PPTX media part number overflow"))?;
                if !names.contains(candidate.as_str())
                    && !addition_names.contains(candidate.as_str())
                {
                    break candidate;
                }
            };
            addition_names.insert(media_path.clone());
            additions.push((media_path.clone(), image.bytes.clone()));
            required_image_defaults.insert(
                extension.to_string(),
                match image.format {
                    PresentationImageFormat::Png => "image/png",
                    PresentationImageFormat::Jpeg => "image/jpeg",
                },
            );
            appended_images = appended_images.saturating_add(1);
            Some(relative_part_target(
                slide_path.as_str(),
                media_path.as_str(),
            )?)
        } else {
            None
        };

        let chart = if let Some(chart) = &slide.chart {
            let chart_path = loop {
                let candidate = format!("ppt/charts/chart{next_chart_number}.xml");
                next_chart_number = next_chart_number
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("PPTX chart part number overflow"))?;
                if !names.contains(candidate.as_str())
                    && !addition_names.contains(candidate.as_str())
                {
                    break candidate;
                }
            };
            addition_names.insert(chart_path.clone());
            additions.push((
                chart_path.clone(),
                presentation_chart_xml(chart)?.into_bytes(),
            ));
            content_type_overrides.push((
                format!("/{chart_path}"),
                "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
            ));
            appended_charts = appended_charts.saturating_add(1);
            Some(relative_part_target(
                slide_path.as_str(),
                chart_path.as_str(),
            )?)
        } else {
            None
        };

        let notes = if slide.notes.is_empty() {
            None
        } else {
            let notes_master_path = notes_master_path
                .as_ref()
                .expect("notes master validated when notes are requested");
            while names
                .contains(format!("ppt/notesSlides/notesSlide{next_notes_number}.xml").as_str())
                || names.contains(
                    format!("ppt/notesSlides/_rels/notesSlide{next_notes_number}.xml.rels")
                        .as_str(),
                )
            {
                next_notes_number = next_notes_number
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("PPTX notes part number overflow"))?;
            }
            let notes_number = next_notes_number;
            next_notes_number = next_notes_number
                .checked_add(1)
                .ok_or_else(|| anyhow!("PPTX notes part number overflow"))?;
            let notes_path = format!("ppt/notesSlides/notesSlide{notes_number}.xml");
            let notes_relationships_path = relationships_part_path(notes_path.as_str())?;
            let notes_master_target =
                relative_part_target(notes_path.as_str(), notes_master_path.as_str())?;
            let slide_target = relative_part_target(notes_path.as_str(), slide_path.as_str())?;
            addition_names.insert(notes_path.clone());
            addition_names.insert(notes_relationships_path.clone());
            additions.push((
                notes_path.clone(),
                notes_slide_xml(slide.notes.as_str(), slide_number)?.into_bytes(),
            ));
            additions.push((
                notes_relationships_path,
                appended_notes_slide_relationships(
                    notes_master_target.as_str(),
                    slide_target.as_str(),
                )
                .into_bytes(),
            ));
            content_type_overrides.push((
                format!("/{notes_path}"),
                "application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml",
            ));
            appended_notes = appended_notes.saturating_add(1);
            Some(relative_part_target(
                slide_path.as_str(),
                notes_path.as_str(),
            )?)
        };

        let image_relationship_id = image.as_ref().map(|_| "rId2");
        let chart_relationship_id =
            chart
                .as_ref()
                .map(|_| if image.is_some() { "rId3" } else { "rId2" });
        addition_names.insert(slide_path.clone());
        addition_names.insert(slide_relationships_path.clone());
        additions.push((
            slide_path.clone(),
            slide_xml(slide, image_relationship_id, chart_relationship_id)?.into_bytes(),
        ));
        additions.push((
            slide_relationships_path,
            appended_slide_relationships(
                layout_target.as_str(),
                image.as_deref(),
                chart.as_deref(),
                notes.as_deref(),
            )
            .into_bytes(),
        ));
        content_type_overrides.push((
            format!("/{slide_path}"),
            "application/vnd.openxmlformats-officedocument.presentationml.slide+xml",
        ));

        let relationship_id = next_relationship_id(&mut used_relationship_ids)?;
        presentation_relationship_additions.push(RelationshipAddition {
            id: relationship_id.clone(),
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide",
            target: relative_part_target("ppt/presentation.xml", slide_path.as_str())?,
        });
        presentation_slide_additions.push((next_slide_id, relationship_id));
        next_slide_id = next_slide_id
            .checked_add(1)
            .ok_or_else(|| anyhow!("PPTX slide identifier overflow"))?;
    }
    drop(archive);

    let updated_presentation = append_presentation_slide_ids(
        presentation_xml.as_str(),
        &slide_metadata,
        presentation_slide_additions.as_slice(),
    )?;
    let updated_presentation_relationships = append_relationship_entries(
        presentation_relationships_xml.as_str(),
        presentation_relationships.relationship_tag_name.as_str(),
        presentation_relationship_additions.as_slice(),
    )?;
    let updated_content_types = append_content_type_entries(
        content_types_xml.as_str(),
        &content_types,
        &required_image_defaults,
        content_type_overrides.as_slice(),
    )?;
    let replacements = BTreeMap::from([
        (
            "[Content_Types].xml".to_string(),
            updated_content_types.into_bytes(),
        ),
        (
            "ppt/presentation.xml".to_string(),
            updated_presentation.into_bytes(),
        ),
        (
            "ppt/_rels/presentation.xml.rels".to_string(),
            updated_presentation_relationships.into_bytes(),
        ),
    ]);
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        additions,
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "previous_slides": existing_slide_parts.len(),
        "appended_slides": slides.len(),
        "slides": existing_slide_parts.len().saturating_add(slides.len()),
        "appended_images": appended_images,
        "appended_charts": appended_charts,
        "appended_chart_types": slides.iter().filter_map(|slide| slide.chart.as_ref().map(|chart| chart.chart_type.as_str())).collect::<Vec<_>>(),
        "appended_speaker_notes": appended_notes,
        "inherited_slide_layout": layout_path,
        "bytes": bytes,
    }))
}

pub(super) fn replace_pptx_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let find = required_text(arguments, "find")?;
    let replacement = arguments
        .get("replacement")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("replacement must be a string"))?;
    if find.is_empty() || find.chars().count() > 10_000 {
        return Err(anyhow!("find must contain between 1 and 10000 characters"));
    }
    validate_slide_text(find, "find", 10_000)?;
    validate_slide_text(replacement, "replacement", MAX_SLIDE_TEXT_CHARS)?;
    if find == replacement {
        return Err(anyhow!(
            "PPTX text replacement must change the matched text"
        ));
    }
    let max_replacements = match arguments.get("max_replacements") {
        None => 100usize,
        Some(Value::Number(value)) => value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| (1..=10_000).contains(value))
            .ok_or_else(|| anyhow!("max_replacements must be an integer between 1 and 10000"))?,
        Some(_) => {
            return Err(anyhow!(
                "max_replacements must be an integer between 1 and 10000"
            ));
        }
    };

    let names = validate_pptx_package(source.as_path())?;
    for required in ["ppt/presentation.xml", "ppt/_rels/presentation.xml.rels"] {
        if !names.contains(required) {
            return Err(anyhow!("PPTX is missing required package part: {required}"));
        }
    }
    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open PPTX {}", source.display()))?;
    let presentation_xml = read_zip_text(&mut archive, "ppt/presentation.xml")?;
    let presentation_relationships_xml =
        read_zip_text(&mut archive, "ppt/_rels/presentation.xml.rels")?;
    let slide_metadata = presentation_slide_metadata(presentation_xml.as_str())?;
    let presentation_relationships = parse_relationship_document(
        presentation_relationships_xml.as_str(),
        "ppt/presentation.xml",
    )?;
    let ordered_slide_paths =
        ordered_presentation_slide_paths(&slide_metadata, &presentation_relationships, &names)?;
    if ordered_slide_paths.len() > MAX_PPTX_SLIDES {
        return Err(anyhow!(
            "PPTX slide count exceeds the {MAX_PPTX_SLIDES} slide editing limit"
        ));
    }
    let selected_positions = selected_slide_positions(arguments, ordered_slide_paths.len())?;
    let mut replacements = BTreeMap::new();
    let mut replacement_count = 0usize;
    let mut matched_slides = Vec::new();
    let mut replacement_limit_reached = false;
    for position in selected_positions {
        let slide_path = ordered_slide_paths
            .get(position - 1)
            .expect("selected slide position validated");
        let slide_xml = read_zip_text(&mut archive, slide_path.as_str())?;
        let remaining = max_replacements.saturating_sub(replacement_count);
        let (updated, count, limit_reached) =
            replace_drawing_text_runs(slide_xml.as_str(), find, replacement, remaining)?;
        if count > 0 {
            replacement_count = replacement_count.saturating_add(count);
            matched_slides.push(position);
            replacements.insert(slide_path.clone(), updated.into_bytes());
        }
        replacement_limit_reached |= limit_reached;
    }
    drop(archive);
    if replacement_count == 0 {
        return Err(anyhow!(
            "PPTX text was not found inside a single visible DrawingML text run"
        ));
    }
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "matched_slides": matched_slides,
        "replacements": replacement_count,
        "replacement_limit_reached": replacement_limit_reached,
        "bytes": bytes,
    }))
}

pub(super) fn replace_pptx_text_across_runs(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let selection = arguments
        .get("selection")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("selection must be a non-empty string"))?;
    let replacement = arguments
        .get("replacement")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("replacement must be a string"))?;
    if selection.chars().count() > 10_000 {
        return Err(anyhow!(
            "selection exceeds the 10000 character safety limit"
        ));
    }
    validate_slide_text(selection, "selection", 10_000)?;
    validate_slide_text(replacement, "replacement", MAX_SLIDE_TEXT_CHARS)?;
    if selection == replacement {
        return Err(anyhow!(
            "PPTX cross-run replacement must change the selected text"
        ));
    }

    let names = validate_pptx_package(source.as_path())?;
    for required in ["ppt/presentation.xml", "ppt/_rels/presentation.xml.rels"] {
        if !names.contains(required) {
            return Err(anyhow!("PPTX is missing required package part: {required}"));
        }
    }
    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open PPTX {}", source.display()))?;
    let presentation_xml = read_zip_text(&mut archive, "ppt/presentation.xml")?;
    let presentation_relationships_xml =
        read_zip_text(&mut archive, "ppt/_rels/presentation.xml.rels")?;
    let slide_metadata = presentation_slide_metadata(presentation_xml.as_str())?;
    let presentation_relationships = parse_relationship_document(
        presentation_relationships_xml.as_str(),
        "ppt/presentation.xml",
    )?;
    let ordered_slide_paths =
        ordered_presentation_slide_paths(&slide_metadata, &presentation_relationships, &names)?;
    if ordered_slide_paths.len() > MAX_PPTX_SLIDES {
        return Err(anyhow!(
            "PPTX slide count exceeds the {MAX_PPTX_SLIDES} slide editing limit"
        ));
    }
    let selected_positions = selected_slide_positions(arguments, ordered_slide_paths.len())?;
    let mut total_occurrences = 0usize;
    let mut matched = None::<(usize, String, String, PptxCrossRunTextMatch)>;
    let mut unsupported_reason = None::<String>;
    for position in selected_positions {
        let slide_path = ordered_slide_paths
            .get(position - 1)
            .expect("selected slide position validated");
        let slide_xml = read_zip_text(&mut archive, slide_path.as_str())?;
        let scan = scan_pptx_cross_run_text(slide_xml.as_str(), selection)?;
        total_occurrences = total_occurrences.saturating_add(scan.occurrences);
        if total_occurrences > 1 {
            return Err(anyhow!(
                "selection must appear exactly once in visible PPTX paragraph text across the selected slides"
            ));
        }
        if scan.occurrences == 1 {
            unsupported_reason = scan.unsupported_reason;
            if let Some(candidate) = scan.matched {
                matched = Some((position, slide_path.clone(), slide_xml, candidate));
            }
        }
    }
    drop(archive);
    if total_occurrences == 0 {
        return Err(anyhow!(
            "selection was not present in visible PPTX paragraph text across the selected slides"
        ));
    }
    let (matched_slide, slide_path, slide_xml, matched) = matched.ok_or_else(|| {
        anyhow!(
            "selection is not an eligible same-format adjacent cross-run PPTX match: {}",
            unsupported_reason.unwrap_or_else(|| "unsupported DrawingML structure".to_string())
        )
    })?;
    let (updated_xml, runs_touched, emptied_runs) =
        rewrite_pptx_cross_run_match(slide_xml.as_str(), &matched, replacement)?;
    let replacements = BTreeMap::from([(slide_path, updated_xml.into_bytes())]);
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "operation": "replace_text_across_runs",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "matched_slide": matched_slide,
        "replacements": 1,
        "runs_touched": runs_touched,
        "emptied_runs": emptied_runs,
        "same_run_properties": true,
        "globally_unique_match": true,
        "bytes": bytes,
    }))
}

pub(super) fn inspect_pptx_table(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let slide_number = required_pptx_index(arguments, "slide_number", MAX_PPTX_SLIDES)?;
    let table_number = required_pptx_index(arguments, "table_number", MAX_PPTX_TABLES_PER_SLIDE)?;
    let names = validate_pptx_package(source.as_path())?;
    for required in ["ppt/presentation.xml", "ppt/_rels/presentation.xml.rels"] {
        if !names.contains(required) {
            return Err(anyhow!("PPTX is missing required package part: {required}"));
        }
    }
    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open PPTX {}", source.display()))?;
    let presentation_xml = read_zip_text(&mut archive, "ppt/presentation.xml")?;
    let presentation_relationships_xml =
        read_zip_text(&mut archive, "ppt/_rels/presentation.xml.rels")?;
    let slide_metadata = presentation_slide_metadata(presentation_xml.as_str())?;
    let presentation_relationships = parse_relationship_document(
        presentation_relationships_xml.as_str(),
        "ppt/presentation.xml",
    )?;
    let ordered_slide_paths =
        ordered_presentation_slide_paths(&slide_metadata, &presentation_relationships, &names)?;
    let slide_path = ordered_slide_paths.get(slide_number - 1).ok_or_else(|| {
        anyhow!(
            "slide_number {slide_number} is out-of-range for a PPTX with {} visible slides",
            ordered_slide_paths.len()
        )
    })?;
    let slide_xml = read_zip_text(&mut archive, slide_path.as_str())?;
    let tables = scan_pptx_tables(slide_xml.as_str())?;
    let table = tables.get(table_number - 1).ok_or_else(|| {
        anyhow!(
            "table_number {table_number} is out-of-range for visible slide {slide_number}, which contains {} tables",
            tables.len()
        )
    })?;
    let (eligible_for_row_editing, row_editing_unsupported_reason) = match table.simple.as_ref() {
        Some(simple) => match simple_pptx_table_rows(slide_xml.as_str(), simple) {
            Ok(_) => (true, None),
            Err(error) => (false, Some(error.to_string())),
        },
        None => (
            false,
            Some(
                table
                    .unsupported_reason
                    .clone()
                    .unwrap_or_else(|| "unsupported DrawingML table structure".to_string()),
            ),
        ),
    };
    let (eligible_for_column_editing, column_editing_unsupported_reason) =
        match table.simple.as_ref() {
            Some(simple) => match simple_pptx_table_columns(slide_xml.as_str(), simple) {
                Ok(_) => (true, None),
                Err(error) => (false, Some(error.to_string())),
            },
            None => (
                false,
                Some(
                    table
                        .unsupported_reason
                        .clone()
                        .unwrap_or_else(|| "unsupported DrawingML table structure".to_string()),
                ),
            ),
        };
    let cell_xml_sha256 = table
        .simple
        .as_ref()
        .map(|simple| simple_pptx_table_cell_xml_sha256(slide_xml.as_str(), simple));
    Ok(json!({
        "path": source_relative,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "table_number": table_number,
        "rows": table.rows,
        "columns": table.columns,
        "cells": table.cells,
        "cell_text": table.cell_text,
        "cell_text_truncated": table.cell_text_truncated,
        "cell_xml_sha256": cell_xml_sha256,
        "eligible_for_cell_replacement": table.simple.is_some(),
        "eligible_for_cell_format_copy": table.simple.is_some(),
        "unsupported_reason": table.unsupported_reason,
        "eligible_for_row_editing": eligible_for_row_editing,
        "row_editing_unsupported_reason": row_editing_unsupported_reason,
        "eligible_for_column_editing": eligible_for_column_editing,
        "column_editing_unsupported_reason": column_editing_unsupported_reason,
    }))
}

pub(super) fn copy_pptx_table_cell_format(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let slide_number = required_pptx_index(arguments, "slide_number", MAX_PPTX_SLIDES)?;
    let table_number = required_pptx_index(arguments, "table_number", MAX_PPTX_TABLES_PER_SLIDE)?;
    let row = required_pptx_index(arguments, "row", MAX_PPTX_TABLE_ROWS)?;
    let column = required_pptx_index(arguments, "column", MAX_PPTX_TABLE_COLUMNS)?;
    let reference_row = required_pptx_index(arguments, "reference_row", MAX_PPTX_TABLE_ROWS)?;
    let reference_column =
        required_pptx_index(arguments, "reference_column", MAX_PPTX_TABLE_COLUMNS)?;
    if row == reference_row && column == reference_column {
        return Err(anyhow!(
            "PPTX table format copy must select different target and reference cells"
        ));
    }
    let expected_text = arguments
        .get("expected_text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("expected_text must be a string"))?;
    let reference_expected_text = arguments
        .get("reference_expected_text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("reference_expected_text must be a string"))?;
    validate_slide_text(
        expected_text,
        "expected_text",
        MAX_PPTX_TABLE_CELL_TEXT_CHARS,
    )?;
    validate_slide_text(
        reference_expected_text,
        "reference_expected_text",
        MAX_PPTX_TABLE_CELL_TEXT_CHARS,
    )?;
    let expected_cell_xml_sha256 = required_pptx_sha256(arguments, "expected_cell_xml_sha256")?;
    let reference_expected_cell_xml_sha256 =
        required_pptx_sha256(arguments, "reference_expected_cell_xml_sha256")?;

    let (slide_path, slide_xml, table) =
        selected_pptx_table(source.as_path(), slide_number, table_number)?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "selected PPTX table is not eligible for simple cell format copying: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    for (label, selected_row, selected_column) in [
        ("target", row, column),
        ("reference", reference_row, reference_column),
    ] {
        if selected_row > simple.rows || selected_column > simple.columns {
            return Err(anyhow!(
                "{label} table cell ({selected_row}, {selected_column}) is out-of-range for a {rows}x{columns} PPTX table",
                rows = simple.rows,
                columns = simple.columns
            ));
        }
    }
    let selected_cell = |selected_row: usize, selected_column: usize| {
        simple
            .cells
            .iter()
            .find(|cell| cell.row == selected_row && cell.column == selected_column)
            .expect("simple PPTX table contains every rectangular cell")
    };
    let target_cell = selected_cell(row, column);
    let reference_cell = selected_cell(reference_row, reference_column);
    if target_cell.decoded != expected_text {
        return Err(anyhow!(
            "target PPTX table cell text does not match expected_text"
        ));
    }
    if reference_cell.decoded != reference_expected_text {
        return Err(anyhow!(
            "reference PPTX table cell text does not match reference_expected_text"
        ));
    }
    ensure_pptx_table_cell_xml_sha256(
        slide_xml.as_str(),
        target_cell,
        expected_cell_xml_sha256.as_str(),
        "target",
    )?;
    ensure_pptx_table_cell_xml_sha256(
        slide_xml.as_str(),
        reference_cell,
        reference_expected_cell_xml_sha256.as_str(),
        "reference",
    )?;

    let target_cell_xml = &slide_xml[target_cell.range.start..target_cell.range.end];
    let mut replacement_cell_xml =
        slide_xml[reference_cell.range.start..reference_cell.range.end].to_string();
    let reference_text_start = reference_cell.text_start - reference_cell.range.start;
    let reference_text_open_end = reference_cell.text_open_end - reference_cell.range.start;
    let reference_text_close_end = reference_cell.text_close_end - reference_cell.range.start;
    let reference_text_opening =
        &replacement_cell_xml[reference_text_start..reference_text_open_end];
    let target_text_opening = pptx_text_opening_for_value(reference_text_opening, expected_text)?;
    replacement_cell_xml.replace_range(
        reference_text_start..reference_text_close_end,
        format!("{target_text_opening}{}</a:t>", escape_xml(expected_text)).as_str(),
    );
    if replacement_cell_xml == target_cell_xml {
        return Err(anyhow!(
            "target PPTX table cell already has the reference cell formatting"
        ));
    }
    let updated_xml = apply_pptx_xml_edits(
        slide_xml.as_str(),
        vec![(
            target_cell.range.start,
            target_cell.range.end,
            replacement_cell_xml,
        )],
    )?;
    validate_updated_pptx_table_cells(
        updated_xml.as_str(),
        table_number,
        simple.rows,
        simple.columns,
    )?;
    let updated_tables = scan_pptx_tables(updated_xml.as_str())?;
    let updated_simple = updated_tables[table_number - 1]
        .simple
        .as_ref()
        .expect("validated updated PPTX table is simple");
    let updated_target = updated_simple
        .cells
        .iter()
        .find(|cell| cell.row == row && cell.column == column)
        .expect("validated updated PPTX table contains the target cell");
    if updated_target.decoded != expected_text {
        return Err(anyhow!(
            "PPTX table format copy unexpectedly changed the target cell text"
        ));
    }
    let updated_cell_xml_sha256 = pptx_table_cell_xml_sha256(updated_xml.as_str(), updated_target);
    let replacements = BTreeMap::from([(slide_path.clone(), updated_xml.into_bytes())]);
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "operation": "copy_table_cell_format",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "table_number": table_number,
        "row": row,
        "column": column,
        "reference_row": reference_row,
        "reference_column": reference_column,
        "target_text_preserved": true,
        "reference_text_not_copied": true,
        "cell_xml_sha256": updated_cell_xml_sha256,
        "bytes": bytes,
    }))
}

pub(super) fn replace_pptx_table_cell_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let slide_number = required_pptx_index(arguments, "slide_number", MAX_PPTX_SLIDES)?;
    let table_number = required_pptx_index(arguments, "table_number", MAX_PPTX_TABLES_PER_SLIDE)?;
    let row = required_pptx_index(arguments, "row", MAX_PPTX_TABLE_ROWS)?;
    let column = required_pptx_index(arguments, "column", MAX_PPTX_TABLE_COLUMNS)?;
    let expected_text = arguments
        .get("expected_text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("expected_text must be a string"))?;
    let replacement = arguments
        .get("replacement")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("replacement must be a string"))?;
    validate_slide_text(
        expected_text,
        "expected_text",
        MAX_PPTX_TABLE_CELL_TEXT_CHARS,
    )?;
    validate_slide_text(replacement, "replacement", MAX_PPTX_TABLE_CELL_TEXT_CHARS)?;
    if expected_text == replacement {
        return Err(anyhow!(
            "PPTX table cell replacement must change the selected cell text"
        ));
    }

    let names = validate_pptx_package(source.as_path())?;
    for required in ["ppt/presentation.xml", "ppt/_rels/presentation.xml.rels"] {
        if !names.contains(required) {
            return Err(anyhow!("PPTX is missing required package part: {required}"));
        }
    }
    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open PPTX {}", source.display()))?;
    let presentation_xml = read_zip_text(&mut archive, "ppt/presentation.xml")?;
    let presentation_relationships_xml =
        read_zip_text(&mut archive, "ppt/_rels/presentation.xml.rels")?;
    let slide_metadata = presentation_slide_metadata(presentation_xml.as_str())?;
    let presentation_relationships = parse_relationship_document(
        presentation_relationships_xml.as_str(),
        "ppt/presentation.xml",
    )?;
    let ordered_slide_paths =
        ordered_presentation_slide_paths(&slide_metadata, &presentation_relationships, &names)?;
    let slide_path = ordered_slide_paths.get(slide_number - 1).ok_or_else(|| {
        anyhow!(
            "slide_number {slide_number} is out-of-range for a PPTX with {} visible slides",
            ordered_slide_paths.len()
        )
    })?;
    let slide_xml = read_zip_text(&mut archive, slide_path.as_str())?;
    let tables = scan_pptx_tables(slide_xml.as_str())?;
    let table = tables.get(table_number - 1).ok_or_else(|| {
        anyhow!(
            "table_number {table_number} is out-of-range for visible slide {slide_number}, which contains {} tables",
            tables.len()
        )
    })?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "selected PPTX table is not eligible for simple cell replacement: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if row > simple.rows || column > simple.columns {
        return Err(anyhow!(
            "table cell ({row}, {column}) is out-of-range for a {rows}x{columns} PPTX table",
            rows = simple.rows,
            columns = simple.columns
        ));
    }
    let cell = simple
        .cells
        .iter()
        .find(|cell| cell.row == row && cell.column == column)
        .expect("simple PPTX table contains every rectangular cell");
    if cell.decoded != expected_text {
        return Err(anyhow!(
            "selected PPTX table cell text does not match expected_text"
        ));
    }
    let opening = &slide_xml[cell.text_start..cell.text_open_end];
    let opening = pptx_text_opening_for_value(opening, replacement)?;
    let mut updated_xml = slide_xml.clone();
    updated_xml.replace_range(
        cell.text_start..cell.text_close_end,
        format!("{opening}{}</a:t>", escape_xml(replacement)).as_str(),
    );
    if updated_xml.len() > super::MAX_XML_BYTES {
        return Err(anyhow!(
            "updated PPTX slide XML exceeds the local XML size limit"
        ));
    }
    drop(archive);
    let replacements = BTreeMap::from([(slide_path.clone(), updated_xml.into_bytes())]);
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "operation": "replace_table_cell_text",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "table_number": table_number,
        "row": row,
        "column": column,
        "previous_characters": expected_text.chars().count(),
        "replacement_characters": replacement.chars().count(),
        "bytes": bytes,
    }))
}

pub(super) fn delete_pptx_table_row(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let slide_number = required_pptx_index(arguments, "slide_number", MAX_PPTX_SLIDES)?;
    let table_number = required_pptx_index(arguments, "table_number", MAX_PPTX_TABLES_PER_SLIDE)?;
    let row = required_pptx_index(arguments, "row", MAX_PPTX_TABLE_ROWS)?;
    let (slide_path, slide_xml, table) =
        selected_pptx_table(source.as_path(), slide_number, table_number)?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "selected PPTX table is not eligible for simple row deletion: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if simple.rows == 1 {
        return Err(anyhow!("cannot delete the only row from a PPTX table"));
    }
    if row > simple.rows {
        return Err(anyhow!(
            "table row {row} is out-of-range for a PPTX table with {} rows",
            simple.rows
        ));
    }
    let expected_cells =
        required_pptx_table_row_cells(arguments, "expected_cells", simple.columns)?;
    ensure_expected_pptx_table_row(simple, row, expected_cells.as_slice())?;
    let rows = simple_pptx_table_rows(slide_xml.as_str(), simple).map_err(|error| {
        anyhow!("selected PPTX table is not eligible for simple row deletion: {error}")
    })?;
    let deleted = rows[row - 1];
    let recipient_index = if row < rows.len() { row } else { row - 2 };
    let recipient = rows[recipient_index];
    let recipient_height = recipient
        .height
        .checked_add(deleted.height)
        .filter(|height| *height <= SLIDE_HEIGHT)
        .ok_or_else(|| anyhow!("PPTX table row height overflow during deletion"))?;
    let updated_xml = apply_pptx_xml_edits(
        slide_xml.as_str(),
        vec![
            (
                recipient.range.start,
                recipient.range.open_end,
                canonical_pptx_table_row_opening(recipient_height),
            ),
            (deleted.range.start, deleted.range.end, String::new()),
        ],
    )?;
    validate_updated_pptx_table_rows(
        updated_xml.as_str(),
        table_number,
        simple.rows - 1,
        simple.columns,
    )?;
    let output_recipient_row = if recipient_index + 1 > row {
        recipient_index
    } else {
        recipient_index + 1
    };
    let replacements = BTreeMap::from([(slide_path.clone(), updated_xml.into_bytes())]);
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "operation": "delete_table_row",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "table_number": table_number,
        "deleted_row": row,
        "previous_rows": simple.rows,
        "rows": simple.rows - 1,
        "columns": simple.columns,
        "height_transferred_to_row": output_recipient_row,
        "table_frame_height_unchanged": true,
        "bytes": bytes,
    }))
}

pub(super) fn insert_pptx_table_row(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let slide_number = required_pptx_index(arguments, "slide_number", MAX_PPTX_SLIDES)?;
    let table_number = required_pptx_index(arguments, "table_number", MAX_PPTX_TABLES_PER_SLIDE)?;
    let reference_row = required_pptx_index(arguments, "reference_row", MAX_PPTX_TABLE_ROWS)?;
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("position must be before or after"))?;
    if !matches!(position, "before" | "after") {
        return Err(anyhow!("position must be before or after"));
    }
    let (slide_path, slide_xml, table) =
        selected_pptx_table(source.as_path(), slide_number, table_number)?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "selected PPTX table is not eligible for simple row insertion: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if simple.rows >= MAX_PPTX_TABLE_ROWS
        || simple.cells.len().saturating_add(simple.columns) > MAX_PPTX_TABLE_CELLS
    {
        return Err(anyhow!(
            "PPTX table cannot accept another row within the local safety limits"
        ));
    }
    if reference_row > simple.rows {
        return Err(anyhow!(
            "reference_row {reference_row} is out-of-range for a PPTX table with {} rows",
            simple.rows
        ));
    }
    let expected_cells =
        required_pptx_table_row_cells(arguments, "expected_cells", simple.columns)?;
    ensure_expected_pptx_table_row(simple, reference_row, expected_cells.as_slice())?;
    let cells = required_pptx_table_row_cells(arguments, "cells", simple.columns)?;
    let existing_text_chars = simple
        .cells
        .iter()
        .map(|cell| cell.decoded.chars().count())
        .sum::<usize>();
    let added_text_chars = cells.iter().map(|cell| cell.chars().count()).sum::<usize>();
    if existing_text_chars.saturating_add(added_text_chars) > MAX_PPTX_TABLE_TOTAL_TEXT_CHARS {
        return Err(anyhow!(
            "inserted PPTX table row would exceed the {MAX_PPTX_TABLE_TOTAL_TEXT_CHARS} character safety limit"
        ));
    }
    let rows = simple_pptx_table_rows(slide_xml.as_str(), simple).map_err(|error| {
        anyhow!("selected PPTX table is not eligible for simple row insertion: {error}")
    })?;
    let reference = rows[reference_row - 1];
    if reference.height < 2 {
        return Err(anyhow!(
            "reference PPTX table row is too short to split safely"
        ));
    }
    let inserted_height = reference.height / 2;
    let retained_height = reference.height - inserted_height;
    let reference_cells = simple
        .cells
        .iter()
        .filter(|cell| cell.row == reference_row)
        .cloned()
        .collect::<Vec<_>>();
    let retained_row_xml =
        pptx_table_row_with_height(slide_xml.as_str(), reference, retained_height)?;
    let inserted_row_xml = clone_pptx_table_row_with_text(
        slide_xml.as_str(),
        reference,
        reference_cells.as_slice(),
        cells.as_slice(),
        inserted_height,
    )?;
    let replacement = if position == "before" {
        format!("{inserted_row_xml}{retained_row_xml}")
    } else {
        format!("{retained_row_xml}{inserted_row_xml}")
    };
    let updated_xml = apply_pptx_xml_edits(
        slide_xml.as_str(),
        vec![(reference.range.start, reference.range.end, replacement)],
    )?;
    validate_updated_pptx_table_rows(
        updated_xml.as_str(),
        table_number,
        simple.rows + 1,
        simple.columns,
    )?;
    let inserted_row = if position == "before" {
        reference_row
    } else {
        reference_row + 1
    };
    let replacements = BTreeMap::from([(slide_path.clone(), updated_xml.into_bytes())]);
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "operation": "insert_table_row",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "table_number": table_number,
        "reference_row": reference_row,
        "position": position,
        "inserted_row": inserted_row,
        "previous_rows": simple.rows,
        "rows": simple.rows + 1,
        "columns": simple.columns,
        "format_cloned_from_reference_row": true,
        "table_frame_height_unchanged": true,
        "bytes": bytes,
    }))
}

pub(super) fn move_pptx_table_row(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let slide_number = required_pptx_index(arguments, "slide_number", MAX_PPTX_SLIDES)?;
    let table_number = required_pptx_index(arguments, "table_number", MAX_PPTX_TABLES_PER_SLIDE)?;
    let row = required_pptx_index(arguments, "row", MAX_PPTX_TABLE_ROWS)?;
    let reference_row = required_pptx_index(arguments, "reference_row", MAX_PPTX_TABLE_ROWS)?;
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("position must be before or after"))?;
    if !matches!(position, "before" | "after") {
        return Err(anyhow!("position must be before or after"));
    }
    let (slide_path, slide_xml, table) =
        selected_pptx_table(source.as_path(), slide_number, table_number)?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "selected PPTX table is not eligible for simple row movement: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if row > simple.rows {
        return Err(anyhow!(
            "table row {row} is out-of-range for a PPTX table with {} rows",
            simple.rows
        ));
    }
    if reference_row > simple.rows {
        return Err(anyhow!(
            "reference_row {reference_row} is out-of-range for a PPTX table with {} rows",
            simple.rows
        ));
    }
    ensure_changed_pptx_table_move(row, reference_row, position, "row")?;
    let expected_cells =
        required_pptx_table_row_cells(arguments, "expected_cells", simple.columns)?;
    ensure_expected_pptx_table_row(simple, row, expected_cells.as_slice())?;
    let reference_expected_cells =
        required_pptx_table_row_cells(arguments, "reference_expected_cells", simple.columns)?;
    ensure_expected_pptx_table_row(simple, reference_row, reference_expected_cells.as_slice())
        .map_err(|_| {
            anyhow!("selected PPTX reference row does not match reference_expected_cells")
        })?;
    let rows = simple_pptx_table_rows(slide_xml.as_str(), simple).map_err(|error| {
        anyhow!("selected PPTX table is not eligible for simple row movement: {error}")
    })?;
    let source_row = rows[row - 1];
    let reference = rows[reference_row - 1];
    let edit = move_pptx_xml_element_edit(
        slide_xml.as_str(),
        source_row.range,
        reference.range,
        position,
    )?;
    let updated_xml = apply_pptx_xml_edits(slide_xml.as_str(), vec![edit])?;
    validate_updated_pptx_table_rows(
        updated_xml.as_str(),
        table_number,
        simple.rows,
        simple.columns,
    )?;
    let moved_row = moved_pptx_table_index(row, reference_row, position)?;
    let replacements = BTreeMap::from([(slide_path.clone(), updated_xml.into_bytes())]);
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "operation": "move_table_row",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "table_number": table_number,
        "row": row,
        "reference_row": reference_row,
        "position": position,
        "moved_row": moved_row,
        "rows": simple.rows,
        "columns": simple.columns,
        "row_xml_and_formatting_preserved": true,
        "table_frame_height_unchanged": true,
        "bytes": bytes,
    }))
}

pub(super) fn delete_pptx_table_column(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let slide_number = required_pptx_index(arguments, "slide_number", MAX_PPTX_SLIDES)?;
    let table_number = required_pptx_index(arguments, "table_number", MAX_PPTX_TABLES_PER_SLIDE)?;
    let column = required_pptx_index(arguments, "column", MAX_PPTX_TABLE_COLUMNS)?;
    let (slide_path, slide_xml, table) =
        selected_pptx_table(source.as_path(), slide_number, table_number)?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "selected PPTX table is not eligible for simple column deletion: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if simple.columns == 1 {
        return Err(anyhow!("cannot delete the only column from a PPTX table"));
    }
    if column > simple.columns {
        return Err(anyhow!(
            "table column {column} is out-of-range for a PPTX table with {} columns",
            simple.columns
        ));
    }
    let expected_cells =
        required_pptx_table_column_cells(arguments, "expected_cells", simple.rows)?;
    ensure_expected_pptx_table_column(simple, column, expected_cells.as_slice())?;
    let columns = simple_pptx_table_columns(slide_xml.as_str(), simple).map_err(|error| {
        anyhow!("selected PPTX table is not eligible for simple column deletion: {error}")
    })?;
    let deleted = columns[column - 1];
    let recipient_index = if column < columns.len() {
        column
    } else {
        column - 2
    };
    let recipient = columns[recipient_index];
    let recipient_width = recipient
        .width
        .checked_add(deleted.width)
        .filter(|width| *width <= SLIDE_WIDTH)
        .ok_or_else(|| anyhow!("PPTX table column width overflow during deletion"))?;
    let mut edits = vec![
        (
            recipient.range.start,
            recipient.range.open_end,
            canonical_pptx_table_column_opening(recipient_width),
        ),
        (deleted.range.start, deleted.range.end, String::new()),
    ];
    edits.extend(
        simple
            .cells
            .iter()
            .filter(|cell| cell.column == column)
            .map(|cell| (cell.range.start, cell.range.end, String::new())),
    );
    let updated_xml = apply_pptx_xml_edits(slide_xml.as_str(), edits)?;
    validate_updated_pptx_table_columns(
        updated_xml.as_str(),
        table_number,
        simple.rows,
        simple.columns - 1,
    )?;
    let output_recipient_column = if recipient_index + 1 > column {
        recipient_index
    } else {
        recipient_index + 1
    };
    let replacements = BTreeMap::from([(slide_path.clone(), updated_xml.into_bytes())]);
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "operation": "delete_table_column",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "table_number": table_number,
        "deleted_column": column,
        "previous_columns": simple.columns,
        "rows": simple.rows,
        "columns": simple.columns - 1,
        "width_transferred_to_column": output_recipient_column,
        "table_frame_width_unchanged": true,
        "bytes": bytes,
    }))
}

pub(super) fn insert_pptx_table_column(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let slide_number = required_pptx_index(arguments, "slide_number", MAX_PPTX_SLIDES)?;
    let table_number = required_pptx_index(arguments, "table_number", MAX_PPTX_TABLES_PER_SLIDE)?;
    let reference_column =
        required_pptx_index(arguments, "reference_column", MAX_PPTX_TABLE_COLUMNS)?;
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("position must be before or after"))?;
    if !matches!(position, "before" | "after") {
        return Err(anyhow!("position must be before or after"));
    }
    let (slide_path, slide_xml, table) =
        selected_pptx_table(source.as_path(), slide_number, table_number)?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "selected PPTX table is not eligible for simple column insertion: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if simple.columns >= MAX_PPTX_TABLE_COLUMNS
        || simple.cells.len().saturating_add(simple.rows) > MAX_PPTX_TABLE_CELLS
    {
        return Err(anyhow!(
            "PPTX table cannot accept another column within the local safety limits"
        ));
    }
    if reference_column > simple.columns {
        return Err(anyhow!(
            "reference_column {reference_column} is out-of-range for a PPTX table with {} columns",
            simple.columns
        ));
    }
    let expected_cells =
        required_pptx_table_column_cells(arguments, "expected_cells", simple.rows)?;
    ensure_expected_pptx_table_column(simple, reference_column, expected_cells.as_slice())?;
    let cells = required_pptx_table_column_cells(arguments, "cells", simple.rows)?;
    let existing_text_chars = simple
        .cells
        .iter()
        .map(|cell| cell.decoded.chars().count())
        .sum::<usize>();
    let added_text_chars = cells.iter().map(|cell| cell.chars().count()).sum::<usize>();
    if existing_text_chars.saturating_add(added_text_chars) > MAX_PPTX_TABLE_TOTAL_TEXT_CHARS {
        return Err(anyhow!(
            "inserted PPTX table column would exceed the {MAX_PPTX_TABLE_TOTAL_TEXT_CHARS} character safety limit"
        ));
    }
    let columns = simple_pptx_table_columns(slide_xml.as_str(), simple).map_err(|error| {
        anyhow!("selected PPTX table is not eligible for simple column insertion: {error}")
    })?;
    let reference = columns[reference_column - 1];
    if reference.width < 2 {
        return Err(anyhow!(
            "reference PPTX table column is too narrow to split safely"
        ));
    }
    let inserted_width = reference.width / 2;
    let retained_width = reference.width - inserted_width;
    let retained_column_xml = canonical_pptx_table_column_opening(retained_width);
    let inserted_column_xml = canonical_pptx_table_column_opening(inserted_width);
    let column_replacement = if position == "before" {
        format!("{inserted_column_xml}{retained_column_xml}")
    } else {
        format!("{retained_column_xml}{inserted_column_xml}")
    };
    let reference_cells = simple
        .cells
        .iter()
        .filter(|cell| cell.column == reference_column)
        .collect::<Vec<_>>();
    if reference_cells.len() != simple.rows {
        return Err(anyhow!(
            "reference PPTX table column does not contain one cell per row"
        ));
    }
    let mut edits = vec![(
        reference.range.start,
        reference.range.end,
        column_replacement,
    )];
    for (cell, value) in reference_cells.into_iter().zip(cells.iter()) {
        let retained_cell_xml = &slide_xml[cell.range.start..cell.range.end];
        let inserted_cell_xml =
            clone_pptx_table_cell_with_text(slide_xml.as_str(), cell, value.as_str())?;
        let replacement = if position == "before" {
            format!("{inserted_cell_xml}{retained_cell_xml}")
        } else {
            format!("{retained_cell_xml}{inserted_cell_xml}")
        };
        edits.push((cell.range.start, cell.range.end, replacement));
    }
    let updated_xml = apply_pptx_xml_edits(slide_xml.as_str(), edits)?;
    validate_updated_pptx_table_columns(
        updated_xml.as_str(),
        table_number,
        simple.rows,
        simple.columns + 1,
    )?;
    let inserted_column = if position == "before" {
        reference_column
    } else {
        reference_column + 1
    };
    let replacements = BTreeMap::from([(slide_path.clone(), updated_xml.into_bytes())]);
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "operation": "insert_table_column",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "table_number": table_number,
        "reference_column": reference_column,
        "position": position,
        "inserted_column": inserted_column,
        "previous_columns": simple.columns,
        "rows": simple.rows,
        "columns": simple.columns + 1,
        "format_cloned_from_reference_column": true,
        "table_frame_width_unchanged": true,
        "bytes": bytes,
    }))
}

pub(super) fn move_pptx_table_column(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let slide_number = required_pptx_index(arguments, "slide_number", MAX_PPTX_SLIDES)?;
    let table_number = required_pptx_index(arguments, "table_number", MAX_PPTX_TABLES_PER_SLIDE)?;
    let column = required_pptx_index(arguments, "column", MAX_PPTX_TABLE_COLUMNS)?;
    let reference_column =
        required_pptx_index(arguments, "reference_column", MAX_PPTX_TABLE_COLUMNS)?;
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("position must be before or after"))?;
    if !matches!(position, "before" | "after") {
        return Err(anyhow!("position must be before or after"));
    }
    let (slide_path, slide_xml, table) =
        selected_pptx_table(source.as_path(), slide_number, table_number)?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "selected PPTX table is not eligible for simple column movement: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if column > simple.columns {
        return Err(anyhow!(
            "table column {column} is out-of-range for a PPTX table with {} columns",
            simple.columns
        ));
    }
    if reference_column > simple.columns {
        return Err(anyhow!(
            "reference_column {reference_column} is out-of-range for a PPTX table with {} columns",
            simple.columns
        ));
    }
    ensure_changed_pptx_table_move(column, reference_column, position, "column")?;
    let expected_cells =
        required_pptx_table_column_cells(arguments, "expected_cells", simple.rows)?;
    ensure_expected_pptx_table_column(simple, column, expected_cells.as_slice())?;
    let reference_expected_cells =
        required_pptx_table_column_cells(arguments, "reference_expected_cells", simple.rows)?;
    ensure_expected_pptx_table_column(
        simple,
        reference_column,
        reference_expected_cells.as_slice(),
    )
    .map_err(|_| {
        anyhow!("selected PPTX reference column does not match reference_expected_cells")
    })?;
    let columns = simple_pptx_table_columns(slide_xml.as_str(), simple).map_err(|error| {
        anyhow!("selected PPTX table is not eligible for simple column movement: {error}")
    })?;
    let mut edits = vec![move_pptx_xml_element_edit(
        slide_xml.as_str(),
        columns[column - 1].range,
        columns[reference_column - 1].range,
        position,
    )?];
    for row in 1..=simple.rows {
        let source_cell = simple
            .cells
            .iter()
            .find(|cell| cell.row == row && cell.column == column)
            .expect("simple PPTX table contains every source column cell");
        let reference_cell = simple
            .cells
            .iter()
            .find(|cell| cell.row == row && cell.column == reference_column)
            .expect("simple PPTX table contains every reference column cell");
        edits.push(move_pptx_xml_element_edit(
            slide_xml.as_str(),
            source_cell.range,
            reference_cell.range,
            position,
        )?);
    }
    let updated_xml = apply_pptx_xml_edits(slide_xml.as_str(), edits)?;
    validate_updated_pptx_table_columns(
        updated_xml.as_str(),
        table_number,
        simple.rows,
        simple.columns,
    )?;
    let moved_column = moved_pptx_table_index(column, reference_column, position)?;
    let replacements = BTreeMap::from([(slide_path.clone(), updated_xml.into_bytes())]);
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "operation": "move_table_column",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "table_number": table_number,
        "column": column,
        "reference_column": reference_column,
        "position": position,
        "moved_column": moved_column,
        "rows": simple.rows,
        "columns": simple.columns,
        "grid_column_and_cell_xml_preserved": true,
        "table_frame_width_unchanged": true,
        "bytes": bytes,
    }))
}

pub(super) fn replace_pptx_notes_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let find = required_text(arguments, "find")?;
    let replacement = arguments
        .get("replacement")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("replacement must be a string"))?;
    if find.is_empty() || find.chars().count() > 10_000 {
        return Err(anyhow!("find must contain between 1 and 10000 characters"));
    }
    validate_slide_text(find, "find", 10_000)?;
    validate_slide_text(replacement, "replacement", MAX_SLIDE_TEXT_CHARS)?;
    if find == replacement {
        return Err(anyhow!(
            "PPTX speaker-note text replacement must change the matched text"
        ));
    }
    let max_replacements = match arguments.get("max_replacements") {
        None => 100usize,
        Some(Value::Number(value)) => value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| (1..=10_000).contains(value))
            .ok_or_else(|| anyhow!("max_replacements must be an integer between 1 and 10000"))?,
        Some(_) => {
            return Err(anyhow!(
                "max_replacements must be an integer between 1 and 10000"
            ));
        }
    };

    let names = validate_pptx_package(source.as_path())?;
    for required in ["ppt/presentation.xml", "ppt/_rels/presentation.xml.rels"] {
        if !names.contains(required) {
            return Err(anyhow!("PPTX is missing required package part: {required}"));
        }
    }
    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open PPTX {}", source.display()))?;
    let presentation_xml = read_zip_text(&mut archive, "ppt/presentation.xml")?;
    let presentation_relationships_xml =
        read_zip_text(&mut archive, "ppt/_rels/presentation.xml.rels")?;
    let slide_metadata = presentation_slide_metadata(presentation_xml.as_str())?;
    let presentation_relationships = parse_relationship_document(
        presentation_relationships_xml.as_str(),
        "ppt/presentation.xml",
    )?;
    let ordered_slide_paths =
        ordered_presentation_slide_paths(&slide_metadata, &presentation_relationships, &names)?;
    if ordered_slide_paths.len() > MAX_PPTX_SLIDES {
        return Err(anyhow!(
            "PPTX slide count exceeds the {MAX_PPTX_SLIDES} speaker-note editing limit"
        ));
    }
    ensure_all_slide_parts_are_referenced(&ordered_slide_paths, &names)?;
    let selected_positions = selected_slide_positions(arguments, ordered_slide_paths.len())?;
    let notes_by_slide =
        owned_notes_parts_by_slide(&mut archive, &names, ordered_slide_paths.as_slice())?;
    let mut replacements = BTreeMap::new();
    let mut replacement_count = 0usize;
    let mut matched_slides = Vec::new();
    let mut replacement_limit_reached = false;
    for position in selected_positions {
        let Some(notes) = &notes_by_slide[position - 1] else {
            continue;
        };
        let notes_xml = read_zip_text(&mut archive, notes.path.as_str())?;
        let remaining = max_replacements.saturating_sub(replacement_count);
        let (updated, count, limit_reached) =
            replace_drawing_text_runs(notes_xml.as_str(), find, replacement, remaining)?;
        if count > 0 {
            replacement_count = replacement_count.saturating_add(count);
            matched_slides.push(position);
            replacements.insert(notes.path.clone(), updated.into_bytes());
        }
        replacement_limit_reached |= limit_reached;
    }
    drop(archive);
    if replacement_count == 0 {
        return Err(anyhow!(
            "PPTX speaker-note text was not found inside a single DrawingML text run"
        ));
    }
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "matched_slides": matched_slides,
        "replacements": replacement_count,
        "replacement_limit_reached": replacement_limit_reached,
        "bytes": bytes,
    }))
}

pub(super) fn reorder_pptx_slides(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;

    let names = validate_pptx_package(source.as_path())?;
    for required in ["ppt/presentation.xml", "ppt/_rels/presentation.xml.rels"] {
        if !names.contains(required) {
            return Err(anyhow!("PPTX is missing required package part: {required}"));
        }
    }
    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open PPTX {}", source.display()))?;
    let presentation_xml = read_zip_text(&mut archive, "ppt/presentation.xml")?;
    let presentation_relationships_xml =
        read_zip_text(&mut archive, "ppt/_rels/presentation.xml.rels")?;
    let slide_metadata = presentation_slide_metadata(presentation_xml.as_str())?;
    let presentation_relationships = parse_relationship_document(
        presentation_relationships_xml.as_str(),
        "ppt/presentation.xml",
    )?;
    let ordered_slide_paths =
        ordered_presentation_slide_paths(&slide_metadata, &presentation_relationships, &names)?;
    if ordered_slide_paths.len() > MAX_PPTX_SLIDES {
        return Err(anyhow!(
            "PPTX slide count exceeds the {MAX_PPTX_SLIDES} slide reordering limit"
        ));
    }
    ensure_all_slide_parts_are_referenced(&ordered_slide_paths, &names)?;
    let slide_order = required_slide_order(arguments, ordered_slide_paths.len())?;
    if slide_order
        .iter()
        .copied()
        .eq(1..=ordered_slide_paths.len())
    {
        return Err(anyhow!("PPTX slide_order must change the current order"));
    }
    let reordered_presentation = rewrite_presentation_slide_ids(
        presentation_xml.as_str(),
        &slide_metadata,
        slide_order.as_slice(),
    )?;
    let reordered_slide_files = slide_order
        .iter()
        .map(|position| {
            ordered_slide_paths
                .get(position - 1)
                .expect("slide order position validated")
                .clone()
        })
        .collect::<Vec<_>>();
    drop(archive);

    let replacements = BTreeMap::from([(
        "ppt/presentation.xml".to_string(),
        reordered_presentation.into_bytes(),
    )]);
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slides": ordered_slide_paths.len(),
        "slide_order": slide_order,
        "slide_files": reordered_slide_files,
        "bytes": bytes,
    }))
}

pub(super) fn delete_pptx_slides(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;

    let names = validate_pptx_package(source.as_path())?;
    for required in [
        "[Content_Types].xml",
        "ppt/presentation.xml",
        "ppt/_rels/presentation.xml.rels",
    ] {
        if !names.contains(required) {
            return Err(anyhow!("PPTX is missing required package part: {required}"));
        }
    }
    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open PPTX {}", source.display()))?;
    let content_types_xml = read_zip_text(&mut archive, "[Content_Types].xml")?;
    let presentation_xml = read_zip_text(&mut archive, "ppt/presentation.xml")?;
    let presentation_relationships_xml =
        read_zip_text(&mut archive, "ppt/_rels/presentation.xml.rels")?;
    reject_unsupported_slide_deletion_references(presentation_xml.as_str())?;
    let slide_metadata = presentation_slide_metadata(presentation_xml.as_str())?;
    let presentation_relationships = parse_relationship_document(
        presentation_relationships_xml.as_str(),
        "ppt/presentation.xml",
    )?;
    let ordered_slide_paths =
        ordered_presentation_slide_paths(&slide_metadata, &presentation_relationships, &names)?;
    if ordered_slide_paths.len() > MAX_PPTX_SLIDES {
        return Err(anyhow!(
            "PPTX slide count exceeds the {MAX_PPTX_SLIDES} slide deletion limit"
        ));
    }
    ensure_all_slide_parts_are_referenced(&ordered_slide_paths, &names)?;
    ensure_presentation_slide_relationships_are_exact(
        &slide_metadata,
        &presentation_relationships,
    )?;
    let deleted_positions = required_deleted_slide_positions(arguments, ordered_slide_paths.len())?;
    let deleted_position_set = deleted_positions.iter().copied().collect::<HashSet<_>>();
    let retained_positions = (1..=ordered_slide_paths.len())
        .filter(|position| !deleted_position_set.contains(position))
        .collect::<Vec<_>>();

    let notes_by_slide =
        owned_notes_parts_by_slide(&mut archive, &names, ordered_slide_paths.as_slice())?;

    let content_types = content_types_metadata(content_types_xml.as_str())?;
    let mut removals = HashSet::<String>::new();
    let mut removed_content_type_parts = HashSet::<String>::new();
    let mut removed_relationship_ids = HashSet::<String>::new();
    let mut deleted_slide_files = Vec::with_capacity(deleted_positions.len());
    let mut deleted_notes = 0usize;
    for position in &deleted_positions {
        let index = position - 1;
        let slide_path = ordered_slide_paths[index].clone();
        let content_type_part = format!("/{slide_path}");
        if !content_types.overrides.contains(content_type_part.as_str()) {
            return Err(anyhow!(
                "PPTX deleted slide is missing its content-type override: {content_type_part}"
            ));
        }
        removed_content_type_parts.insert(content_type_part);
        removals.insert(slide_path.clone());
        let slide_relationships_path = relationships_part_path(slide_path.as_str())?;
        if names.contains(slide_relationships_path.as_str()) {
            removals.insert(slide_relationships_path);
        }
        removed_relationship_ids.insert(slide_metadata.relationship_ids[index].clone());
        if let Some(notes) = &notes_by_slide[index] {
            let content_type_part = format!("/{}", notes.path);
            if !content_types.overrides.contains(content_type_part.as_str()) {
                return Err(anyhow!(
                    "PPTX deleted notes part is missing its content-type override: {content_type_part}"
                ));
            }
            removed_content_type_parts.insert(content_type_part);
            removals.insert(notes.path.clone());
            removals.insert(notes.relationships_path.clone());
            deleted_notes = deleted_notes.saturating_add(1);
        }
        deleted_slide_files.push(slide_path);
    }

    let updated_presentation = rewrite_presentation_slide_ids(
        presentation_xml.as_str(),
        &slide_metadata,
        retained_positions.as_slice(),
    )?;
    let updated_presentation_relationships = remove_relationship_entries(
        presentation_relationships_xml.as_str(),
        &removed_relationship_ids,
    )?;
    let updated_content_types =
        remove_content_type_overrides(content_types_xml.as_str(), &removed_content_type_parts)?;
    drop(archive);

    let replacements = BTreeMap::from([
        (
            "[Content_Types].xml".to_string(),
            updated_content_types.into_bytes(),
        ),
        (
            "ppt/presentation.xml".to_string(),
            updated_presentation.into_bytes(),
        ),
        (
            "ppt/_rels/presentation.xml.rels".to_string(),
            updated_presentation_relationships.into_bytes(),
        ),
    ]);
    let bytes = rewrite_pptx_package_with_removals(
        source.as_path(),
        target.as_path(),
        &replacements,
        &removals,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "previous_slides": ordered_slide_paths.len(),
        "deleted_slides": deleted_positions,
        "deleted_slide_files": deleted_slide_files,
        "deleted_speaker_notes": deleted_notes,
        "slides": retained_positions.len(),
        "bytes": bytes,
    }))
}

pub(super) fn inspect_pptx(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (path, relative) = input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let names = validate_pptx_package(path.as_path())?;
    for required in ["ppt/presentation.xml", "ppt/_rels/presentation.xml.rels"] {
        if !names.contains(required) {
            return Err(anyhow!("PPTX is missing required package part: {required}"));
        }
    }
    let mut archive = ZipArchive::new(File::open(path.as_path())?)
        .with_context(|| format!("open PPTX {}", path.display()))?;
    let presentation_xml = read_zip_text(&mut archive, "ppt/presentation.xml")?;
    let (slide_width, slide_height) = presentation_slide_size(presentation_xml.as_str())?;
    let presentation_relationships_xml =
        read_zip_text(&mut archive, "ppt/_rels/presentation.xml.rels")?;
    let slide_metadata = presentation_slide_metadata(presentation_xml.as_str())?;
    let presentation_relationships = parse_relationship_document(
        presentation_relationships_xml.as_str(),
        "ppt/presentation.xml",
    )?;
    let ordered_slide_paths =
        ordered_presentation_slide_paths(&slide_metadata, &presentation_relationships, &names)?;
    ensure_all_slide_parts_are_referenced(&ordered_slide_paths, &names)?;
    if ordered_slide_paths.is_empty() || ordered_slide_paths.len() > 1_000 {
        return Err(anyhow!(
            "PPTX slide count is outside the inspection safety limit"
        ));
    }
    let mut metadata = Vec::with_capacity(ordered_slide_paths.len());
    let mut image_count = 0usize;
    let mut notes_count = 0usize;
    let mut table_count = 0usize;
    for (index, slide_path) in ordered_slide_paths.iter().enumerate() {
        let number = index + 1;
        let slide_xml = read_zip_text(&mut archive, slide_path.as_str())?;
        let text_runs = drawing_text_runs(slide_xml.as_str(), 8_001)?;
        let title = text_runs.first().cloned().unwrap_or_default();
        let text = text_runs.join("\n");
        let tables = scan_pptx_tables(slide_xml.as_str())?;
        table_count = table_count.saturating_add(tables.len());
        let table_metadata = tables
            .iter()
            .enumerate()
            .map(|(table_index, table)| {
                json!({
                    "number": table_index + 1,
                    "rows": table.rows,
                    "columns": table.columns,
                    "cells": table.cells,
                    "cell_text_truncated": table.cell_text_truncated,
                    "eligible_for_cell_replacement": table.simple.is_some(),
                    "unsupported_reason": table.unsupported_reason.as_deref(),
                })
            })
            .collect::<Vec<_>>();
        let relationships_path = relationships_part_path(slide_path.as_str())?;
        let relationships = if names.contains(relationships_path.as_str()) {
            read_zip_text(&mut archive, relationships_path.as_str())?
        } else {
            String::new()
        };
        let relationship_metadata =
            inspect_slide_relationships(relationships.as_str(), slide_path.as_str())?;
        image_count = image_count.saturating_add(relationship_metadata.image_count);
        let notes_path = relationship_metadata.notes_path;
        let (notes_present, notes_preview, notes_truncated) = if let Some(notes_path) = notes_path {
            if !names.contains(notes_path.as_str()) {
                return Err(anyhow!(
                    "PPTX is missing referenced notes part: {notes_path}"
                ));
            }
            let notes_xml = read_zip_text(&mut archive, notes_path.as_str())?;
            let notes = drawing_text_runs(notes_xml.as_str(), 4_001)?.join("\n");
            notes_count = notes_count.saturating_add(1);
            (
                true,
                notes.chars().take(4_000).collect::<String>(),
                notes.chars().count() > 4_000,
            )
        } else {
            (false, String::new(), false)
        };
        metadata.push(json!({
            "number": number,
            "slide_id": slide_metadata.slide_ids[index],
            "file": slide_path,
            "title": title.chars().take(1_000).collect::<String>(),
            "text_preview": text.chars().take(8_000).collect::<String>(),
            "text_truncated": text.chars().count() > 8_000,
            "images": relationship_metadata.image_count,
            "tables": tables.len(),
            "table_metadata": table_metadata,
            "notes_present": notes_present,
            "notes_preview": notes_preview,
            "notes_truncated": notes_truncated,
        }));
    }
    let media_files = names
        .iter()
        .filter(|name| name.starts_with("ppt/media/") && !name.ends_with('/'))
        .count();
    Ok(json!({
        "path": relative,
        "bytes": file_size(path.as_path())?,
        "slides": ordered_slide_paths.len(),
        "slide_files": ordered_slide_paths,
        "slide_width_emu": slide_width,
        "slide_height_emu": slide_height,
        "widescreen": slide_width.saturating_mul(9).abs_diff(slide_height.saturating_mul(16)) < 20_000,
        "images": image_count,
        "tables": table_count,
        "media_files": media_files,
        "speaker_notes": notes_count,
        "slide_metadata": metadata,
    }))
}

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
            let value_axis = pptx_chart_value_axis_by_position(chart.axes.as_slice(), "l");
            let secondary_value_axis =
                pptx_chart_value_axis_by_position(chart.axes.as_slice(), "r");
            let mut metadata = json!({
                "slide_number": slide_number,
                "chart_number": chart_index + 1,
                "relationship_id": reference.relationship_id,
                "part": reference.part,
                "chart_xml_sha256": hex::encode(Sha256::digest(chart_xml.as_bytes())),
                "chart_types": chart.chart_types,
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
                .expect("PPTX chart metadata object");
            metadata_object.insert(
                "value_axis_minimum".to_string(),
                json!(value_axis.and_then(|axis| axis.minimum.clone())),
            );
            metadata_object.insert(
                "value_axis_maximum".to_string(),
                json!(value_axis.and_then(|axis| axis.maximum.clone())),
            );
            metadata_object.insert(
                "value_axis_log_base".to_string(),
                json!(value_axis.and_then(|axis| axis.log_base.clone())),
            );
            metadata_object.insert(
                "value_axis_major_tick_mark".to_string(),
                json!(pptx_chart_axis_tick_mark_name(value_axis, true)),
            );
            metadata_object.insert(
                "value_axis_major_tick_mark_value".to_string(),
                json!(value_axis.and_then(|axis| axis.major_tick_mark.clone())),
            );
            metadata_object.insert(
                "value_axis_minor_tick_mark".to_string(),
                json!(pptx_chart_axis_tick_mark_name(value_axis, false)),
            );
            metadata_object.insert(
                "value_axis_minor_tick_mark_value".to_string(),
                json!(value_axis.and_then(|axis| axis.minor_tick_mark.clone())),
            );
            metadata_object.insert(
                "value_axis_major_unit".to_string(),
                json!(value_axis.and_then(|axis| axis.major_unit.clone())),
            );
            metadata_object.insert(
                "value_axis_minor_unit".to_string(),
                json!(value_axis.and_then(|axis| axis.minor_unit.clone())),
            );
            metadata_object.insert(
                "value_axis_number_format".to_string(),
                json!(pptx_chart_axis_number_format_name(value_axis)),
            );
            metadata_object.insert(
                "value_axis_number_format_code".to_string(),
                json!(value_axis.and_then(|axis| axis.number_format_code.clone())),
            );
            metadata_object.insert(
                "value_axis_number_format_source_linked".to_string(),
                json!(value_axis.and_then(|axis| axis.number_format_source_linked)),
            );
            metadata_object.insert(
                "secondary_value_axis_minimum".to_string(),
                json!(secondary_value_axis.and_then(|axis| axis.minimum.clone())),
            );
            metadata_object.insert(
                "secondary_value_axis_maximum".to_string(),
                json!(secondary_value_axis.and_then(|axis| axis.maximum.clone())),
            );
            metadata_object.insert(
                "secondary_value_axis_log_base".to_string(),
                json!(secondary_value_axis.and_then(|axis| axis.log_base.clone())),
            );
            metadata_object.insert(
                "secondary_value_axis_major_tick_mark".to_string(),
                json!(pptx_chart_axis_tick_mark_name(secondary_value_axis, true)),
            );
            metadata_object.insert(
                "secondary_value_axis_major_tick_mark_value".to_string(),
                json!(secondary_value_axis.and_then(|axis| axis.major_tick_mark.clone())),
            );
            metadata_object.insert(
                "secondary_value_axis_minor_tick_mark".to_string(),
                json!(pptx_chart_axis_tick_mark_name(secondary_value_axis, false)),
            );
            metadata_object.insert(
                "secondary_value_axis_minor_tick_mark_value".to_string(),
                json!(secondary_value_axis.and_then(|axis| axis.minor_tick_mark.clone())),
            );
            metadata_object.insert(
                "secondary_value_axis_major_unit".to_string(),
                json!(secondary_value_axis.and_then(|axis| axis.major_unit.clone())),
            );
            metadata_object.insert(
                "secondary_value_axis_minor_unit".to_string(),
                json!(secondary_value_axis.and_then(|axis| axis.minor_unit.clone())),
            );
            metadata_object.insert(
                "secondary_value_axis_number_format".to_string(),
                json!(pptx_chart_axis_number_format_name(secondary_value_axis)),
            );
            metadata_object.insert(
                "secondary_value_axis_number_format_code".to_string(),
                json!(secondary_value_axis.and_then(|axis| axis.number_format_code.clone())),
            );
            metadata_object.insert(
                "secondary_value_axis_number_format_source_linked".to_string(),
                json!(secondary_value_axis.and_then(|axis| axis.number_format_source_linked)),
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

pub(super) fn replace_pptx_chart(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let slide_number = required_pptx_index(arguments, "slide_number", MAX_PPTX_SLIDES)?;
    let chart_number = required_pptx_index(arguments, "chart_number", MAX_PPTX_CHARTS_PER_SLIDE)?;
    let expected_chart_xml_sha256 = required_pptx_chart_sha256(arguments)?;
    let expected_snapshot = arguments
        .get("expected_self_contained_edit_snapshot")
        .filter(|value| value.is_object())
        .ok_or_else(|| {
            anyhow!(
                "expected_self_contained_edit_snapshot must be the complete object returned by inspect_pptx_charts"
            )
        })?;
    let replacement = parse_presentation_chart(
        arguments
            .get("replacement")
            .ok_or_else(|| anyhow!("replacement chart is required"))?,
        slide_number,
    )?;

    let names = validate_pptx_package(source.as_path())?;
    for required in [
        "[Content_Types].xml",
        "ppt/presentation.xml",
        "ppt/_rels/presentation.xml.rels",
    ] {
        if !names.contains(required) {
            return Err(anyhow!("PPTX is missing required package part: {required}"));
        }
    }
    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open PPTX {}", source.display()))?;
    let content_types_xml = read_zip_text(&mut archive, "[Content_Types].xml")?;
    let ownership = inspect_pptx_chart_ownership(&mut archive, &names)?;
    let slide_path = ownership
        .ordered_slide_paths
        .get(slide_number - 1)
        .ok_or_else(|| {
            anyhow!(
                "slide_number {slide_number} is out-of-range for a PPTX with {} visible slides",
                ownership.ordered_slide_paths.len()
            )
        })?;
    let slide_charts = ownership
        .charts_by_slide
        .get(slide_number - 1)
        .expect("chart ownership has one entry per visible slide");
    let reference = slide_charts.get(chart_number - 1).ok_or_else(|| {
        anyhow!(
            "chart_number {chart_number} is out-of-range for visible slide {slide_number}, which contains {} charts",
            slide_charts.len()
        )
    })?;
    ensure_standard_pptx_chart_content_type(content_types_xml.as_str(), reference.part.as_str())?;
    let chart_xml = read_zip_text(&mut archive, reference.part.as_str())?;
    let actual_chart_xml_sha256 = hex::encode(Sha256::digest(chart_xml.as_bytes()));
    if actual_chart_xml_sha256 != expected_chart_xml_sha256 {
        return Err(anyhow!(
            "selected PPTX chart XML does not match expected_chart_xml_sha256"
        ));
    }
    let inspected = inspect_standard_pptx_chart_xml(chart_xml.as_str())?;
    let relationships =
        inspect_pptx_chart_relationships(&mut archive, &names, reference.part.as_str())?;
    let (_, actual_snapshot) = canonical_pptx_chart_snapshot(
        &inspected,
        &relationships,
        chart_xml.as_str(),
    )
    .map_err(|error| {
        anyhow!("selected PPTX chart is not eligible for self-contained chart replacement: {error}")
    })?;
    if &actual_snapshot != expected_snapshot {
        return Err(anyhow!(
            "selected PPTX chart does not match expected_self_contained_edit_snapshot"
        ));
    }
    let replacement_snapshot = presentation_chart_snapshot(&replacement);
    let replacement_xml = presentation_chart_xml(&replacement)?;
    if replacement_xml == chart_xml {
        return Err(anyhow!(
            "PPTX chart replacement must change the selected chart"
        ));
    }
    let replacement_chart_xml_sha256 = hex::encode(Sha256::digest(replacement_xml.as_bytes()));
    drop(archive);

    let replacements = BTreeMap::from([(reference.part.clone(), replacement_xml.into_bytes())]);
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "operation": "replace_chart",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "chart_number": chart_number,
        "relationship_id": reference.relationship_id,
        "part": reference.part,
        "previous_chart_xml_sha256": actual_chart_xml_sha256,
        "chart_xml_sha256": replacement_chart_xml_sha256,
        "self_contained_edit_snapshot": replacement_snapshot,
        "relationship_count": 0,
        "embedded_workbook": Value::Null,
        "bytes": bytes,
    }))
}

fn parse_slides(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Vec<SlideDefinition>> {
    let slides = arguments
        .get("slides")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("slides must be an array"))?;
    if slides.is_empty() || slides.len() > MAX_PPTX_SLIDES {
        return Err(anyhow!(
            "slides must contain between 1 and {MAX_PPTX_SLIDES} items"
        ));
    }
    let mut output = Vec::with_capacity(slides.len());
    let mut total_text_chars = 0usize;
    let mut total_image_bytes = 0usize;
    for (index, slide) in slides.iter().enumerate() {
        let object = slide
            .as_object()
            .ok_or_else(|| anyhow!("each slide must be an object"))?;
        let title = object
            .get("title")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("each slide requires a title"))?
            .to_string();
        let body = optional_slide_text(object.get("body"), "body")?;
        let left_body = optional_slide_text(object.get("left_body"), "left_body")?;
        let right_body = optional_slide_text(object.get("right_body"), "right_body")?;
        let notes = optional_slide_text(object.get("notes"), "notes")?;
        validate_slide_text(title.as_str(), "title", 1_000)?;
        let layout = SlideLayout::parse(object.get("layout").and_then(Value::as_str))?;
        if layout == SlideLayout::Table
            && ["body", "left_body", "right_body", "image", "chart"]
                .iter()
                .any(|field| object.contains_key(*field))
        {
            return Err(anyhow!(
                "table slides do not support body, left_body, right_body, image, or chart"
            ));
        }
        if layout == SlideLayout::Chart
            && ["body", "left_body", "right_body", "image", "table"]
                .iter()
                .any(|field| object.contains_key(*field))
        {
            return Err(anyhow!(
                "chart slides do not support body, left_body, right_body, image, or table"
            ));
        }
        let table = if layout == SlideLayout::Table {
            Some(parse_presentation_table(
                object
                    .get("table")
                    .ok_or_else(|| anyhow!("table slides require table"))?,
                index + 1,
            )?)
        } else {
            if object.contains_key("table") {
                return Err(anyhow!("slide table is only supported by the table layout"));
            }
            None
        };
        let chart = if layout == SlideLayout::Chart {
            Some(parse_presentation_chart(
                object
                    .get("chart")
                    .ok_or_else(|| anyhow!("chart slides require chart"))?,
                index + 1,
            )?)
        } else {
            if object.contains_key("chart") {
                return Err(anyhow!("slide chart is only supported by the chart layout"));
            }
            None
        };
        let image = object
            .get("image")
            .map(|image| parse_image(image, state, request, index + 1))
            .transpose()?;
        if matches!(layout, SlideLayout::ImageRight | SlideLayout::ImageFull) && image.is_none() {
            return Err(anyhow!("image_right and image_full slides require image"));
        }
        if !matches!(layout, SlideLayout::ImageRight | SlideLayout::ImageFull) && image.is_some() {
            return Err(anyhow!(
                "slide image is only supported by image_right or image_full layouts"
            ));
        }
        if layout == SlideLayout::TwoColumn && left_body.is_empty() && right_body.is_empty() {
            return Err(anyhow!(
                "two_column slides require left_body, right_body, or both"
            ));
        }
        total_text_chars = total_text_chars.saturating_add(
            title.chars().count()
                + body.chars().count()
                + left_body.chars().count()
                + right_body.chars().count()
                + notes.chars().count()
                + table
                    .as_ref()
                    .map(|table| {
                        table
                            .cells
                            .iter()
                            .flatten()
                            .map(|cell| cell.chars().count())
                            .sum::<usize>()
                    })
                    .unwrap_or(0)
                + chart
                    .as_ref()
                    .map(|chart| {
                        chart.title.chars().count()
                            + chart
                                .categories
                                .iter()
                                .map(|category| category.chars().count())
                                .sum::<usize>()
                            + chart
                                .series
                                .iter()
                                .map(|series| series.name.chars().count())
                                .sum::<usize>()
                    })
                    .unwrap_or(0),
        );
        if total_text_chars > MAX_PPTX_TEXT_CHARS {
            return Err(anyhow!(
                "presentation exceeds the {MAX_PPTX_TEXT_CHARS} character safety limit"
            ));
        }
        if let Some(image) = &image {
            total_image_bytes = total_image_bytes.saturating_add(image.bytes.len());
            if total_image_bytes > MAX_PPTX_TOTAL_IMAGE_BYTES {
                return Err(anyhow!(
                    "presentation images exceed the 50 MiB combined safety limit"
                ));
            }
        }
        output.push(SlideDefinition {
            title,
            body,
            left_body,
            right_body,
            notes,
            layout,
            image,
            table,
            chart,
        });
    }
    Ok(output)
}

fn parse_presentation_table(value: &Value, slide_number: usize) -> Result<PresentationTable> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("slide {slide_number} table must be an object"))?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "cells" | "header_row"))
    {
        return Err(anyhow!(
            "slide {slide_number} table contains unsupported properties"
        ));
    }
    let rows = object
        .get("cells")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("slide {slide_number} table cells must be an array"))?;
    if rows.is_empty() || rows.len() > MAX_PPTX_CREATE_TABLE_ROWS {
        return Err(anyhow!(
            "slide {slide_number} table must contain between 1 and {MAX_PPTX_CREATE_TABLE_ROWS} rows"
        ));
    }
    let header_row = object
        .get("header_row")
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| anyhow!("slide {slide_number} table header_row must be a boolean"))
        })
        .transpose()?
        .unwrap_or(true);
    let mut columns = None;
    let mut cells = Vec::with_capacity(rows.len());
    let mut cell_count = 0usize;
    let mut total_text_chars = 0usize;
    for (row_index, row) in rows.iter().enumerate() {
        let row = row.as_array().ok_or_else(|| {
            anyhow!(
                "slide {slide_number} table row {} must be an array",
                row_index + 1
            )
        })?;
        if row.is_empty() || row.len() > MAX_PPTX_CREATE_TABLE_COLUMNS {
            return Err(anyhow!(
                "slide {slide_number} table row {} must contain between 1 and {MAX_PPTX_CREATE_TABLE_COLUMNS} columns",
                row_index + 1
            ));
        }
        match columns {
            Some(expected) if row.len() != expected => {
                return Err(anyhow!(
                    "slide {slide_number} table cells must form a rectangular matrix"
                ));
            }
            None => columns = Some(row.len()),
            _ => {}
        }
        cell_count = cell_count.saturating_add(row.len());
        if cell_count > MAX_PPTX_CREATE_TABLE_CELLS {
            return Err(anyhow!(
                "slide {slide_number} table exceeds the {MAX_PPTX_CREATE_TABLE_CELLS} cell safety limit"
            ));
        }
        let mut output_row = Vec::with_capacity(row.len());
        for (column_index, cell) in row.iter().enumerate() {
            let cell = cell.as_str().ok_or_else(|| {
                anyhow!(
                    "slide {slide_number} table cell at row {}, column {} must be a string",
                    row_index + 1,
                    column_index + 1
                )
            })?;
            let label = format!(
                "slide {slide_number} table cell at row {}, column {}",
                row_index + 1,
                column_index + 1
            );
            validate_slide_text(cell, label.as_str(), MAX_PPTX_TABLE_CELL_TEXT_CHARS)?;
            total_text_chars = total_text_chars.saturating_add(cell.chars().count());
            if total_text_chars > MAX_PPTX_TABLE_TOTAL_TEXT_CHARS {
                return Err(anyhow!(
                    "slide {slide_number} table text exceeds the {MAX_PPTX_TABLE_TOTAL_TEXT_CHARS} character safety limit"
                ));
            }
            output_row.push(cell.to_string());
        }
        cells.push(output_row);
    }
    Ok(PresentationTable { cells, header_row })
}

fn parse_presentation_chart(value: &Value, slide_number: usize) -> Result<PresentationChart> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("slide {slide_number} chart must be an object"))?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "type"
                | "title"
                | "categories"
                | "series"
                | "show_legend"
                | "legend_position"
                | "data_labels"
                | "category_axis_title"
                | "value_axis_title"
                | "secondary_value_axis_title"
                | "value_axis_minimum"
                | "value_axis_maximum"
                | "value_axis_log_base"
                | "value_axis_major_tick_mark"
                | "value_axis_minor_tick_mark"
                | "value_axis_major_unit"
                | "value_axis_minor_unit"
                | "value_axis_number_format"
                | "secondary_value_axis_minimum"
                | "secondary_value_axis_maximum"
                | "secondary_value_axis_log_base"
                | "secondary_value_axis_major_tick_mark"
                | "secondary_value_axis_minor_tick_mark"
                | "secondary_value_axis_major_unit"
                | "secondary_value_axis_minor_unit"
                | "secondary_value_axis_number_format"
        )
    }) {
        return Err(anyhow!(
            "slide {slide_number} chart contains unsupported properties"
        ));
    }
    let chart_type = PresentationChartType::parse(
        object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("slide {slide_number} chart type is required"))?,
    )?;
    let title = object
        .get("title")
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| anyhow!("slide {slide_number} chart title must be a string"))
        })
        .transpose()?
        .unwrap_or_default()
        .to_string();
    validate_slide_text(
        title.as_str(),
        format!("slide {slide_number} chart title").as_str(),
        1_000,
    )?;
    if !title.is_empty() && title.trim().is_empty() {
        return Err(anyhow!(
            "slide {slide_number} chart title cannot contain only whitespace"
        ));
    }
    let category_axis_title = object
        .get("category_axis_title")
        .map(|value| {
            value.as_str().ok_or_else(|| {
                anyhow!("slide {slide_number} chart category_axis_title must be a string")
            })
        })
        .transpose()?
        .unwrap_or_default()
        .to_string();
    validate_slide_text(
        category_axis_title.as_str(),
        format!("slide {slide_number} chart category_axis_title").as_str(),
        1_000,
    )?;
    if !category_axis_title.is_empty() && category_axis_title.trim().is_empty() {
        return Err(anyhow!(
            "slide {slide_number} chart category_axis_title cannot contain only whitespace"
        ));
    }
    let value_axis_title = object
        .get("value_axis_title")
        .map(|value| {
            value.as_str().ok_or_else(|| {
                anyhow!("slide {slide_number} chart value_axis_title must be a string")
            })
        })
        .transpose()?
        .unwrap_or_default()
        .to_string();
    validate_slide_text(
        value_axis_title.as_str(),
        format!("slide {slide_number} chart value_axis_title").as_str(),
        1_000,
    )?;
    if !value_axis_title.is_empty() && value_axis_title.trim().is_empty() {
        return Err(anyhow!(
            "slide {slide_number} chart value_axis_title cannot contain only whitespace"
        ));
    }
    let secondary_value_axis_title = object
        .get("secondary_value_axis_title")
        .map(|value| {
            value.as_str().ok_or_else(|| {
                anyhow!("slide {slide_number} chart secondary_value_axis_title must be a string")
            })
        })
        .transpose()?
        .unwrap_or_default()
        .to_string();
    validate_slide_text(
        secondary_value_axis_title.as_str(),
        format!("slide {slide_number} chart secondary_value_axis_title").as_str(),
        1_000,
    )?;
    if !secondary_value_axis_title.is_empty() && secondary_value_axis_title.trim().is_empty() {
        return Err(anyhow!(
            "slide {slide_number} chart secondary_value_axis_title cannot contain only whitespace"
        ));
    }
    let value_axis_minimum = parse_presentation_chart_axis_bound(
        object.get("value_axis_minimum"),
        slide_number,
        "value_axis_minimum",
    )?;
    let value_axis_maximum = parse_presentation_chart_axis_bound(
        object.get("value_axis_maximum"),
        slide_number,
        "value_axis_maximum",
    )?;
    let value_axis_log_base = parse_presentation_chart_axis_log_base(
        object.get("value_axis_log_base"),
        slide_number,
        "value_axis_log_base",
    )?;
    let value_axis_major_tick_mark = parse_presentation_chart_axis_tick_mark(
        object.get("value_axis_major_tick_mark"),
        slide_number,
        "value_axis_major_tick_mark",
    )?;
    let value_axis_minor_tick_mark = parse_presentation_chart_axis_tick_mark(
        object.get("value_axis_minor_tick_mark"),
        slide_number,
        "value_axis_minor_tick_mark",
    )?;
    let value_axis_major_unit = parse_presentation_chart_axis_unit(
        object.get("value_axis_major_unit"),
        slide_number,
        "value_axis_major_unit",
    )?;
    let value_axis_minor_unit = parse_presentation_chart_axis_unit(
        object.get("value_axis_minor_unit"),
        slide_number,
        "value_axis_minor_unit",
    )?;
    let value_axis_number_format = parse_presentation_chart_axis_number_format(
        object.get("value_axis_number_format"),
        slide_number,
        "value_axis_number_format",
    )?;
    let secondary_value_axis_minimum = parse_presentation_chart_axis_bound(
        object.get("secondary_value_axis_minimum"),
        slide_number,
        "secondary_value_axis_minimum",
    )?;
    let secondary_value_axis_maximum = parse_presentation_chart_axis_bound(
        object.get("secondary_value_axis_maximum"),
        slide_number,
        "secondary_value_axis_maximum",
    )?;
    let secondary_value_axis_log_base = parse_presentation_chart_axis_log_base(
        object.get("secondary_value_axis_log_base"),
        slide_number,
        "secondary_value_axis_log_base",
    )?;
    let secondary_value_axis_major_tick_mark = parse_presentation_chart_axis_tick_mark(
        object.get("secondary_value_axis_major_tick_mark"),
        slide_number,
        "secondary_value_axis_major_tick_mark",
    )?;
    let secondary_value_axis_minor_tick_mark = parse_presentation_chart_axis_tick_mark(
        object.get("secondary_value_axis_minor_tick_mark"),
        slide_number,
        "secondary_value_axis_minor_tick_mark",
    )?;
    let secondary_value_axis_major_unit = parse_presentation_chart_axis_unit(
        object.get("secondary_value_axis_major_unit"),
        slide_number,
        "secondary_value_axis_major_unit",
    )?;
    let secondary_value_axis_minor_unit = parse_presentation_chart_axis_unit(
        object.get("secondary_value_axis_minor_unit"),
        slide_number,
        "secondary_value_axis_minor_unit",
    )?;
    let secondary_value_axis_number_format = parse_presentation_chart_axis_number_format(
        object.get("secondary_value_axis_number_format"),
        slide_number,
        "secondary_value_axis_number_format",
    )?;
    if chart_type.is_part_to_whole()
        && (!category_axis_title.is_empty()
            || !value_axis_title.is_empty()
            || !secondary_value_axis_title.is_empty()
            || value_axis_minimum.is_some()
            || value_axis_maximum.is_some()
            || value_axis_log_base.is_some()
            || value_axis_major_tick_mark != PresentationChartAxisTickMark::None
            || value_axis_minor_tick_mark != PresentationChartAxisTickMark::None
            || value_axis_major_unit.is_some()
            || value_axis_minor_unit.is_some()
            || value_axis_number_format != PresentationChartValueAxisNumberFormat::General
            || secondary_value_axis_minimum.is_some()
            || secondary_value_axis_maximum.is_some()
            || secondary_value_axis_log_base.is_some()
            || secondary_value_axis_major_tick_mark != PresentationChartAxisTickMark::None
            || secondary_value_axis_minor_tick_mark != PresentationChartAxisTickMark::None
            || secondary_value_axis_major_unit.is_some()
            || secondary_value_axis_minor_unit.is_some()
            || secondary_value_axis_number_format
                != PresentationChartValueAxisNumberFormat::General)
    {
        return Err(anyhow!(
            "slide {slide_number} {} chart does not support category/value axis titles, bounds, logarithmic scales, tick marks, units, or number formats",
            chart_type.as_str()
        ));
    }
    let categories = object
        .get("categories")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("slide {slide_number} chart categories must be an array"))?;
    if categories.is_empty() || categories.len() > MAX_PPTX_CREATE_CHART_CATEGORIES {
        return Err(anyhow!(
            "slide {slide_number} chart must contain between 1 and {MAX_PPTX_CREATE_CHART_CATEGORIES} categories"
        ));
    }
    let categories = categories
        .iter()
        .enumerate()
        .map(|(index, category)| {
            let category = category.as_str().ok_or_else(|| {
                anyhow!(
                    "slide {slide_number} chart category {} must be a string",
                    index + 1
                )
            })?;
            let label = format!("slide {slide_number} chart category {}", index + 1);
            validate_slide_text(category, label.as_str(), 1_000)?;
            if category.trim().is_empty() {
                return Err(anyhow!("{label} cannot be empty"));
            }
            Ok(category.to_string())
        })
        .collect::<Result<Vec<_>>>()?;
    let series = object
        .get("series")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("slide {slide_number} chart series must be an array"))?;
    if series.is_empty() || series.len() > MAX_PPTX_CREATE_CHART_SERIES {
        return Err(anyhow!(
            "slide {slide_number} chart must contain between 1 and {MAX_PPTX_CREATE_CHART_SERIES} series"
        ));
    }
    if chart_type.is_part_to_whole() && series.len() != 1 {
        return Err(anyhow!(
            "slide {slide_number} {} chart requires exactly one series",
            chart_type.as_str()
        ));
    }
    let mut series_names = HashSet::new();
    let mut parsed_series = Vec::with_capacity(series.len());
    for (series_index, series) in series.iter().enumerate() {
        let series = series.as_object().ok_or_else(|| {
            anyhow!(
                "slide {slide_number} chart series {} must be an object",
                series_index + 1
            )
        })?;
        if series.keys().any(|key| {
            !matches!(
                key.as_str(),
                "name"
                    | "values"
                    | "value_axis"
                    | "color"
                    | "marker_style"
                    | "marker_size"
                    | "smooth"
            )
        }) {
            return Err(anyhow!(
                "slide {slide_number} chart series {} contains unsupported properties",
                series_index + 1
            ));
        }
        let name = series.get("name").and_then(Value::as_str).ok_or_else(|| {
            anyhow!(
                "slide {slide_number} chart series {} name is required",
                series_index + 1
            )
        })?;
        let label = format!(
            "slide {slide_number} chart series {} name",
            series_index + 1
        );
        validate_slide_text(name, label.as_str(), 1_000)?;
        if name.trim().is_empty() {
            return Err(anyhow!("{label} cannot be empty"));
        }
        if !series_names.insert(name.to_string()) {
            return Err(anyhow!(
                "slide {slide_number} chart series names must be unique"
            ));
        }
        let values = series
            .get("values")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                anyhow!(
                    "slide {slide_number} chart series {} values must be an array",
                    series_index + 1
                )
            })?;
        if values.len() != categories.len() {
            return Err(anyhow!(
                "slide {slide_number} chart series {} must contain exactly one value per category",
                series_index + 1
            ));
        }
        let values = values
            .iter()
            .enumerate()
            .map(|(value_index, value)| {
                let value = value.as_f64().ok_or_else(|| {
                    anyhow!(
                        "slide {slide_number} chart series {} value {} must be a finite number",
                        series_index + 1,
                        value_index + 1
                    )
                })?;
                if !value.is_finite() || value.abs() > MAX_PPTX_CREATE_CHART_VALUE_ABS {
                    return Err(anyhow!(
                        "slide {slide_number} chart series {} value {} exceeds the numeric safety limit",
                        series_index + 1,
                        value_index + 1
                    ));
                }
                if chart_type.is_part_to_whole() && value < 0.0 {
                    return Err(anyhow!(
                        "slide {slide_number} {} chart values must be non-negative",
                        chart_type.as_str()
                    ));
                }
                Ok(value)
            })
            .collect::<Result<Vec<_>>>()?;
        if chart_type.is_part_to_whole() && !values.iter().any(|value| *value > 0.0) {
            return Err(anyhow!(
                "slide {slide_number} {} chart requires at least one positive value",
                chart_type.as_str()
            ));
        }
        let value_axis = PresentationChartValueAxis::parse(
            series
                .get("value_axis")
                .map(|value| {
                    value.as_str().ok_or_else(|| {
                        anyhow!(
                            "slide {slide_number} chart series {} value_axis must be a string",
                            series_index + 1
                        )
                    })
                })
                .transpose()?
                .unwrap_or("primary"),
        )?;
        let color = parse_presentation_chart_series_color(
            series.get("color"),
            slide_number,
            series_index + 1,
        )?;
        let (marker_style, marker_size) = parse_presentation_chart_series_marker(
            chart_type,
            series.get("marker_style"),
            series.get("marker_size"),
            slide_number,
            series_index + 1,
        )?;
        let smooth = parse_presentation_chart_series_smooth(
            chart_type,
            series.get("smooth"),
            slide_number,
            series_index + 1,
        )?;
        parsed_series.push(PresentationChartSeries {
            name: name.to_string(),
            values,
            value_axis,
            color,
            marker_style,
            marker_size,
            smooth,
        });
    }
    let secondary_series = parsed_series
        .iter()
        .filter(|series| series.value_axis == PresentationChartValueAxis::Secondary)
        .count();
    if chart_type.is_part_to_whole() && secondary_series != 0 {
        return Err(anyhow!(
            "slide {slide_number} {} chart series must use the primary value_axis",
            chart_type.as_str()
        ));
    }
    if secondary_series == parsed_series.len() {
        return Err(anyhow!(
            "slide {slide_number} chart secondary value axis requires at least one primary series"
        ));
    }
    if secondary_series == 0 && !secondary_value_axis_title.is_empty() {
        return Err(anyhow!(
            "slide {slide_number} chart secondary_value_axis_title requires at least one secondary series"
        ));
    }
    if secondary_series == 0
        && (secondary_value_axis_minimum.is_some()
            || secondary_value_axis_maximum.is_some()
            || secondary_value_axis_log_base.is_some()
            || secondary_value_axis_major_tick_mark != PresentationChartAxisTickMark::None
            || secondary_value_axis_minor_tick_mark != PresentationChartAxisTickMark::None
            || secondary_value_axis_major_unit.is_some()
            || secondary_value_axis_minor_unit.is_some()
            || secondary_value_axis_number_format
                != PresentationChartValueAxisNumberFormat::General)
    {
        return Err(anyhow!(
            "slide {slide_number} chart secondary value-axis bounds, logarithmic scale, tick marks, units, or number format require at least one secondary series"
        ));
    }
    validate_presentation_chart_axis_bounds(
        parsed_series.as_slice(),
        PresentationChartValueAxis::Primary,
        value_axis_minimum,
        value_axis_maximum,
        value_axis_log_base,
        value_axis_major_unit,
        value_axis_minor_unit,
        slide_number,
        "primary",
    )?;
    if secondary_series != 0 {
        validate_presentation_chart_axis_bounds(
            parsed_series.as_slice(),
            PresentationChartValueAxis::Secondary,
            secondary_value_axis_minimum,
            secondary_value_axis_maximum,
            secondary_value_axis_log_base,
            secondary_value_axis_major_unit,
            secondary_value_axis_minor_unit,
            slide_number,
            "secondary",
        )?;
    }
    let show_legend = object
        .get("show_legend")
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| anyhow!("slide {slide_number} chart show_legend must be a boolean"))
        })
        .transpose()?
        .unwrap_or(true);
    let legend_position = PresentationChartLegendPosition::parse(
        object
            .get("legend_position")
            .map(|value| {
                value.as_str().ok_or_else(|| {
                    anyhow!("slide {slide_number} chart legend_position must be a string")
                })
            })
            .transpose()?
            .unwrap_or("right"),
    )?;
    if !show_legend && legend_position != PresentationChartLegendPosition::Right {
        return Err(anyhow!(
            "slide {slide_number} chart legend_position must be right when show_legend=false"
        ));
    }
    let data_labels = PresentationChartDataLabels::parse(
        object
            .get("data_labels")
            .map(|value| {
                value.as_str().ok_or_else(|| {
                    anyhow!("slide {slide_number} chart data_labels must be a string")
                })
            })
            .transpose()?
            .unwrap_or("none"),
    )?;
    if data_labels == PresentationChartDataLabels::Percentage && !chart_type.is_part_to_whole() {
        return Err(anyhow!(
            "slide {slide_number} percentage data labels are supported only for pie or doughnut charts"
        ));
    }
    Ok(PresentationChart {
        chart_type,
        title,
        categories,
        series: parsed_series,
        show_legend,
        legend_position,
        data_labels,
        category_axis_title,
        value_axis_title,
        secondary_value_axis_title,
        value_axis_minimum,
        value_axis_maximum,
        value_axis_log_base,
        value_axis_major_tick_mark,
        value_axis_minor_tick_mark,
        value_axis_major_unit,
        value_axis_minor_unit,
        value_axis_number_format,
        secondary_value_axis_minimum,
        secondary_value_axis_maximum,
        secondary_value_axis_log_base,
        secondary_value_axis_major_tick_mark,
        secondary_value_axis_minor_tick_mark,
        secondary_value_axis_major_unit,
        secondary_value_axis_minor_unit,
        secondary_value_axis_number_format,
    })
}

fn parse_presentation_chart_series_color(
    value: Option<&Value>,
    slide_number: usize,
    series_number: usize,
) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let value = value.as_str().ok_or_else(|| {
        anyhow!(
            "slide {slide_number} chart series {series_number} color must be a #RRGGBB string or null"
        )
    })?;
    normalize_presentation_chart_api_color(value)
        .map(Some)
        .ok_or_else(|| {
            anyhow!(
            "slide {slide_number} chart series {series_number} color must use exact #RRGGBB syntax"
        )
        })
}

fn normalize_presentation_chart_api_color(value: &str) -> Option<String> {
    let rgb = value.strip_prefix('#')?;
    normalize_presentation_chart_rgb(rgb).map(|rgb| format!("#{rgb}"))
}

fn normalize_presentation_chart_rgb(value: &str) -> Option<String> {
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(value.to_ascii_uppercase())
}

fn parse_presentation_chart_series_marker(
    chart_type: PresentationChartType,
    style: Option<&Value>,
    size: Option<&Value>,
    slide_number: usize,
    series_number: usize,
) -> Result<(Option<PresentationChartMarkerStyle>, Option<u8>)> {
    if chart_type != PresentationChartType::Line {
        if style.is_some_and(|value| !value.is_null()) || size.is_some_and(|value| !value.is_null())
        {
            return Err(anyhow!(
                "slide {slide_number} chart series {series_number} marker_style and marker_size are supported only for line charts"
            ));
        }
        return Ok((None, None));
    }
    let style = match style {
        None => PresentationChartMarkerStyle::Circle,
        Some(value) if value.is_null() => PresentationChartMarkerStyle::Circle,
        Some(value) => PresentationChartMarkerStyle::parse(value.as_str().ok_or_else(|| {
            anyhow!(
                "slide {slide_number} chart series {series_number} marker_style must be none, circle, square, diamond, triangle, or null"
            )
        })?)?,
    };
    if style == PresentationChartMarkerStyle::None {
        if size.is_some_and(|value| !value.is_null()) {
            return Err(anyhow!(
                "slide {slide_number} chart series {series_number} marker_size must be null or omitted when marker_style=none"
            ));
        }
        return Ok((Some(style), None));
    }
    let size = match size {
        None => DEFAULT_PPTX_CREATE_CHART_MARKER_SIZE,
        Some(value) if value.is_null() => DEFAULT_PPTX_CREATE_CHART_MARKER_SIZE,
        Some(value) => {
            let value = value.as_u64().ok_or_else(|| {
                anyhow!(
                    "slide {slide_number} chart series {series_number} marker_size must be an integer between {MIN_PPTX_CREATE_CHART_MARKER_SIZE} and {MAX_PPTX_CREATE_CHART_MARKER_SIZE}, or null"
                )
            })?;
            let value = u8::try_from(value).map_err(|_| {
                anyhow!(
                    "slide {slide_number} chart series {series_number} marker_size exceeds the safety limit"
                )
            })?;
            if !(MIN_PPTX_CREATE_CHART_MARKER_SIZE..=MAX_PPTX_CREATE_CHART_MARKER_SIZE)
                .contains(&value)
            {
                return Err(anyhow!(
                    "slide {slide_number} chart series {series_number} marker_size must be between {MIN_PPTX_CREATE_CHART_MARKER_SIZE} and {MAX_PPTX_CREATE_CHART_MARKER_SIZE}"
                ));
            }
            value
        }
    };
    Ok((Some(style), Some(size)))
}

fn parse_presentation_chart_series_smooth(
    chart_type: PresentationChartType,
    value: Option<&Value>,
    slide_number: usize,
    series_number: usize,
) -> Result<Option<bool>> {
    if chart_type != PresentationChartType::Line {
        if value.is_some_and(|value| !value.is_null()) {
            return Err(anyhow!(
                "slide {slide_number} chart series {series_number} smooth is supported only for line charts"
            ));
        }
        return Ok(None);
    }
    match value {
        None => Ok(Some(false)),
        Some(value) if value.is_null() => Ok(Some(false)),
        Some(value) => value.as_bool().map(Some).ok_or_else(|| {
            anyhow!(
                "slide {slide_number} chart series {series_number} smooth must be a boolean or null"
            )
        }),
    }
}

fn parse_presentation_chart_axis_bound(
    value: Option<&Value>,
    slide_number: usize,
    field: &str,
) -> Result<Option<f64>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let value = value.as_f64().ok_or_else(|| {
        anyhow!("slide {slide_number} chart {field} must be a finite number or null")
    })?;
    if !value.is_finite() || value.abs() > MAX_PPTX_CREATE_CHART_VALUE_ABS {
        return Err(anyhow!(
            "slide {slide_number} chart {field} exceeds the numeric safety limit"
        ));
    }
    Ok(Some(value))
}

fn parse_presentation_chart_axis_number_format(
    value: Option<&Value>,
    slide_number: usize,
    field: &str,
) -> Result<PresentationChartValueAxisNumberFormat> {
    let value = value
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| anyhow!("slide {slide_number} chart {field} must be a string"))
        })
        .transpose()?
        .unwrap_or("general");
    PresentationChartValueAxisNumberFormat::parse(value)
}

fn parse_presentation_chart_axis_tick_mark(
    value: Option<&Value>,
    slide_number: usize,
    field: &str,
) -> Result<PresentationChartAxisTickMark> {
    let value = value
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| anyhow!("slide {slide_number} chart {field} must be a string"))
        })
        .transpose()?
        .unwrap_or("none");
    PresentationChartAxisTickMark::parse(value)
}

fn parse_presentation_chart_axis_log_base(
    value: Option<&Value>,
    slide_number: usize,
    field: &str,
) -> Result<Option<f64>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let value = value.as_f64().ok_or_else(|| {
        anyhow!(
            "slide {slide_number} chart {field} must be a finite number between {MIN_PPTX_CREATE_CHART_LOG_BASE} and {MAX_PPTX_CREATE_CHART_LOG_BASE}, or null"
        )
    })?;
    if !value.is_finite()
        || !(MIN_PPTX_CREATE_CHART_LOG_BASE..=MAX_PPTX_CREATE_CHART_LOG_BASE).contains(&value)
    {
        return Err(anyhow!(
            "slide {slide_number} chart {field} must be between {MIN_PPTX_CREATE_CHART_LOG_BASE} and {MAX_PPTX_CREATE_CHART_LOG_BASE}"
        ));
    }
    Ok(Some(value))
}

fn parse_presentation_chart_axis_unit(
    value: Option<&Value>,
    slide_number: usize,
    field: &str,
) -> Result<Option<f64>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let value = value.as_f64().ok_or_else(|| {
        anyhow!("slide {slide_number} chart {field} must be a finite positive number or null")
    })?;
    if !value.is_finite() || value <= 0.0 || value > MAX_PPTX_CREATE_CHART_VALUE_ABS {
        return Err(anyhow!(
            "slide {slide_number} chart {field} must be positive and within the numeric safety limit"
        ));
    }
    Ok(Some(value))
}

fn validate_presentation_chart_axis_bounds(
    series: &[PresentationChartSeries],
    axis: PresentationChartValueAxis,
    minimum: Option<f64>,
    maximum: Option<f64>,
    log_base: Option<f64>,
    major_unit: Option<f64>,
    minor_unit: Option<f64>,
    slide_number: usize,
    label: &str,
) -> Result<()> {
    if matches!((minimum, maximum), (Some(minimum), Some(maximum)) if minimum >= maximum) {
        return Err(anyhow!(
            "slide {slide_number} chart {label} value-axis minimum must be below its maximum"
        ));
    }
    if matches!((major_unit, minor_unit), (Some(major_unit), Some(minor_unit)) if minor_unit >= major_unit)
    {
        return Err(anyhow!(
            "slide {slide_number} chart {label} value-axis minor unit must be below its major unit"
        ));
    }
    if let (Some(minimum), Some(maximum)) = (minimum, maximum) {
        let span = maximum - minimum;
        if major_unit.is_some_and(|major_unit| major_unit > span) {
            return Err(anyhow!(
                "slide {slide_number} chart {label} value-axis major unit exceeds its explicit range"
            ));
        }
        if minor_unit.is_some_and(|minor_unit| minor_unit > span) {
            return Err(anyhow!(
                "slide {slide_number} chart {label} value-axis minor unit exceeds its explicit range"
            ));
        }
    }
    let values = series
        .iter()
        .filter(|series| series.value_axis == axis)
        .flat_map(|series| series.values.iter().copied())
        .collect::<Vec<_>>();
    let data_minimum =
        values.iter().copied().reduce(f64::min).ok_or_else(|| {
            anyhow!("slide {slide_number} chart {label} value axis has no series")
        })?;
    let data_maximum =
        values.iter().copied().reduce(f64::max).ok_or_else(|| {
            anyhow!("slide {slide_number} chart {label} value axis has no series")
        })?;
    if log_base.is_some() {
        if minimum.is_some_and(|minimum| minimum <= 0.0)
            || maximum.is_some_and(|maximum| maximum <= 0.0)
        {
            return Err(anyhow!(
                "slide {slide_number} chart {label} logarithmic value-axis bounds must be positive"
            ));
        }
        if data_minimum <= 0.0 {
            return Err(anyhow!(
                "slide {slide_number} chart {label} logarithmic value axis requires every series value to be positive"
            ));
        }
    }
    if minimum.is_some_and(|minimum| minimum > data_minimum) {
        return Err(anyhow!(
            "slide {slide_number} chart {label} value-axis minimum would hide series values"
        ));
    }
    if maximum.is_some_and(|maximum| maximum < data_maximum) {
        return Err(anyhow!(
            "slide {slide_number} chart {label} value-axis maximum would hide series values"
        ));
    }
    Ok(())
}

fn optional_slide_text(value: Option<&Value>, field: &str) -> Result<String> {
    let text = value
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| anyhow!("{field} must be a string"))
        })
        .transpose()?
        .unwrap_or_default()
        .to_string();
    validate_slide_text(text.as_str(), field, MAX_SLIDE_TEXT_CHARS)?;
    Ok(text)
}

fn validate_slide_text(value: &str, field: &str, max_chars: usize) -> Result<()> {
    if value.chars().count() > max_chars {
        return Err(anyhow!(
            "{field} exceeds the {max_chars} character safety limit"
        ));
    }
    if value.lines().count() > MAX_SLIDE_LINES {
        return Err(anyhow!(
            "{field} exceeds the {MAX_SLIDE_LINES} line safety limit"
        ));
    }
    if value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\t' | '\r' | '\n'))
    {
        return Err(anyhow!(
            "{field} contains XML-incompatible control characters"
        ));
    }
    Ok(())
}

fn parse_image(
    value: &Value,
    state: &LocalState,
    request: &RelayRequest,
    slide_number: usize,
) -> Result<PresentationImage> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("slide image must be an object"))?;
    let requested = object
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("slide image path is required"))?;
    let (path, relative) = input_file_any(state, request, requested)?;
    let bytes =
        fs::read(path.as_path()).with_context(|| format!("read slide image {}", path.display()))?;
    if bytes.is_empty() || bytes.len() > MAX_PPTX_IMAGE_BYTES {
        return Err(anyhow!(
            "slide image must contain between 1 byte and 10 MiB"
        ));
    }
    let (format, width, height) = validate_image(path.as_path(), bytes.as_slice())?;
    let alt_text = object
        .get("alt_text")
        .and_then(Value::as_str)
        .unwrap_or("Presentation image")
        .to_string();
    validate_slide_text(alt_text.as_str(), "image alt_text", 1_024)?;
    let fit = match object
        .get("fit")
        .and_then(Value::as_str)
        .unwrap_or("contain")
    {
        "contain" => ImageFit::Contain,
        "cover" => ImageFit::Cover,
        value => return Err(anyhow!("unsupported slide image fit: {value}")),
    };
    if alt_text.trim().is_empty() {
        return Err(anyhow!(
            "slide {slide_number} image alt_text cannot be empty"
        ));
    }
    Ok(PresentationImage {
        source_path: relative,
        bytes,
        format,
        width,
        height,
        alt_text,
        fit,
    })
}

fn validate_image(path: &Path, bytes: &[u8]) -> Result<(PresentationImageFormat, u32, u32)> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let (format, width, height) = match extension.as_str() {
        "png" => {
            let (width, height) = png_dimensions(bytes)?;
            (PresentationImageFormat::Png, width, height)
        }
        "jpg" | "jpeg" => {
            let (width, height) = jpeg_dimensions(bytes)?;
            (PresentationImageFormat::Jpeg, width, height)
        }
        _ => return Err(anyhow!("PPTX images must use .png, .jpg, or .jpeg")),
    };
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width == 0
        || height == 0
        || width > 20_000
        || height > 20_000
        || pixels > MAX_PPTX_IMAGE_PIXELS
    {
        return Err(anyhow!(
            "PPTX image dimensions exceed the 20000 px edge or 40 megapixel safety limit"
        ));
    }
    Ok((format, width, height))
}

fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 33 || !bytes.starts_with(SIGNATURE) {
        return Err(anyhow!(
            "PNG image has an invalid signature or chunk structure"
        ));
    }
    if &bytes[12..16] != b"IHDR" || u32::from_be_bytes(bytes[8..12].try_into()?) != 13 {
        return Err(anyhow!("PNG image must begin with a valid IHDR chunk"));
    }
    if !bytes.ends_with(b"IEND\xaeB`\x82") {
        return Err(anyhow!("PNG image is missing a valid terminal IEND chunk"));
    }
    Ok((
        u32::from_be_bytes(bytes[16..20].try_into()?),
        u32::from_be_bytes(bytes[20..24].try_into()?),
    ))
}

fn jpeg_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    if bytes.len() < 4 || !bytes.starts_with(&[0xff, 0xd8]) || !bytes.ends_with(&[0xff, 0xd9]) {
        return Err(anyhow!("JPEG image has an invalid start or end marker"));
    }
    let mut cursor = 2usize;
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor] != 0xff {
            cursor += 1;
        }
        while cursor < bytes.len() && bytes[cursor] == 0xff {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }
        let marker = bytes[cursor];
        cursor += 1;
        if matches!(marker, 0x01 | 0xd0..=0xd9) {
            continue;
        }
        if cursor + 2 > bytes.len() {
            break;
        }
        let segment_length = usize::from(u16::from_be_bytes([bytes[cursor], bytes[cursor + 1]]));
        if segment_length < 2 || cursor.saturating_add(segment_length) > bytes.len() {
            return Err(anyhow!("JPEG image contains an invalid segment length"));
        }
        if matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) {
            if segment_length < 7 {
                return Err(anyhow!("JPEG image has an invalid frame header"));
            }
            let height = u32::from(u16::from_be_bytes([bytes[cursor + 3], bytes[cursor + 4]]));
            let width = u32::from(u16::from_be_bytes([bytes[cursor + 5], bytes[cursor + 6]]));
            return Ok((width, height));
        }
        if marker == 0xda {
            break;
        }
        cursor += segment_length;
    }
    Err(anyhow!("JPEG image is missing a supported frame header"))
}

fn presentation_entries(slides: &[SlideDefinition]) -> Result<Vec<(String, Vec<u8>)>> {
    let notes_present = slides.iter().any(|slide| !slide.notes.is_empty());
    let image_formats = slides
        .iter()
        .filter_map(|slide| slide.image.as_ref().map(|image| image.format))
        .collect::<HashSet<_>>();
    let mut entries = vec![
        (
            "[Content_Types].xml".to_string(),
            content_types(slides, notes_present, &image_formats).into_bytes(),
        ),
        ("_rels/.rels".to_string(), root_relationships().into_bytes()),
        (
            "ppt/presentation.xml".to_string(),
            presentation_xml(slides.len(), notes_present).into_bytes(),
        ),
        (
            "ppt/_rels/presentation.xml.rels".to_string(),
            presentation_relationships(slides.len(), notes_present).into_bytes(),
        ),
        (
            "ppt/slideMasters/slideMaster1.xml".to_string(),
            slide_master_xml().into_bytes(),
        ),
        (
            "ppt/slideMasters/_rels/slideMaster1.xml.rels".to_string(),
            slide_master_relationships().into_bytes(),
        ),
        (
            "ppt/slideLayouts/slideLayout1.xml".to_string(),
            slide_layout_xml().into_bytes(),
        ),
        (
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels".to_string(),
            slide_layout_relationships().into_bytes(),
        ),
        ("ppt/theme/theme1.xml".to_string(), theme_xml().into_bytes()),
    ];
    if notes_present {
        entries.push((
            "ppt/notesMasters/notesMaster1.xml".to_string(),
            notes_master_xml().into_bytes(),
        ));
        entries.push((
            "ppt/notesMasters/_rels/notesMaster1.xml.rels".to_string(),
            notes_master_relationships().into_bytes(),
        ));
    }
    let mut media_number = 0usize;
    let mut notes_number = 0usize;
    let mut chart_number = 0usize;
    for (index, slide) in slides.iter().enumerate() {
        let slide_number = index + 1;
        let media = slide.image.as_ref().map(|image| {
            media_number += 1;
            (media_number, image)
        });
        let note = if slide.notes.is_empty() {
            None
        } else {
            notes_number += 1;
            Some(notes_number)
        };
        let chart = slide.chart.as_ref().map(|chart| {
            chart_number += 1;
            (chart_number, chart)
        });
        entries.push((
            format!("ppt/slides/slide{slide_number}.xml"),
            slide_xml(slide, media.map(|_| "rId2"), chart.map(|_| "rId2"))?.into_bytes(),
        ));
        entries.push((
            format!("ppt/slides/_rels/slide{slide_number}.xml.rels"),
            slide_relationships(
                media.map(|(number, image)| (number, image.format)),
                chart.map(|(number, _)| number),
                note,
            )
            .into_bytes(),
        ));
        if let Some((number, image)) = media {
            entries.push((
                format!("ppt/media/image{number}.{}", image.format.extension()),
                image.bytes.clone(),
            ));
        }
        if let Some(note_number) = note {
            entries.push((
                format!("ppt/notesSlides/notesSlide{note_number}.xml"),
                notes_slide_xml(slide.notes.as_str(), slide_number)?.into_bytes(),
            ));
            entries.push((
                format!("ppt/notesSlides/_rels/notesSlide{note_number}.xml.rels"),
                notes_slide_relationships(slide_number).into_bytes(),
            ));
        }
        if let Some((chart_number, chart)) = chart {
            entries.push((
                format!("ppt/charts/chart{chart_number}.xml"),
                presentation_chart_xml(chart)?.into_bytes(),
            ));
        }
    }
    Ok(entries)
}

fn content_types(
    slides: &[SlideDefinition],
    notes_present: bool,
    image_formats: &HashSet<PresentationImageFormat>,
) -> String {
    let slide_parts = (1..=slides.len())
        .map(|index| format!("<Override PartName=\"/ppt/slides/slide{index}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"))
        .collect::<String>();
    let notes_parts = slides
        .iter()
        .filter(|slide| !slide.notes.is_empty())
        .enumerate()
        .map(|(index, _)| format!("<Override PartName=\"/ppt/notesSlides/notesSlide{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml\"/>", index + 1))
        .collect::<String>();
    let notes_master = if notes_present {
        r#"<Override PartName="/ppt/notesMasters/notesMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml"/>"#
    } else {
        ""
    };
    let chart_parts = slides
        .iter()
        .filter(|slide| slide.chart.is_some())
        .enumerate()
        .map(|(index, _)| format!("<Override PartName=\"/ppt/charts/chart{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawingml.chart+xml\"/>", index + 1))
        .collect::<String>();
    let png = if image_formats.contains(&PresentationImageFormat::Png) {
        r#"<Default Extension="png" ContentType="image/png"/>"#
    } else {
        ""
    };
    let jpeg = if image_formats.contains(&PresentationImageFormat::Jpeg) {
        r#"<Default Extension="jpg" ContentType="image/jpeg"/><Default Extension="jpeg" ContentType="image/jpeg"/>"#
    } else {
        ""
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>{png}{jpeg}<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>{notes_master}{slide_parts}{notes_parts}{chart_parts}</Types>"#
    )
}

fn root_relationships() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#.to_string()
}

fn presentation_xml(slide_count: usize, notes_present: bool) -> String {
    let slide_ids = (1..=slide_count)
        .map(|index| {
            format!(
                "<p:sldId id=\"{}\" r:id=\"rId{}\"/>",
                255 + index,
                index + 1
            )
        })
        .collect::<String>();
    let notes = if notes_present {
        format!(
            "<p:notesMasterIdLst><p:notesMasterId r:id=\"rId{}\"/></p:notesMasterIdLst>",
            slide_count + 2
        )
    } else {
        String::new()
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>{notes}<p:sldIdLst>{slide_ids}</p:sldIdLst><p:sldSz cx="{SLIDE_WIDTH}" cy="{SLIDE_HEIGHT}" type="screen16x9"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>"#
    )
}

fn presentation_relationships(slide_count: usize, notes_present: bool) -> String {
    let slides = (1..=slide_count)
        .map(|index| format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide{index}.xml\"/>", index + 1))
        .collect::<String>();
    let notes = if notes_present {
        format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster\" Target=\"notesMasters/notesMaster1.xml\"/>", slide_count + 2)
    } else {
        String::new()
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>{slides}{notes}</Relationships>"#
    )
}

fn slide_xml(
    slide: &SlideDefinition,
    image_relationship: Option<&str>,
    chart_relationship: Option<&str>,
) -> Result<String> {
    let content = match slide.layout {
        SlideLayout::TitleBody => format!(
            "{}{}",
            text_shape(
                2,
                "Title",
                685_800,
                365_760,
                10_820_400,
                1_005_840,
                2_800,
                slide.title.as_str(),
                true,
                "1F2937",
                "left",
                None
            ),
            text_shape(
                3,
                "Body",
                914_400,
                1_554_480,
                10_363_200,
                4_572_000,
                1_800,
                slide.body.as_str(),
                false,
                "1F2937",
                "left",
                None
            )
        ),
        SlideLayout::TitleOnly => text_shape(
            2,
            "Title",
            914_400,
            2_194_560,
            10_363_200,
            1_828_800,
            3_200,
            slide.title.as_str(),
            true,
            "1F2937",
            "center",
            None,
        ),
        SlideLayout::Section => format!(
            "{}{}",
            text_shape(
                2,
                "Section Title",
                914_400,
                1_828_800,
                10_363_200,
                1_371_600,
                3_200,
                slide.title.as_str(),
                true,
                "FFFFFF",
                "center",
                Some(("2563EB", 100_000))
            ),
            text_shape(
                3,
                "Section Subtitle",
                1_371_600,
                3_383_280,
                9_448_800,
                1_371_600,
                2_000,
                slide.body.as_str(),
                false,
                "1F2937",
                "center",
                None
            )
        ),
        SlideLayout::TwoColumn => format!(
            "{}{}{}",
            text_shape(
                2,
                "Title",
                685_800,
                365_760,
                10_820_400,
                1_005_840,
                2_800,
                slide.title.as_str(),
                true,
                "1F2937",
                "left",
                None
            ),
            text_shape(
                3,
                "Left Column",
                685_800,
                1_554_480,
                5_212_080,
                4_754_880,
                1_650,
                slide.left_body.as_str(),
                false,
                "1F2937",
                "left",
                None
            ),
            text_shape(
                4,
                "Right Column",
                6_294_120,
                1_554_480,
                5_212_080,
                4_754_880,
                1_650,
                slide.right_body.as_str(),
                false,
                "1F2937",
                "left",
                None
            )
        ),
        SlideLayout::ImageRight => {
            let image = slide.image.as_ref().expect("validated image_right image");
            format!(
                "{}{}{}",
                text_shape(
                    2,
                    "Title",
                    685_800,
                    365_760,
                    10_820_400,
                    1_005_840,
                    2_800,
                    slide.title.as_str(),
                    true,
                    "1F2937",
                    "left",
                    None
                ),
                text_shape(
                    3,
                    "Body",
                    685_800,
                    1_554_480,
                    5_029_200,
                    4_754_880,
                    1_650,
                    slide.body.as_str(),
                    false,
                    "1F2937",
                    "left",
                    None
                ),
                picture_shape(
                    4,
                    image_relationship.expect("image relationship"),
                    image,
                    6_035_040,
                    1_462_080,
                    5_486_400,
                    4_937_760
                )
            )
        }
        SlideLayout::ImageFull => {
            let image = slide.image.as_ref().expect("validated image_full image");
            format!(
                "{}{}{}",
                picture_shape(
                    2,
                    image_relationship.expect("image relationship"),
                    image,
                    0,
                    0,
                    SLIDE_WIDTH,
                    SLIDE_HEIGHT
                ),
                text_shape(
                    3,
                    "Title Overlay",
                    548_640,
                    365_760,
                    11_094_720,
                    1_097_280,
                    3_000,
                    slide.title.as_str(),
                    true,
                    "FFFFFF",
                    "left",
                    Some(("111827", 70_000))
                ),
                text_shape(
                    4,
                    "Body Overlay",
                    548_640,
                    5_120_640,
                    11_094_720,
                    1_188_720,
                    1_600,
                    slide.body.as_str(),
                    false,
                    "FFFFFF",
                    "left",
                    Some(("111827", 70_000))
                )
            )
        }
        SlideLayout::Table => format!(
            "{}{}",
            text_shape(
                2,
                "Title",
                685_800,
                365_760,
                10_820_400,
                1_005_840,
                2_800,
                slide.title.as_str(),
                true,
                "1F2937",
                "left",
                None
            ),
            table_shape(
                3,
                "Table 1",
                685_800,
                1_554_480,
                10_820_400,
                4_754_880,
                slide.table.as_ref().expect("validated table layout")
            )
        ),
        SlideLayout::Chart => format!(
            "{}{}",
            text_shape(
                2,
                "Title",
                685_800,
                365_760,
                10_820_400,
                1_005_840,
                2_800,
                slide.title.as_str(),
                true,
                "1F2937",
                "left",
                None
            ),
            chart_shape(
                3,
                "Chart 1",
                chart_relationship.expect("chart relationship"),
                685_800,
                1_462_080,
                10_820_400,
                4_846_320,
            )
        ),
    };
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree>{}{content}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#,
        group_shape()
    ))
}

#[allow(clippy::too_many_arguments)]
fn table_shape(
    id: usize,
    name: &str,
    x: i64,
    y: i64,
    cx: i64,
    cy: i64,
    table: &PresentationTable,
) -> String {
    let rows = table.cells.len();
    let columns = table.cells[0].len();
    let column_widths = distributed_table_sizes(cx, columns);
    let row_heights = distributed_table_sizes(cy, rows);
    let font_size = if columns >= 12 || rows >= 30 {
        1_000
    } else if columns >= 8 || rows >= 20 {
        1_200
    } else {
        1_400
    };
    let grid = column_widths
        .iter()
        .map(|width| format!("<a:gridCol w=\"{width}\"/>"))
        .collect::<String>();
    let rows_xml = table
        .cells
        .iter()
        .zip(row_heights)
        .enumerate()
        .map(|(row_index, (row, height))| {
            let cells = row
                .iter()
                .map(|cell| {
                    let bold = usize::from(table.header_row && row_index == 0);
                    format!(
                        r#"<a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:pPr algn="l"/><a:r><a:rPr lang="zh-CN" sz="{font_size}" b="{bold}"><a:solidFill><a:srgbClr val="1F2937"/></a:solidFill></a:rPr><a:t xml:space="preserve">{}</a:t></a:r><a:endParaRPr lang="zh-CN" sz="{font_size}"/></a:p></a:txBody><a:tcPr marL="45720" marR="45720" marT="22860" marB="22860" anchor="ctr"/></a:tc>"#,
                        escape_xml(cell)
                    )
                })
                .collect::<String>();
            format!("<a:tr h=\"{height}\">{cells}</a:tr>")
        })
        .collect::<String>();
    let first_row = usize::from(table.header_row);
    format!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="{id}" name="{}"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl><a:tblPr firstRow="{first_row}" bandRow="1"><a:tableStyleId>{{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}}</a:tableStyleId></a:tblPr><a:tblGrid>{grid}</a:tblGrid>{rows_xml}</a:tbl></a:graphicData></a:graphic></p:graphicFrame>"#,
        escape_xml(name)
    )
}

#[allow(clippy::too_many_arguments)]
fn chart_shape(
    id: usize,
    name: &str,
    relationship_id: &str,
    x: i64,
    y: i64,
    cx: i64,
    cy: i64,
) -> String {
    format!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="{id}" name="{}"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="{}"/></a:graphicData></a:graphic></p:graphicFrame>"#,
        escape_xml(name),
        escape_xml(relationship_id)
    )
}

fn presentation_chart_xml(chart: &PresentationChart) -> Result<String> {
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
        .collect::<String>();
    let secondary_series = chart
        .series
        .iter()
        .enumerate()
        .filter(|(_, series)| series.value_axis == PresentationChartValueAxis::Secondary)
        .map(|(index, series)| presentation_chart_series_xml(chart, index, series))
        .collect::<String>();
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
        let axes = presentation_chart_axes_xml(
            PPTX_PRIMARY_CATEGORY_AXIS_ID,
            PPTX_PRIMARY_VALUE_AXIS_ID,
            cross_between,
            chart.category_axis_title.as_str(),
            chart.value_axis_title.as_str(),
            PresentationChartValueAxisOptions {
                minimum: chart.value_axis_minimum,
                maximum: chart.value_axis_maximum,
                log_base: chart.value_axis_log_base,
                major_tick_mark: chart.value_axis_major_tick_mark,
                minor_tick_mark: chart.value_axis_minor_tick_mark,
                major_unit: chart.value_axis_major_unit,
                minor_unit: chart.value_axis_minor_unit,
                number_format: chart.value_axis_number_format,
            },
        );
        if has_secondary_axis {
            let secondary_group = presentation_chart_group_xml(
                chart.chart_type,
                secondary_series.as_str(),
                data_labels,
                PPTX_SECONDARY_CATEGORY_AXIS_ID,
                PPTX_SECONDARY_VALUE_AXIS_ID,
            );
            format!(
                "{primary_group}{secondary_group}{axes}{}",
                presentation_chart_secondary_axes_xml(
                    PPTX_SECONDARY_CATEGORY_AXIS_ID,
                    PPTX_SECONDARY_VALUE_AXIS_ID,
                    cross_between,
                    chart.secondary_value_axis_title.as_str(),
                    PresentationChartValueAxisOptions {
                        minimum: chart.secondary_value_axis_minimum,
                        maximum: chart.secondary_value_axis_maximum,
                        log_base: chart.secondary_value_axis_log_base,
                        major_tick_mark: chart.secondary_value_axis_major_tick_mark,
                        minor_tick_mark: chart.secondary_value_axis_minor_tick_mark,
                        major_unit: chart.secondary_value_axis_major_unit,
                        minor_unit: chart.secondary_value_axis_minor_unit,
                        number_format: chart.secondary_value_axis_number_format,
                    },
                )
            )
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
                .saturating_mul(chart.categories.len())
                .saturating_mul(2)
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
    }
}

fn presentation_chart_title_xml(title: &str) -> String {
    format!(
        r#"<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="zh-CN"/><a:t xml:space="preserve">{}</a:t></a:r><a:endParaRPr lang="zh-CN"/></a:p></c:rich></c:tx><c:layout/><c:overlay val="0"/></c:title>"#,
        escape_xml(title)
    )
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
) -> String {
    let color = presentation_chart_series_color_xml(chart.chart_type, series.color.as_deref());
    let marker = presentation_chart_series_marker_xml(series.marker_style, series.marker_size);
    let smooth = series
        .smooth
        .map(|smooth| format!(r#"<c:smooth val="{}"/>"#, u8::from(smooth)))
        .unwrap_or_default();
    format!(
        r#"<c:ser><c:idx val="{index}"/><c:order val="{index}"/><c:tx><c:v>{}</c:v></c:tx>{color}{marker}<c:cat>{}</c:cat><c:val>{}</c:val>{smooth}</c:ser>"#,
        escape_xml(series.name.as_str()),
        presentation_chart_string_literal(chart.categories.as_slice()),
        presentation_chart_number_literal(series.values.as_slice())
    )
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
    if chart_type == PresentationChartType::Line {
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

fn presentation_chart_number_text(value: f64) -> String {
    if value == 0.0 {
        "0".to_string()
    } else {
        value.to_string()
    }
}

fn presentation_chart_snapshot(chart: &PresentationChart) -> Value {
    json!({
        "type": chart.chart_type.as_str(),
        "title": chart.title,
        "categories": chart.categories,
        "series": chart.series.iter().map(|series| json!({
            "name": series.name,
            "values": series.values,
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

fn canonical_pptx_chart_snapshot(
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
        "bar" => PresentationChartType::Column,
        "line" => PresentationChartType::Line,
        "pie" => PresentationChartType::Pie,
        "area" => PresentationChartType::Area,
        "doughnut" => PresentationChartType::Doughnut,
        _ => {
            return Err(anyhow!(
                "chart type is outside the canonical column, line, pie, area, or doughnut contract"
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
    let (value_axis_options, secondary_value_axis_options) = if chart_type.is_part_to_whole() {
        (default_axis_options(), default_axis_options())
    } else {
        let primary_axis = pptx_chart_value_axis_by_position(inspection.axes.as_slice(), "l")
            .ok_or_else(|| anyhow!("canonical chart is missing its primary left value axis"))?;
        let primary_options = canonical_pptx_chart_axis_options(primary_axis, "primary")?;
        let secondary_options =
            match pptx_chart_value_axis_by_position(inspection.axes.as_slice(), "r") {
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
    let categories = inspection.series[0].categories.clone();
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
        if !item.bubble_sizes.is_empty() {
            return Err(anyhow!(
                "canonical self-contained charts must not contain bubble-size caches"
            ));
        }
        if item.categories != categories {
            return Err(anyhow!(
                "canonical self-contained chart series must share identical categories"
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
        if chart_type == PresentationChartType::Line {
            if marker_style.is_none() {
                return Err(anyhow!(
                    "canonical line chart series requires one marker style"
                ));
            }
        } else if marker_style.is_some() || item.marker_size.is_some() {
            return Err(anyhow!(
                "canonical non-line chart series must not contain marker styling"
            ));
        }
        if item.smooth_custom {
            return Err(anyhow!(
                "canonical self-contained chart series smoothing is outside the exact bounded line-smoothing contract"
            ));
        }
        if chart_type == PresentationChartType::Line {
            if item.smooth.is_none() {
                return Err(anyhow!(
                    "canonical line chart series requires one smooth value"
                ));
            }
        } else if item.smooth.is_some() {
            return Err(anyhow!(
                "canonical non-line chart series must not contain smoothing"
            ));
        }
        series.push(PresentationChartSeries {
            name: item.name.clone(),
            values,
            value_axis,
            color: item.color.clone(),
            marker_style,
            marker_size: item.marker_size,
            smooth: item.smooth,
        });
    }
    let candidate = PresentationChart {
        chart_type,
        title: inspection.title.clone(),
        categories,
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

fn presentation_chart_axes_xml(
    category_axis_id: u32,
    value_axis_id: u32,
    cross_between: &str,
    category_axis_title: &str,
    value_axis_title: &str,
    value_axis_options: PresentationChartValueAxisOptions,
) -> String {
    let PresentationChartValueAxisOptions {
        minimum,
        maximum,
        log_base,
        major_tick_mark,
        minor_tick_mark,
        major_unit,
        minor_unit,
        number_format,
    } = value_axis_options;
    let category_axis_title = if category_axis_title.is_empty() {
        String::new()
    } else {
        presentation_chart_title_xml(category_axis_title)
    };
    let value_axis_title = if value_axis_title.is_empty() {
        String::new()
    } else {
        presentation_chart_title_xml(value_axis_title)
    };
    let value_axis_scaling = presentation_chart_axis_scaling_xml(minimum, maximum, log_base);
    let value_axis_number_format = presentation_chart_axis_number_format_xml(number_format);
    let value_axis_tick_marks =
        presentation_chart_axis_tick_marks_xml(major_tick_mark, minor_tick_mark);
    let value_axis_units = presentation_chart_axis_units_xml(major_unit, minor_unit);
    format!(
        r#"<c:catAx><c:axId val="{category_axis_id}"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/>{category_axis_title}<c:tickLblPos val="nextTo"/><c:crossAx val="{value_axis_id}"/><c:crosses val="autoZero"/><c:auto val="1"/><c:lblAlgn val="ctr"/><c:lblOffset val="100"/></c:catAx><c:valAx><c:axId val="{value_axis_id}"/>{value_axis_scaling}<c:delete val="0"/><c:axPos val="l"/><c:majorGridlines/>{value_axis_title}{value_axis_number_format}{value_axis_tick_marks}<c:tickLblPos val="nextTo"/><c:crossAx val="{category_axis_id}"/><c:crosses val="autoZero"/><c:crossBetween val="{cross_between}"/>{value_axis_units}</c:valAx>"#
    )
}

fn presentation_chart_secondary_axes_xml(
    category_axis_id: u32,
    value_axis_id: u32,
    cross_between: &str,
    value_axis_title: &str,
    value_axis_options: PresentationChartValueAxisOptions,
) -> String {
    let PresentationChartValueAxisOptions {
        minimum,
        maximum,
        log_base,
        major_tick_mark,
        minor_tick_mark,
        major_unit,
        minor_unit,
        number_format,
    } = value_axis_options;
    let value_axis_title = if value_axis_title.is_empty() {
        String::new()
    } else {
        presentation_chart_title_xml(value_axis_title)
    };
    let value_axis_scaling = presentation_chart_axis_scaling_xml(minimum, maximum, log_base);
    let value_axis_number_format = presentation_chart_axis_number_format_xml(number_format);
    let value_axis_tick_marks =
        presentation_chart_axis_tick_marks_xml(major_tick_mark, minor_tick_mark);
    let value_axis_units = presentation_chart_axis_units_xml(major_unit, minor_unit);
    format!(
        r#"<c:catAx><c:axId val="{category_axis_id}"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="1"/><c:axPos val="t"/><c:tickLblPos val="none"/><c:crossAx val="{value_axis_id}"/><c:crosses val="max"/><c:auto val="1"/><c:lblAlgn val="ctr"/><c:lblOffset val="100"/></c:catAx><c:valAx><c:axId val="{value_axis_id}"/>{value_axis_scaling}<c:delete val="0"/><c:axPos val="r"/>{value_axis_title}{value_axis_number_format}{value_axis_tick_marks}<c:tickLblPos val="nextTo"/><c:crossAx val="{category_axis_id}"/><c:crosses val="max"/><c:crossBetween val="{cross_between}"/>{value_axis_units}</c:valAx>"#
    )
}

fn presentation_chart_axis_scaling_xml(
    minimum: Option<f64>,
    maximum: Option<f64>,
    log_base: Option<f64>,
) -> String {
    let log_base = log_base
        .map(|value| {
            format!(
                r#"<c:logBase val="{}"/>"#,
                presentation_chart_number_text(value)
            )
        })
        .unwrap_or_default();
    let maximum = maximum
        .map(|value| {
            format!(
                r#"<c:max val="{}"/>"#,
                presentation_chart_number_text(value)
            )
        })
        .unwrap_or_default();
    let minimum = minimum
        .map(|value| {
            format!(
                r#"<c:min val="{}"/>"#,
                presentation_chart_number_text(value)
            )
        })
        .unwrap_or_default();
    format!(r#"<c:scaling>{log_base}<c:orientation val="minMax"/>{maximum}{minimum}</c:scaling>"#)
}

fn presentation_chart_axis_number_format_xml(
    number_format: PresentationChartValueAxisNumberFormat,
) -> String {
    format!(
        r#"<c:numFmt formatCode="{}" sourceLinked="{}"/>"#,
        number_format.format_code(),
        if number_format.source_linked() {
            "1"
        } else {
            "0"
        }
    )
}

fn presentation_chart_axis_tick_marks_xml(
    major_tick_mark: PresentationChartAxisTickMark,
    minor_tick_mark: PresentationChartAxisTickMark,
) -> String {
    let major_tick_mark = match major_tick_mark {
        PresentationChartAxisTickMark::None => String::new(),
        value => format!(r#"<c:majorTickMark val="{}"/>"#, value.as_ooxml()),
    };
    let minor_tick_mark = match minor_tick_mark {
        PresentationChartAxisTickMark::None => String::new(),
        value => format!(r#"<c:minorTickMark val="{}"/>"#, value.as_ooxml()),
    };
    format!("{major_tick_mark}{minor_tick_mark}")
}

fn presentation_chart_axis_units_xml(major_unit: Option<f64>, minor_unit: Option<f64>) -> String {
    let major_unit = major_unit
        .map(|value| {
            format!(
                r#"<c:majorUnit val="{}"/>"#,
                presentation_chart_number_text(value)
            )
        })
        .unwrap_or_default();
    let minor_unit = minor_unit
        .map(|value| {
            format!(
                r#"<c:minorUnit val="{}"/>"#,
                presentation_chart_number_text(value)
            )
        })
        .unwrap_or_default();
    format!("{major_unit}{minor_unit}")
}

fn distributed_table_sizes(total: i64, parts: usize) -> Vec<i64> {
    let parts = i64::try_from(parts).expect("validated table dimension");
    let base = total / parts;
    (0..parts)
        .map(|index| {
            if index + 1 == parts {
                total - base * (parts - 1)
            } else {
                base
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn text_shape(
    id: usize,
    name: &str,
    x: i64,
    y: i64,
    cx: i64,
    cy: i64,
    font_size: usize,
    text: &str,
    bold: bool,
    color: &str,
    alignment: &str,
    fill: Option<(&str, usize)>,
) -> String {
    let paragraphs = text_paragraphs(text, font_size, bold, color, alignment);
    let fill = fill.map_or_else(
        || "<a:noFill/>".to_string(),
        |(color, alpha)| format!("<a:solidFill><a:srgbClr val=\"{color}\"><a:alpha val=\"{alpha}\"/></a:srgbClr></a:solidFill>"),
    );
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{fill}<a:ln><a:noFill/></a:ln></p:spPr><p:txBody><a:bodyPr wrap="square" lIns="91440" rIns="91440" tIns="45720" bIns="45720"/><a:lstStyle/>{paragraphs}</p:txBody></p:sp>"#,
        escape_xml(name)
    )
}

fn text_paragraphs(
    text: &str,
    font_size: usize,
    bold: bool,
    color: &str,
    alignment: &str,
) -> String {
    if text.is_empty() {
        return format!(
            "<a:p><a:pPr algn=\"{}\"/><a:endParaRPr lang=\"zh-CN\"/></a:p>",
            alignment_code(alignment)
        );
    }
    text.lines()
        .map(|line| {
            let (bullet, line) = line
                .strip_prefix("- ")
                .or_else(|| line.strip_prefix("* "))
                .map_or((false, line), |line| (true, line));
            let paragraph = if bullet {
                format!("<a:pPr algn=\"{}\" marL=\"342900\" indent=\"-285750\"><a:buChar char=\"•\"/></a:pPr>", alignment_code(alignment))
            } else {
                format!("<a:pPr algn=\"{}\"/>", alignment_code(alignment))
            };
            format!(
                "<a:p>{paragraph}<a:r><a:rPr lang=\"zh-CN\" sz=\"{font_size}\" b=\"{}\"><a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill></a:rPr><a:t xml:space=\"preserve\">{}</a:t></a:r><a:endParaRPr lang=\"zh-CN\"/></a:p>",
                usize::from(bold),
                escape_xml(line)
            )
        })
        .collect()
}

fn alignment_code(value: &str) -> &str {
    match value {
        "center" => "ctr",
        "right" => "r",
        _ => "l",
    }
}

#[allow(clippy::too_many_arguments)]
fn picture_shape(
    id: usize,
    relationship_id: &str,
    image: &PresentationImage,
    box_x: i64,
    box_y: i64,
    box_cx: i64,
    box_cy: i64,
) -> String {
    let (x, y, cx, cy, crop) = fitted_image_box(image, box_x, box_y, box_cx, box_cy);
    format!(
        r#"<p:pic><p:nvPicPr><p:cNvPr id="{id}" name="{}" descr="{}"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="{relationship_id}"/>{crop}<a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:ln><a:noFill/></a:ln></p:spPr></p:pic>"#,
        escape_xml(
            Path::new(image.source_path.as_str())
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("Presentation image")
        ),
        escape_xml(image.alt_text.as_str())
    )
}

fn fitted_image_box(
    image: &PresentationImage,
    box_x: i64,
    box_y: i64,
    box_cx: i64,
    box_cy: i64,
) -> (i64, i64, i64, i64, String) {
    let image_ratio = f64::from(image.width) / f64::from(image.height);
    let box_ratio = box_cx as f64 / box_cy as f64;
    match image.fit {
        ImageFit::Contain => {
            if image_ratio > box_ratio {
                let cy = (box_cx as f64 / image_ratio).round() as i64;
                (box_x, box_y + (box_cy - cy) / 2, box_cx, cy, String::new())
            } else {
                let cx = (box_cy as f64 * image_ratio).round() as i64;
                (box_x + (box_cx - cx) / 2, box_y, cx, box_cy, String::new())
            }
        }
        ImageFit::Cover => {
            let (left, right, top, bottom) = if image_ratio > box_ratio {
                let visible = box_ratio / image_ratio;
                let crop = (((1.0 - visible) / 2.0) * 100_000.0).round() as i64;
                (crop, crop, 0, 0)
            } else {
                let visible = image_ratio / box_ratio;
                let crop = (((1.0 - visible) / 2.0) * 100_000.0).round() as i64;
                (0, 0, crop, crop)
            };
            (
                box_x,
                box_y,
                box_cx,
                box_cy,
                format!("<a:srcRect l=\"{left}\" r=\"{right}\" t=\"{top}\" b=\"{bottom}\"/>"),
            )
        }
    }
}

fn slide_relationships(
    image: Option<(usize, PresentationImageFormat)>,
    chart_number: Option<usize>,
    notes_number: Option<usize>,
) -> String {
    let mut relationships = String::from(
        r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>"#,
    );
    let mut next_id = 2usize;
    if let Some((number, format)) = image {
        relationships.push_str(
            format!("<Relationship Id=\"rId{next_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/image{number}.{}\"/>", format.extension()).as_str(),
        );
        next_id += 1;
    }
    if let Some(chart_number) = chart_number {
        relationships.push_str(
            format!("<Relationship Id=\"rId{next_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart\" Target=\"../charts/chart{chart_number}.xml\"/>").as_str(),
        );
        next_id += 1;
    }
    if let Some(notes_number) = notes_number {
        relationships.push_str(
            format!("<Relationship Id=\"rId{next_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide\" Target=\"../notesSlides/notesSlide{notes_number}.xml\"/>").as_str(),
        );
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{relationships}</Relationships>"#
    )
}

fn group_shape() -> String {
    r#"<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>"#.to_string()
}

fn slide_master_xml() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree>{}</p:spTree></p:cSld><p:clrMap accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" bg1="lt1" bg2="lt2" folHlink="folHlink" hlink="hlink" tx1="dk1" tx2="dk2"/><p:sldLayoutIdLst><p:sldLayoutId id="1" r:id="rId1"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>"#,
        group_shape()
    )
}

fn slide_master_relationships() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/></Relationships>"#.to_string()
}

fn slide_layout_xml() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="blank" preserve="1"><p:cSld name="Blank"><p:spTree>{}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#,
        group_shape()
    )
}

fn slide_layout_relationships() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#.to_string()
}

fn notes_master_xml() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:notesMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld name="ChatOS Notes"><p:spTree>{}</p:spTree></p:cSld><p:clrMap accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" bg1="lt1" bg2="lt2" folHlink="folHlink" hlink="hlink" tx1="dk1" tx2="dk2"/><p:hf dt="1" hdr="1" ftr="1" sldNum="1"/><p:notesStyle/></p:notesMaster>"#,
        group_shape()
    )
}

fn notes_master_relationships() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/></Relationships>"#.to_string()
}

fn notes_slide_xml(notes: &str, slide_number: usize) -> Result<String> {
    validate_slide_text(notes, "notes", MAX_SLIDE_TEXT_CHARS)?;
    let shape = text_shape(
        2,
        "Speaker Notes",
        685_800,
        1_371_600,
        5_486_400,
        6_858_000,
        1_400,
        notes,
        false,
        "1F2937",
        "left",
        None,
    )
    .replace(
        "<p:nvPr/>",
        "<p:nvPr><p:ph type=\"body\" idx=\"1\"/></p:nvPr>",
    );
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:notes xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld name="Slide {slide_number} Notes"><p:spTree>{}{shape}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>"#,
        group_shape()
    ))
}

fn notes_slide_relationships(slide_number: usize) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster" Target="../notesMasters/notesMaster1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="../slides/slide{slide_number}.xml"/></Relationships>"#
    )
}

fn theme_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="ChatOS"><a:themeElements><a:clrScheme name="ChatOS"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="1F2937"/></a:dk2><a:lt2><a:srgbClr val="F3F4F6"/></a:lt2><a:accent1><a:srgbClr val="2563EB"/></a:accent1><a:accent2><a:srgbClr val="0F766E"/></a:accent2><a:accent3><a:srgbClr val="7C3AED"/></a:accent3><a:accent4><a:srgbClr val="EA580C"/></a:accent4><a:accent5><a:srgbClr val="DB2777"/></a:accent5><a:accent6><a:srgbClr val="4B5563"/></a:accent6><a:hlink><a:srgbClr val="0000FF"/></a:hlink><a:folHlink><a:srgbClr val="800080"/></a:folHlink></a:clrScheme><a:fontScheme name="ChatOS"><a:majorFont><a:latin typeface="Aptos Display"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Aptos"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="ChatOS"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="9525"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#.to_string()
}

fn presentation_slide_metadata(xml: &str) -> Result<PresentationSlideMetadata> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut slide_ids = Vec::new();
    let mut used_slide_ids = HashSet::new();
    let mut relationship_ids = Vec::new();
    let mut used_relationship_ids = HashSet::new();
    let mut max_slide_id = 0u32;
    let mut slide_tag_name = None;
    let mut relationship_attribute_name = None;
    let mut slide_list_count = 0usize;
    loop {
        match reader
            .read_event()
            .context("parse PPTX presentation slide list")?
        {
            Event::Start(event) if event.local_name().as_ref() == b"sldIdLst" => {
                slide_list_count = slide_list_count.saturating_add(1);
            }
            Event::Empty(event) if event.local_name().as_ref() == b"sldIdLst" => {
                return Err(anyhow!("PPTX presentation slide list is empty"));
            }
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"sldId" =>
            {
                let mut numeric_id = None;
                let mut relationship_id = None;
                let mut relationship_key = None;
                for attribute in event.attributes().with_checks(false) {
                    let attribute = attribute.context("parse PPTX slide id attribute")?;
                    let key = attribute.key.as_ref();
                    let value = attribute
                        .decoded_and_normalized_value(XmlVersion::Explicit1_0, reader.decoder())?
                        .into_owned();
                    if key == b"id" {
                        numeric_id = Some(value.parse::<u32>().context("parse PPTX slide id")?);
                    } else if key.ends_with(b":id") {
                        relationship_id = Some(value);
                        relationship_key = Some(String::from_utf8_lossy(key).into_owned());
                    }
                }
                let numeric_id = numeric_id
                    .ok_or_else(|| anyhow!("PPTX slide id is missing numeric id attribute"))?;
                let relationship_id = relationship_id
                    .ok_or_else(|| anyhow!("PPTX slide id is missing relationship id attribute"))?;
                if !used_slide_ids.insert(numeric_id) {
                    return Err(anyhow!(
                        "PPTX presentation contains duplicate numeric slide ids"
                    ));
                }
                if !used_relationship_ids.insert(relationship_id.clone()) {
                    return Err(anyhow!(
                        "PPTX presentation contains duplicate slide relationship ids"
                    ));
                }
                max_slide_id = max_slide_id.max(numeric_id);
                slide_ids.push(numeric_id);
                relationship_ids.push(relationship_id);
                let current_tag = String::from_utf8_lossy(event.name().as_ref()).into_owned();
                if slide_tag_name
                    .as_ref()
                    .is_some_and(|existing| existing != &current_tag)
                {
                    return Err(anyhow!("PPTX presentation mixes slide id namespaces"));
                }
                slide_tag_name = Some(current_tag);
                let relationship_key = relationship_key.expect("relationship key validated");
                if relationship_attribute_name
                    .as_ref()
                    .is_some_and(|existing| existing != &relationship_key)
                {
                    return Err(anyhow!(
                        "PPTX presentation mixes relationship attribute namespaces"
                    ));
                }
                relationship_attribute_name = Some(relationship_key);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if slide_list_count != 1 || relationship_ids.is_empty() {
        return Err(anyhow!(
            "PPTX presentation must contain exactly one non-empty slide list"
        ));
    }
    Ok(PresentationSlideMetadata {
        slide_ids,
        relationship_ids,
        max_slide_id,
        slide_tag_name: slide_tag_name.expect("non-empty slide list has tag name"),
        relationship_attribute_name: relationship_attribute_name
            .expect("non-empty slide list has relationship attribute"),
    })
}

fn ensure_all_slide_parts_are_referenced(
    ordered_slide_paths: &[String],
    names: &HashSet<String>,
) -> Result<()> {
    let package_slide_paths = names
        .iter()
        .filter(|name| slide_part_number(name.as_str()).is_some())
        .cloned()
        .collect::<HashSet<_>>();
    let referenced_slide_paths = ordered_slide_paths.iter().cloned().collect::<HashSet<_>>();
    if referenced_slide_paths != package_slide_paths {
        return Err(anyhow!(
            "PPTX contains unreferenced or missing slide parts; conservative editing was refused"
        ));
    }
    Ok(())
}

fn required_slide_order(arguments: &Value, slide_count: usize) -> Result<Vec<usize>> {
    let values = arguments
        .get("slide_order")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("slide_order must be an array of positive integers"))?;
    if values.len() != slide_count || values.len() > MAX_PPTX_SLIDES {
        return Err(anyhow!(
            "slide_order must contain every current slide position exactly once"
        ));
    }
    let mut seen = HashSet::with_capacity(values.len());
    let mut order = Vec::with_capacity(values.len());
    for value in values {
        let position = value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value >= 1 && *value <= slide_count)
            .ok_or_else(|| anyhow!("slide_order contains an out-of-range slide position"))?;
        if !seen.insert(position) {
            return Err(anyhow!("slide_order must not contain duplicates"));
        }
        order.push(position);
    }
    Ok(order)
}

fn required_deleted_slide_positions(arguments: &Value, slide_count: usize) -> Result<Vec<usize>> {
    let values = arguments
        .get("slide_numbers")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("slide_numbers must be an array of positive integers"))?;
    if values.is_empty() || values.len() >= slide_count || values.len() > MAX_PPTX_SLIDES {
        return Err(anyhow!(
            "slide_numbers must delete at least one slide while leaving at least one slide"
        ));
    }
    let mut positions = BTreeSet::new();
    for value in values {
        let position = value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value >= 1 && *value <= slide_count)
            .ok_or_else(|| anyhow!("slide_numbers contains an out-of-range slide number"))?;
        if !positions.insert(position) {
            return Err(anyhow!("slide_numbers must not contain duplicates"));
        }
    }
    Ok(positions.into_iter().collect())
}

fn ensure_presentation_slide_relationships_are_exact(
    metadata: &PresentationSlideMetadata,
    relationships: &RelationshipDocument,
) -> Result<()> {
    let expected = metadata
        .relationship_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut actual = HashSet::new();
    for relationship in relationships
        .relationships
        .iter()
        .filter(|relationship| relationship.relationship_type.ends_with("/slide"))
    {
        if relationship.external || !actual.insert(relationship.id.as_str()) {
            return Err(anyhow!(
                "PPTX presentation contains ambiguous or external slide relationships"
            ));
        }
    }
    if actual != expected {
        return Err(anyhow!(
            "PPTX presentation slide relationships do not exactly match the visible slide list"
        ));
    }
    Ok(())
}

fn reject_unsupported_slide_deletion_references(xml: &str) -> Result<()> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut in_slide_list = false;
    loop {
        match reader
            .read_event()
            .context("inspect PPTX slide deletion references")?
        {
            Event::Start(event) if event.local_name().as_ref() == b"sldIdLst" => {
                if in_slide_list {
                    return Err(anyhow!("PPTX presentation contains nested slide lists"));
                }
                in_slide_list = true;
            }
            Event::End(event) if event.local_name().as_ref() == b"sldIdLst" => {
                if !in_slide_list {
                    return Err(anyhow!(
                        "PPTX presentation contains an unmatched slide list"
                    ));
                }
                in_slide_list = false;
            }
            Event::Start(event) | Event::Empty(event)
                if matches!(
                    event.local_name().as_ref(),
                    b"custShowLst" | b"custShow" | b"sectionLst" | b"section"
                ) =>
            {
                return Err(anyhow!(
                    "PPTX custom shows or presentation sections make slide deletion ambiguous"
                ));
            }
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"sldId" && !in_slide_list =>
            {
                return Err(anyhow!(
                    "PPTX contains slide-id references outside the visible slide list"
                ));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if in_slide_list {
        return Err(anyhow!("PPTX presentation slide list is not closed"));
    }
    Ok(())
}

fn ordered_presentation_slide_paths(
    metadata: &PresentationSlideMetadata,
    relationships: &RelationshipDocument,
    names: &HashSet<String>,
) -> Result<Vec<String>> {
    let relationships_by_id = relationships
        .relationships
        .iter()
        .map(|relationship| (relationship.id.as_str(), relationship))
        .collect::<HashMap<_, _>>();
    let mut ordered = Vec::with_capacity(metadata.relationship_ids.len());
    let mut referenced = HashSet::new();
    for relationship_id in &metadata.relationship_ids {
        let relationship = relationships_by_id
            .get(relationship_id.as_str())
            .ok_or_else(|| {
                anyhow!("PPTX presentation references a missing relationship: {relationship_id}")
            })?;
        if relationship.external || !relationship.relationship_type.ends_with("/slide") {
            return Err(anyhow!(
                "PPTX presentation slide relationship is external or has an unexpected type"
            ));
        }
        let path = resolve_part_target("ppt/presentation.xml", relationship.target.as_str())?;
        if !names.contains(path.as_str()) || !referenced.insert(path.clone()) {
            return Err(anyhow!(
                "PPTX presentation contains a missing or duplicate slide reference"
            ));
        }
        ordered.push(path);
    }
    Ok(ordered)
}

fn owned_notes_parts_by_slide(
    archive: &mut ZipArchive<File>,
    names: &HashSet<String>,
    ordered_slide_paths: &[String],
) -> Result<Vec<Option<OwnedNotesPart>>> {
    let mut notes_by_slide = Vec::with_capacity(ordered_slide_paths.len());
    let mut notes_owners = HashMap::<String, String>::new();
    for slide_path in ordered_slide_paths {
        let slide_relationships_path = relationships_part_path(slide_path.as_str())?;
        if !names.contains(slide_relationships_path.as_str()) {
            notes_by_slide.push(None);
            continue;
        }
        let slide_relationships_xml = read_zip_text(archive, slide_relationships_path.as_str())?;
        let slide_relationships =
            parse_relationship_document(slide_relationships_xml.as_str(), slide_path.as_str())?;
        let notes_relationships = slide_relationships
            .relationships
            .iter()
            .filter(|relationship| relationship.relationship_type.ends_with("/notesSlide"))
            .collect::<Vec<_>>();
        if notes_relationships.len() > 1
            || notes_relationships
                .first()
                .is_some_and(|item| item.external)
        {
            return Err(anyhow!(
                "PPTX slide contains ambiguous or external speaker-note relationships"
            ));
        }
        let Some(notes_relationship) = notes_relationships.first() else {
            notes_by_slide.push(None);
            continue;
        };
        let notes_path =
            resolve_part_target(slide_path.as_str(), notes_relationship.target.as_str())?;
        if !names.contains(notes_path.as_str()) {
            return Err(anyhow!(
                "PPTX is missing referenced notes part: {notes_path}"
            ));
        }
        if notes_owners
            .insert(notes_path.clone(), slide_path.clone())
            .is_some()
        {
            return Err(anyhow!(
                "PPTX speaker-note part is shared by multiple slides; conservative editing was refused"
            ));
        }
        let notes_relationships_path = relationships_part_path(notes_path.as_str())?;
        if !names.contains(notes_relationships_path.as_str()) {
            return Err(anyhow!(
                "PPTX notes part is missing its relationship part: {notes_relationships_path}"
            ));
        }
        let notes_relationships_xml = read_zip_text(archive, notes_relationships_path.as_str())?;
        let notes_part_relationships =
            parse_relationship_document(notes_relationships_xml.as_str(), notes_path.as_str())?;
        let slide_back_references = notes_part_relationships
            .relationships
            .iter()
            .filter(|relationship| relationship.relationship_type.ends_with("/slide"))
            .collect::<Vec<_>>();
        if slide_back_references.len() != 1 || slide_back_references[0].external {
            return Err(anyhow!(
                "PPTX notes part must contain exactly one internal owning-slide relationship"
            ));
        }
        let owner_path = resolve_part_target(
            notes_path.as_str(),
            slide_back_references[0].target.as_str(),
        )?;
        if owner_path != *slide_path {
            return Err(anyhow!(
                "PPTX notes part owning-slide relationship does not match its slide"
            ));
        }
        notes_by_slide.push(Some(OwnedNotesPart {
            path: notes_path,
            relationships_path: notes_relationships_path,
        }));
    }
    Ok(notes_by_slide)
}

fn selected_slide_positions(arguments: &Value, slide_count: usize) -> Result<Vec<usize>> {
    let Some(value) = arguments.get("slide_numbers") else {
        return Ok((1..=slide_count).collect());
    };
    let values = value
        .as_array()
        .ok_or_else(|| anyhow!("slide_numbers must be an array of positive integers"))?;
    if values.is_empty() || values.len() > MAX_PPTX_SLIDES {
        return Err(anyhow!(
            "slide_numbers must contain between 1 and {MAX_PPTX_SLIDES} items"
        ));
    }
    let mut positions = BTreeSet::new();
    for value in values {
        let position = value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value >= 1 && *value <= slide_count)
            .ok_or_else(|| anyhow!("slide_numbers contains an out-of-range slide number"))?;
        if !positions.insert(position) {
            return Err(anyhow!("slide_numbers must not contain duplicates"));
        }
    }
    Ok(positions.into_iter().collect())
}

fn replace_drawing_text_runs(
    xml: &str,
    find: &str,
    replacement: &str,
    max_replacements: usize,
) -> Result<(String, usize, bool)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(xml.len()));
    let mut in_drawing_text = false;
    let mut drawing_text_value = String::new();
    let mut drawing_text_events = Vec::<Event<'static>>::new();
    let mut replacements = 0usize;
    let mut limit_reached = false;
    loop {
        let event = reader.read_event().context("rewrite PPTX DrawingML text")?;
        match event {
            Event::Start(start) if start.name().as_ref() == b"a:t" => {
                if in_drawing_text {
                    return Err(anyhow!("PPTX contains nested DrawingML text elements"));
                }
                in_drawing_text = true;
                drawing_text_value.clear();
                drawing_text_events.clear();
                writer.write_event(Event::Start(start.into_owned()))?;
            }
            Event::End(end) if end.name().as_ref() == b"a:t" => {
                if !in_drawing_text {
                    return Err(anyhow!("PPTX contains an unmatched DrawingML text end tag"));
                }
                let occurrences = drawing_text_value.matches(find).count();
                let allowed = occurrences.min(max_replacements.saturating_sub(replacements));
                if allowed > 0 {
                    let updated = drawing_text_value.replacen(find, replacement, allowed);
                    writer.write_event(Event::Text(BytesText::new(updated.as_str())))?;
                    replacements = replacements.saturating_add(allowed);
                } else {
                    for event in drawing_text_events.drain(..) {
                        writer.write_event(event)?;
                    }
                }
                if occurrences > allowed {
                    limit_reached = true;
                }
                in_drawing_text = false;
                writer.write_event(Event::End(end.into_owned()))?;
            }
            Event::Text(text) if in_drawing_text => {
                let decoded = text
                    .xml_content(XmlVersion::Explicit1_0)
                    .context("decode PPTX DrawingML text")?;
                let value = unescape(decoded.as_ref())
                    .context("unescape PPTX DrawingML text")?
                    .into_owned();
                drawing_text_value.push_str(value.as_str());
                drawing_text_events.push(Event::Text(text.into_owned()));
            }
            Event::GeneralRef(reference) if in_drawing_text => {
                if let Some(character) = reference
                    .resolve_char_ref()
                    .context("resolve PPTX DrawingML character reference")?
                {
                    drawing_text_value.push(character);
                } else {
                    let entity = reference
                        .decode()
                        .context("decode PPTX DrawingML entity reference")?;
                    let value = resolve_xml_entity(entity.as_ref()).ok_or_else(|| {
                        anyhow!("PPTX DrawingML text contains an unsupported entity reference")
                    })?;
                    drawing_text_value.push_str(value);
                }
                drawing_text_events.push(Event::GeneralRef(reference.into_owned()));
            }
            Event::CData(cdata) if in_drawing_text => {
                let value = cdata
                    .xml_content(XmlVersion::Explicit1_0)
                    .context("decode PPTX DrawingML CDATA")?;
                drawing_text_value.push_str(value.as_ref());
                drawing_text_events.push(Event::CData(cdata.into_owned()));
            }
            _event if in_drawing_text => {
                return Err(anyhow!(
                    "PPTX DrawingML text run contains unsupported nested XML content"
                ));
            }
            Event::Eof => {
                if in_drawing_text {
                    return Err(anyhow!("PPTX contains an unclosed DrawingML text element"));
                }
                writer.write_event(Event::Eof)?;
                break;
            }
            event => writer.write_event(event.into_owned())?,
        }
    }
    Ok((
        xml_output(writer, "updated PPTX slide XML")?,
        replacements,
        limit_reached,
    ))
}

fn required_pptx_index(arguments: &Value, key: &str, maximum: usize) -> Result<usize> {
    arguments
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| (1..=maximum).contains(value))
        .ok_or_else(|| anyhow!("{key} must be an integer between 1 and {maximum}"))
}

fn selected_pptx_table(
    source: &Path,
    slide_number: usize,
    table_number: usize,
) -> Result<(String, String, PptxTableScan)> {
    let names = validate_pptx_package(source)?;
    for required in ["ppt/presentation.xml", "ppt/_rels/presentation.xml.rels"] {
        if !names.contains(required) {
            return Err(anyhow!("PPTX is missing required package part: {required}"));
        }
    }
    let mut archive = ZipArchive::new(File::open(source)?)
        .with_context(|| format!("open PPTX {}", source.display()))?;
    let presentation_xml = read_zip_text(&mut archive, "ppt/presentation.xml")?;
    let presentation_relationships_xml =
        read_zip_text(&mut archive, "ppt/_rels/presentation.xml.rels")?;
    let slide_metadata = presentation_slide_metadata(presentation_xml.as_str())?;
    let presentation_relationships = parse_relationship_document(
        presentation_relationships_xml.as_str(),
        "ppt/presentation.xml",
    )?;
    let ordered_slide_paths =
        ordered_presentation_slide_paths(&slide_metadata, &presentation_relationships, &names)?;
    let slide_path = ordered_slide_paths.get(slide_number - 1).ok_or_else(|| {
        anyhow!(
            "slide_number {slide_number} is out-of-range for a PPTX with {} visible slides",
            ordered_slide_paths.len()
        )
    })?;
    let slide_xml = read_zip_text(&mut archive, slide_path.as_str())?;
    let mut tables = scan_pptx_tables(slide_xml.as_str())?;
    if table_number == 0 || table_number > tables.len() {
        return Err(anyhow!(
            "table_number {table_number} is out-of-range for visible slide {slide_number}, which contains {} tables",
            tables.len()
        ));
    }
    let table = tables.remove(table_number - 1);
    Ok((slide_path.clone(), slide_xml, table))
}

fn required_pptx_sha256(arguments: &Value, key: &str) -> Result<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .map(str::to_string)
        .ok_or_else(|| {
            anyhow!("{key} must be one lowercase SHA-256 value returned by inspect_pptx_table")
        })
}

fn required_pptx_chart_sha256(arguments: &Value) -> Result<String> {
    arguments
        .get("expected_chart_xml_sha256")
        .and_then(Value::as_str)
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .map(str::to_string)
        .ok_or_else(|| {
            anyhow!(
                "expected_chart_xml_sha256 must be one lowercase SHA-256 value returned by inspect_pptx_charts"
            )
        })
}

fn pptx_table_cell_xml_sha256(xml: &str, cell: &SimplePptxTableCell) -> String {
    hex::encode(Sha256::digest(
        &xml.as_bytes()[cell.range.start..cell.range.end],
    ))
}

fn simple_pptx_table_cell_xml_sha256(xml: &str, table: &SimplePptxTable) -> Vec<Vec<String>> {
    (1..=table.rows)
        .map(|row| {
            table
                .cells
                .iter()
                .filter(|cell| cell.row == row)
                .map(|cell| pptx_table_cell_xml_sha256(xml, cell))
                .collect()
        })
        .collect()
}

fn ensure_pptx_table_cell_xml_sha256(
    xml: &str,
    cell: &SimplePptxTableCell,
    expected: &str,
    label: &str,
) -> Result<()> {
    if pptx_table_cell_xml_sha256(xml, cell) != expected {
        return Err(anyhow!(
            "{label} PPTX table cell XML does not match the inspected SHA-256 snapshot"
        ));
    }
    Ok(())
}

fn required_pptx_table_row_cells(
    arguments: &Value,
    key: &str,
    columns: usize,
) -> Result<Vec<String>> {
    let values = arguments
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("{key} must be an array of complete cell strings"))?;
    if values.len() != columns {
        return Err(anyhow!(
            "{key} must contain exactly {columns} cell strings for the selected PPTX table"
        ));
    }
    let mut output = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let value = value
            .as_str()
            .ok_or_else(|| anyhow!("{key} cell {} must be a string", index + 1))?;
        validate_slide_text(
            value,
            format!("{key} cell {}", index + 1).as_str(),
            MAX_PPTX_TABLE_CELL_TEXT_CHARS,
        )?;
        output.push(value.to_string());
    }
    Ok(output)
}

fn ensure_expected_pptx_table_row(
    table: &SimplePptxTable,
    row: usize,
    expected_cells: &[String],
) -> Result<()> {
    let actual = table
        .cells
        .iter()
        .filter(|cell| cell.row == row)
        .map(|cell| cell.decoded.as_str())
        .collect::<Vec<_>>();
    if actual.len() != expected_cells.len()
        || !actual
            .iter()
            .zip(expected_cells)
            .all(|(actual, expected)| *actual == expected)
    {
        return Err(anyhow!(
            "selected PPTX table row does not match expected_cells"
        ));
    }
    Ok(())
}

fn required_pptx_table_column_cells(
    arguments: &Value,
    key: &str,
    rows: usize,
) -> Result<Vec<String>> {
    let values = arguments
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("{key} must be an array of complete cell strings"))?;
    if values.len() != rows {
        return Err(anyhow!(
            "{key} must contain exactly {rows} cell strings for the selected PPTX table"
        ));
    }
    let mut output = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let value = value
            .as_str()
            .ok_or_else(|| anyhow!("{key} cell {} must be a string", index + 1))?;
        validate_slide_text(
            value,
            format!("{key} cell {}", index + 1).as_str(),
            MAX_PPTX_TABLE_CELL_TEXT_CHARS,
        )?;
        output.push(value.to_string());
    }
    Ok(output)
}

fn ensure_expected_pptx_table_column(
    table: &SimplePptxTable,
    column: usize,
    expected_cells: &[String],
) -> Result<()> {
    let actual = table
        .cells
        .iter()
        .filter(|cell| cell.column == column)
        .map(|cell| cell.decoded.as_str())
        .collect::<Vec<_>>();
    if actual.len() != expected_cells.len()
        || !actual
            .iter()
            .zip(expected_cells)
            .all(|(actual, expected)| *actual == expected)
    {
        return Err(anyhow!(
            "selected PPTX table column does not match expected_cells"
        ));
    }
    Ok(())
}

fn scan_pptx_tables(xml: &str) -> Result<Vec<PptxTableScan>> {
    let ranges = pptx_xml_element_ranges(
        xml,
        "<a:tbl",
        "</a:tbl>",
        MAX_PPTX_TABLES_PER_SLIDE,
        "PPTX slide tables",
    )?;
    ranges
        .into_iter()
        .map(|range| scan_pptx_table(xml, range))
        .collect()
}

fn scan_pptx_table(xml: &str, range: PptxXmlElementRange) -> Result<PptxTableScan> {
    let table_xml = &xml[range.start..range.end];
    let row_ranges = pptx_xml_element_ranges(
        table_xml,
        "<a:tr",
        "</a:tr>",
        MAX_PPTX_TABLE_ROWS,
        "PPTX table rows",
    )?;
    let mut rows = Vec::with_capacity(row_ranges.len());
    let mut cells = 0usize;
    let mut columns = 0usize;
    let mut cell_text_truncated = false;
    for row in &row_ranges {
        let row_xml = &table_xml[row.start..row.end];
        let cell_ranges = pptx_xml_element_ranges(
            row_xml,
            "<a:tc",
            "</a:tc>",
            MAX_PPTX_TABLE_COLUMNS,
            "PPTX table row cells",
        )?;
        cells = cells.saturating_add(cell_ranges.len());
        if cells > MAX_PPTX_TABLE_CELLS {
            return Err(anyhow!(
                "PPTX table cells exceed the {MAX_PPTX_TABLE_CELLS} cell safety limit"
            ));
        }
        columns = columns.max(cell_ranges.len());
        let mut row_text = Vec::with_capacity(cell_ranges.len());
        for cell in cell_ranges {
            let cell_xml = &row_xml[cell.start..cell.end];
            let preview = drawing_text_runs(cell_xml, MAX_PPTX_TABLE_PREVIEW_CHARS + 1)?.join("");
            let truncated = preview.chars().count() > MAX_PPTX_TABLE_PREVIEW_CHARS;
            cell_text_truncated |= truncated;
            row_text.push(
                preview
                    .chars()
                    .take(MAX_PPTX_TABLE_PREVIEW_CHARS)
                    .collect::<String>(),
            );
        }
        rows.push(row_text);
    }
    let (simple, unsupported_reason) = match simple_pptx_table(xml, range) {
        Ok(table) => (Some(table), None),
        Err(error) => (None, Some(error.to_string())),
    };
    Ok(PptxTableScan {
        rows: row_ranges.len(),
        columns,
        cells,
        cell_text: rows,
        cell_text_truncated,
        simple,
        unsupported_reason,
    })
}

fn simple_pptx_table(xml: &str, range: PptxXmlElementRange) -> Result<SimplePptxTable> {
    if xml.contains("<!--") || xml.contains("<![CDATA[") || xml.contains("<!DOCTYPE") {
        return Err(anyhow!(
            "simple PPTX table editing does not support comments, CDATA, or DTD markup"
        ));
    }
    let without_declaration = xml
        .trim_start()
        .strip_prefix("<?xml")
        .and_then(|value| value.find("?>").map(|end| &value[end + 2..]))
        .unwrap_or(xml);
    if without_declaration.contains("<?") {
        return Err(anyhow!(
            "simple PPTX table editing does not support processing instructions"
        ));
    }
    let table_xml = &xml[range.start..range.end];
    if &table_xml[..range.open_end - range.start] != "<a:tbl>" {
        return Err(anyhow!(
            "simple PPTX table editing requires a standard a:tbl opening tag"
        ));
    }
    if find_next_pptx_xml_tag_start(
        table_xml,
        "<a:tbl",
        range.open_end.saturating_sub(range.start),
    )
    .is_some_and(|start| start < range.close_start.saturating_sub(range.start))
    {
        return Err(anyhow!("nested DrawingML tables are not supported"));
    }
    let stack = pptx_xml_open_element_stack_at(xml, range.start)?;
    if stack.last().map(String::as_str) != Some("a:graphicData") {
        return Err(anyhow!(
            "DrawingML table is not a direct child of a:graphicData"
        ));
    }
    let graphic_data_start = xml[..range.start]
        .rfind("<a:graphicData")
        .ok_or_else(|| anyhow!("DrawingML table is missing its graphicData parent"))?;
    let graphic_data_open_end = pptx_xml_tag_end(xml, graphic_data_start, range.start)?;
    let graphic_data_opening = &xml[graphic_data_start..graphic_data_open_end];
    let uri = pptx_opening_attribute(graphic_data_opening, "graphicData", "uri")?;
    if uri.as_deref() != Some("http://schemas.openxmlformats.org/drawingml/2006/table") {
        return Err(anyhow!(
            "DrawingML table graphicData has a nonstandard table URI"
        ));
    }

    let table_children = pptx_direct_child_local_names(table_xml, "tbl")?;
    if table_children.len() < 3
        || table_children[0] != "tblPr"
        || table_children[1] != "tblGrid"
        || table_children[2..].iter().any(|name| name != "tr")
    {
        return Err(anyhow!(
            "simple PPTX table requires one tblPr, one tblGrid, and direct rows in canonical order"
        ));
    }

    let grid_ranges = pptx_direct_element_ranges(
        table_xml,
        "<a:tblGrid",
        "</a:tblGrid>",
        1,
        "a:tbl",
        "PPTX table grids",
    )?;
    if grid_ranges.len() != 1 || grid_ranges[0].open_end == grid_ranges[0].end {
        return Err(anyhow!(
            "simple PPTX table requires one non-empty direct tblGrid"
        ));
    }
    let grid_xml = &table_xml[grid_ranges[0].start..grid_ranges[0].end];
    let grid_children = pptx_direct_child_local_names(grid_xml, "tblGrid")?;
    if grid_children.is_empty() || grid_children.iter().any(|name| name != "gridCol") {
        return Err(anyhow!(
            "simple PPTX table grid must contain only gridCol children"
        ));
    }
    let grid_columns = grid_children.len();
    if grid_columns > MAX_PPTX_TABLE_COLUMNS {
        return Err(anyhow!(
            "PPTX table columns exceed the {MAX_PPTX_TABLE_COLUMNS} column safety limit"
        ));
    }

    let row_ranges = pptx_direct_element_ranges(
        table_xml,
        "<a:tr",
        "</a:tr>",
        MAX_PPTX_TABLE_ROWS,
        "a:tbl",
        "PPTX table rows",
    )?;
    if row_ranges.is_empty() {
        return Err(anyhow!("simple PPTX table contains no rows"));
    }
    let mut cells = Vec::new();
    let mut total_text_chars = 0usize;
    for (row_index, row_range) in row_ranges.iter().enumerate() {
        let row_xml = &table_xml[row_range.start..row_range.end];
        let row_children = pptx_direct_child_local_names(row_xml, "tr")?;
        if row_children.is_empty() || row_children.iter().any(|name| name != "tc") {
            return Err(anyhow!(
                "simple PPTX table rows must contain only direct table cells"
            ));
        }
        if row_children.len() != grid_columns {
            return Err(anyhow!(
                "simple PPTX table must be rectangular and match its table grid"
            ));
        }
        let cell_ranges = pptx_direct_element_ranges(
            row_xml,
            "<a:tc",
            "</a:tc>",
            MAX_PPTX_TABLE_COLUMNS,
            "a:tr",
            "PPTX table row cells",
        )?;
        if cell_ranges.len() != grid_columns {
            return Err(anyhow!(
                "simple PPTX table must be rectangular and match its table grid"
            ));
        }
        for (column_index, cell_range) in cell_ranges.iter().enumerate() {
            let cell_xml = &row_xml[cell_range.start..cell_range.end];
            if &cell_xml[..cell_range.open_end - cell_range.start] != "<a:tc>" {
                return Err(anyhow!(
                    "merged or attributed PPTX table cells are not supported"
                ));
            }
            let cell_children = pptx_direct_child_local_names(cell_xml, "tc")?;
            let standard_cell_children = (cell_children.len() == 1 && cell_children[0] == "txBody")
                || (cell_children.len() == 2
                    && cell_children[0] == "txBody"
                    && cell_children[1] == "tcPr");
            if !standard_cell_children {
                return Err(anyhow!(
                    "simple PPTX table cells require one direct txBody followed by optional tcPr"
                ));
            }
            let text_body_ranges = pptx_direct_element_ranges(
                cell_xml,
                "<a:txBody",
                "</a:txBody>",
                1,
                "a:tc",
                "PPTX table cell text bodies",
            )?;
            if text_body_ranges.len() != 1
                || text_body_ranges[0].open_end == text_body_ranges[0].end
            {
                return Err(anyhow!(
                    "simple PPTX table cell requires one non-empty text body"
                ));
            }
            let text_body = text_body_ranges[0];
            let text_body_xml = &cell_xml[text_body.start..text_body.end];
            let body_children = pptx_direct_child_local_names(text_body_xml, "txBody")?;
            let mut body_cursor = 0usize;
            if body_children.get(body_cursor).map(String::as_str) == Some("bodyPr") {
                body_cursor += 1;
            }
            if body_children.get(body_cursor).map(String::as_str) == Some("lstStyle") {
                body_cursor += 1;
            }
            if body_children.get(body_cursor).map(String::as_str) != Some("p")
                || body_cursor + 1 != body_children.len()
            {
                return Err(anyhow!(
                    "simple PPTX table cell text body requires optional bodyPr/lstStyle followed by exactly one paragraph"
                ));
            }
            let paragraph_ranges = pptx_direct_element_ranges(
                text_body_xml,
                "<a:p",
                "</a:p>",
                1,
                "a:txBody",
                "PPTX table cell paragraphs",
            )?;
            if paragraph_ranges.len() != 1
                || paragraph_ranges[0].open_end == paragraph_ranges[0].end
            {
                return Err(anyhow!(
                    "simple PPTX table cell requires one non-empty paragraph"
                ));
            }
            let paragraph = paragraph_ranges[0];
            let paragraph_xml = &text_body_xml[paragraph.start..paragraph.end];
            let paragraph_children = pptx_direct_child_local_names(paragraph_xml, "p")?;
            let mut paragraph_cursor = 0usize;
            if paragraph_children.get(paragraph_cursor).map(String::as_str) == Some("pPr") {
                paragraph_cursor += 1;
            }
            if paragraph_children.get(paragraph_cursor).map(String::as_str) != Some("r") {
                return Err(anyhow!(
                    "simple PPTX table cell paragraph requires exactly one direct text run"
                ));
            }
            paragraph_cursor += 1;
            if paragraph_children.get(paragraph_cursor).map(String::as_str) == Some("endParaRPr") {
                paragraph_cursor += 1;
            }
            if paragraph_cursor != paragraph_children.len() {
                return Err(anyhow!(
                    "simple PPTX table cell paragraph contains unsupported direct content"
                ));
            }
            if pptx_paragraph_has_unsupported_cross_run_content(paragraph_xml) {
                return Err(anyhow!(
                    "simple PPTX table cell contains a field, break, hyperlink, extension, or other unsupported content"
                ));
            }
            let paragraph_absolute_start = range.start
                + row_range.start
                + cell_range.start
                + text_body.start
                + paragraph.start;
            let paragraph_absolute = PptxXmlElementRange {
                start: paragraph_absolute_start,
                open_end: paragraph_absolute_start + (paragraph.open_end - paragraph.start),
                close_start: paragraph_absolute_start + (paragraph.close_start - paragraph.start),
                end: paragraph_absolute_start + (paragraph.end - paragraph.start),
            };
            let runs = simple_pptx_text_runs(xml, paragraph_absolute)?;
            if runs.len() != 1 {
                return Err(anyhow!(
                    "simple PPTX table cell must contain exactly one DrawingML text run"
                ));
            }
            let run = runs.into_iter().next().expect("one table cell run");
            let characters = run.decoded.chars().count();
            if characters > MAX_PPTX_TABLE_CELL_TEXT_CHARS {
                return Err(anyhow!(
                    "PPTX table cell text exceeds the {MAX_PPTX_TABLE_CELL_TEXT_CHARS} character safety limit"
                ));
            }
            total_text_chars = total_text_chars.saturating_add(characters);
            if total_text_chars > MAX_PPTX_TABLE_TOTAL_TEXT_CHARS {
                return Err(anyhow!(
                    "PPTX table text exceeds the {MAX_PPTX_TABLE_TOTAL_TEXT_CHARS} character safety limit"
                ));
            }
            cells.push(SimplePptxTableCell {
                row: row_index + 1,
                column: column_index + 1,
                range: PptxXmlElementRange {
                    start: range.start + row_range.start + cell_range.start,
                    open_end: range.start + row_range.start + cell_range.open_end,
                    close_start: range.start + row_range.start + cell_range.close_start,
                    end: range.start + row_range.start + cell_range.end,
                },
                text_start: run.text_start,
                text_open_end: run.text_open_end,
                text_close_end: run.text_close_end,
                decoded: run.decoded,
            });
            if cells.len() > MAX_PPTX_TABLE_CELLS {
                return Err(anyhow!(
                    "PPTX table cells exceed the {MAX_PPTX_TABLE_CELLS} cell safety limit"
                ));
            }
        }
    }
    Ok(SimplePptxTable {
        range,
        rows: row_ranges.len(),
        columns: grid_columns,
        cells,
    })
}

fn simple_pptx_table_rows(xml: &str, table: &SimplePptxTable) -> Result<Vec<SimplePptxTableRow>> {
    let table_xml = &xml[table.range.start..table.range.end];
    let ranges = pptx_direct_element_ranges(
        table_xml,
        "<a:tr",
        "</a:tr>",
        MAX_PPTX_TABLE_ROWS,
        "a:tbl",
        "PPTX table rows",
    )?;
    if ranges.len() != table.rows {
        return Err(anyhow!(
            "simple PPTX table row structure changed during validation"
        ));
    }
    let mut rows = Vec::with_capacity(ranges.len());
    let mut total_height = 0i64;
    for range in ranges {
        let absolute = PptxXmlElementRange {
            start: table.range.start + range.start,
            open_end: table.range.start + range.open_end,
            close_start: table.range.start + range.close_start,
            end: table.range.start + range.end,
        };
        let opening = &xml[absolute.start..absolute.open_end];
        let height = canonical_pptx_table_row_height(opening)?;
        total_height = total_height
            .checked_add(height)
            .filter(|value| *value <= SLIDE_HEIGHT)
            .ok_or_else(|| anyhow!("PPTX table total row height exceeds the slide height"))?;
        rows.push(SimplePptxTableRow {
            range: absolute,
            height,
        });
    }
    Ok(rows)
}

fn simple_pptx_table_columns(
    xml: &str,
    table: &SimplePptxTable,
) -> Result<Vec<SimplePptxTableColumn>> {
    let table_xml = &xml[table.range.start..table.range.end];
    let grid_ranges = pptx_direct_element_ranges(
        table_xml,
        "<a:tblGrid",
        "</a:tblGrid>",
        1,
        "a:tbl",
        "PPTX table grids",
    )?;
    if grid_ranges.len() != 1 || grid_ranges[0].open_end == grid_ranges[0].end {
        return Err(anyhow!(
            "simple PPTX table column editing requires one non-empty direct tblGrid"
        ));
    }
    let grid = grid_ranges[0];
    let grid_xml = &table_xml[grid.start..grid.end];
    let ranges = pptx_direct_element_ranges(
        grid_xml,
        "<a:gridCol",
        "</a:gridCol>",
        MAX_PPTX_TABLE_COLUMNS,
        "a:tblGrid",
        "PPTX table grid columns",
    )?;
    if ranges.len() != table.columns {
        return Err(anyhow!(
            "simple PPTX table column structure changed during validation"
        ));
    }
    let mut columns = Vec::with_capacity(ranges.len());
    let mut total_width = 0i64;
    for range in ranges {
        let absolute = PptxXmlElementRange {
            start: table.range.start + grid.start + range.start,
            open_end: table.range.start + grid.start + range.open_end,
            close_start: table.range.start + grid.start + range.close_start,
            end: table.range.start + grid.start + range.end,
        };
        let opening = &xml[absolute.start..absolute.open_end];
        let width = canonical_pptx_table_column_width(opening)?;
        total_width = total_width
            .checked_add(width)
            .filter(|value| *value <= SLIDE_WIDTH)
            .ok_or_else(|| anyhow!("PPTX table total column width exceeds the slide width"))?;
        columns.push(SimplePptxTableColumn {
            range: absolute,
            width,
        });
    }
    Ok(columns)
}

fn canonical_pptx_table_column_width(opening: &str) -> Result<i64> {
    let raw = opening
        .strip_prefix("<a:gridCol w=\"")
        .and_then(|value| value.strip_suffix("\"/>"))
        .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .ok_or_else(|| {
            anyhow!(
                "simple PPTX table column editing requires canonical a:gridCol elements with one w attribute"
            )
        })?;
    raw.parse::<i64>()
        .ok()
        .filter(|width| (1..=SLIDE_WIDTH).contains(width))
        .ok_or_else(|| anyhow!("PPTX table column width is outside the local safety limit"))
}

fn canonical_pptx_table_column_opening(width: i64) -> String {
    format!("<a:gridCol w=\"{width}\"/>")
}

fn canonical_pptx_table_row_height(opening: &str) -> Result<i64> {
    let raw = opening
        .strip_prefix("<a:tr h=\"")
        .and_then(|value| value.strip_suffix("\">"))
        .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .ok_or_else(|| {
            anyhow!(
                "simple PPTX table row editing requires canonical a:tr elements with one h attribute"
            )
        })?;
    raw.parse::<i64>()
        .ok()
        .filter(|height| (1..=SLIDE_HEIGHT).contains(height))
        .ok_or_else(|| anyhow!("PPTX table row height is outside the local safety limit"))
}

fn canonical_pptx_table_row_opening(height: i64) -> String {
    format!("<a:tr h=\"{height}\">")
}

fn pptx_table_row_with_height(xml: &str, row: SimplePptxTableRow, height: i64) -> Result<String> {
    let mut output = xml[row.range.start..row.range.end].to_string();
    output.replace_range(
        0..row.range.open_end - row.range.start,
        canonical_pptx_table_row_opening(height).as_str(),
    );
    Ok(output)
}

fn clone_pptx_table_row_with_text(
    xml: &str,
    row: SimplePptxTableRow,
    cells: &[SimplePptxTableCell],
    values: &[String],
    height: i64,
) -> Result<String> {
    if cells.len() != values.len() {
        return Err(anyhow!(
            "inserted PPTX table row cell count does not match the reference row"
        ));
    }
    let mut output = xml[row.range.start..row.range.end].to_string();
    let mut edits = cells
        .iter()
        .zip(values)
        .map(|(cell, value)| {
            let opening = &xml[cell.text_start..cell.text_open_end];
            let opening = pptx_text_opening_for_value(opening, value)?;
            Ok((
                cell.text_start - row.range.start,
                cell.text_close_end - row.range.start,
                format!("{opening}{}</a:t>", escape_xml(value)),
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    edits.sort_by(|left, right| right.0.cmp(&left.0));
    for (start, end, replacement) in edits {
        output.replace_range(start..end, replacement.as_str());
    }
    output.replace_range(
        0..row.range.open_end - row.range.start,
        canonical_pptx_table_row_opening(height).as_str(),
    );
    Ok(output)
}

fn clone_pptx_table_cell_with_text(
    xml: &str,
    cell: &SimplePptxTableCell,
    value: &str,
) -> Result<String> {
    let mut output = xml[cell.range.start..cell.range.end].to_string();
    let opening = &xml[cell.text_start..cell.text_open_end];
    let opening = pptx_text_opening_for_value(opening, value)?;
    output.replace_range(
        cell.text_start - cell.range.start..cell.text_close_end - cell.range.start,
        format!("{opening}{}</a:t>", escape_xml(value)).as_str(),
    );
    Ok(output)
}

fn ensure_changed_pptx_table_move(
    source: usize,
    reference: usize,
    position: &str,
    item: &str,
) -> Result<()> {
    if source == reference {
        return Err(anyhow!(
            "{item} and reference_{item} must select different {item}s"
        ));
    }
    if (position == "before" && source + 1 == reference)
        || (position == "after" && reference + 1 == source)
    {
        return Err(anyhow!(
            "requested PPTX table {item} move is already in the requested position"
        ));
    }
    Ok(())
}

fn moved_pptx_table_index(source: usize, reference: usize, position: &str) -> Result<usize> {
    match (source < reference, position) {
        (true, "before") => Ok(reference - 1),
        (true, "after") => Ok(reference),
        (false, "before") => Ok(reference),
        (false, "after") => Ok(reference + 1),
        (_, _) => Err(anyhow!("position must be before or after")),
    }
}

fn move_pptx_xml_element_edit(
    xml: &str,
    source: PptxXmlElementRange,
    reference: PptxXmlElementRange,
    position: &str,
) -> Result<(usize, usize, String)> {
    if source.start >= source.end
        || reference.start >= reference.end
        || source.end > xml.len()
        || reference.end > xml.len()
        || !xml.is_char_boundary(source.start)
        || !xml.is_char_boundary(source.end)
        || !xml.is_char_boundary(reference.start)
        || !xml.is_char_boundary(reference.end)
        || (source.start < reference.end && reference.start < source.end)
    {
        return Err(anyhow!(
            "PPTX table move element ranges are invalid or overlapping"
        ));
    }
    let source_xml = &xml[source.start..source.end];
    match (source.start < reference.start, position) {
        (true, "before") => Ok((
            source.start,
            reference.start,
            format!("{}{source_xml}", &xml[source.end..reference.start]),
        )),
        (true, "after") => Ok((
            source.start,
            reference.end,
            format!("{}{source_xml}", &xml[source.end..reference.end]),
        )),
        (false, "before") => Ok((
            reference.start,
            source.end,
            format!("{source_xml}{}", &xml[reference.start..source.start]),
        )),
        (false, "after") => Ok((
            reference.end,
            source.end,
            format!("{source_xml}{}", &xml[reference.end..source.start]),
        )),
        (_, _) => Err(anyhow!("position must be before or after")),
    }
}

fn apply_pptx_xml_edits(xml: &str, mut edits: Vec<(usize, usize, String)>) -> Result<String> {
    edits.sort_by(|left, right| right.0.cmp(&left.0));
    let mut next_start = xml.len();
    let mut output = xml.to_string();
    for (start, end, replacement) in edits {
        if start > end
            || end > xml.len()
            || !xml.is_char_boundary(start)
            || !xml.is_char_boundary(end)
            || end > next_start
        {
            return Err(anyhow!("PPTX XML edit ranges are invalid or overlapping"));
        }
        output.replace_range(start..end, replacement.as_str());
        next_start = start;
    }
    if output.len() > super::MAX_XML_BYTES {
        return Err(anyhow!(
            "updated PPTX slide XML exceeds the local XML size limit"
        ));
    }
    Ok(output)
}

fn validate_updated_pptx_table_rows(
    xml: &str,
    table_number: usize,
    expected_rows: usize,
    expected_columns: usize,
) -> Result<()> {
    let tables = scan_pptx_tables(xml)?;
    let table = tables
        .get(table_number - 1)
        .ok_or_else(|| anyhow!("updated PPTX slide is missing the structurally edited table"))?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "updated PPTX table no longer satisfies the simple-table contract: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if simple.rows != expected_rows || simple.columns != expected_columns {
        return Err(anyhow!(
            "updated PPTX table dimensions do not match the requested row edit"
        ));
    }
    simple_pptx_table_rows(xml, simple)?;
    Ok(())
}

fn validate_updated_pptx_table_columns(
    xml: &str,
    table_number: usize,
    expected_rows: usize,
    expected_columns: usize,
) -> Result<()> {
    let tables = scan_pptx_tables(xml)?;
    let table = tables
        .get(table_number - 1)
        .ok_or_else(|| anyhow!("updated PPTX slide is missing the structurally edited table"))?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "updated PPTX table no longer satisfies the simple-table contract: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if simple.rows != expected_rows || simple.columns != expected_columns {
        return Err(anyhow!(
            "updated PPTX table dimensions do not match the requested column edit"
        ));
    }
    simple_pptx_table_columns(xml, simple)?;
    Ok(())
}

fn validate_updated_pptx_table_cells(
    xml: &str,
    table_number: usize,
    expected_rows: usize,
    expected_columns: usize,
) -> Result<()> {
    let tables = scan_pptx_tables(xml)?;
    let table = tables
        .get(table_number - 1)
        .ok_or_else(|| anyhow!("updated PPTX slide is missing the formatted table"))?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "updated PPTX table no longer satisfies the simple-table contract: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if simple.rows != expected_rows || simple.columns != expected_columns {
        return Err(anyhow!(
            "updated PPTX table dimensions changed during cell format copying"
        ));
    }
    Ok(())
}

fn pptx_direct_element_ranges(
    xml: &str,
    opening: &str,
    closing: &str,
    maximum: usize,
    parent: &str,
    label: &str,
) -> Result<Vec<PptxXmlElementRange>> {
    let ranges = pptx_xml_element_ranges(xml, opening, closing, maximum, label)?;
    for range in &ranges {
        let stack = pptx_xml_open_element_stack_at(xml, range.start)?;
        if stack.len() != 1 || stack[0] != parent {
            return Err(anyhow!("{label} must be direct children of {parent}"));
        }
    }
    Ok(ranges)
}

fn pptx_direct_child_local_names(xml: &str, expected_root: &str) -> Result<Vec<String>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut depth = 0usize;
    let mut root_seen = false;
    let mut children = Vec::new();
    loop {
        match reader.read_event().context("parse PPTX table XML")? {
            Event::Start(event) => {
                let local = String::from_utf8_lossy(event.local_name().as_ref()).into_owned();
                if depth == 0 {
                    if root_seen || local != expected_root {
                        return Err(anyhow!("PPTX table XML has an unexpected root element"));
                    }
                    root_seen = true;
                } else if depth == 1 {
                    children.push(local);
                }
                depth = depth
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("PPTX table XML nesting exceeds the safety limit"))?;
                if depth > 256 {
                    return Err(anyhow!("PPTX table XML nesting exceeds the safety limit"));
                }
            }
            Event::Empty(event) => {
                let local = String::from_utf8_lossy(event.local_name().as_ref()).into_owned();
                if depth == 0 {
                    if root_seen || local != expected_root {
                        return Err(anyhow!("PPTX table XML has an unexpected root element"));
                    }
                    root_seen = true;
                } else if depth == 1 {
                    children.push(local);
                }
            }
            Event::End(_) => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| anyhow!("PPTX table XML has an unmatched closing tag"))?;
            }
            Event::Text(text) => {
                if depth <= 1
                    && !text
                        .xml_content(XmlVersion::Explicit1_0)
                        .context("decode PPTX table whitespace")?
                        .trim()
                        .is_empty()
                {
                    return Err(anyhow!(
                        "PPTX table XML contains unexpected direct text content"
                    ));
                }
            }
            Event::Decl(_) if depth == 0 && !root_seen => {}
            Event::Comment(_) | Event::CData(_) | Event::DocType(_) | Event::PI(_) => {
                return Err(anyhow!(
                    "simple PPTX table editing does not support comments, CDATA, DTD, or processing instructions"
                ));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !root_seen || depth != 0 {
        return Err(anyhow!("PPTX table XML has invalid element boundaries"));
    }
    Ok(children)
}

fn pptx_opening_attribute(
    opening: &str,
    expected_local_name: &str,
    attribute_name: &str,
) -> Result<Option<String>> {
    let mut reader = Reader::from_str(opening);
    reader.config_mut().trim_text(false);
    match reader.read_event().context("parse PPTX opening tag")? {
        Event::Start(event) | Event::Empty(event) => {
            if event.local_name().as_ref() != expected_local_name.as_bytes() {
                return Err(anyhow!("PPTX XML opening tag has an unexpected name"));
            }
            optional_xml_attribute(&reader, &event, attribute_name)
        }
        _ => Err(anyhow!("PPTX XML opening tag is invalid")),
    }
}

fn scan_pptx_cross_run_text(xml: &str, selection: &str) -> Result<PptxCrossRunScan> {
    if xml.contains("<!--") || xml.contains("<![CDATA[") || xml.contains("<!DOCTYPE") {
        return Err(anyhow!(
            "PPTX cross-run replacement does not support comments, CDATA, or DTD markup"
        ));
    }
    let paragraphs = pptx_xml_element_ranges(
        xml,
        "<a:p",
        "</a:p>",
        MAX_PPTX_PARAGRAPHS_PER_SLIDE,
        "PPTX paragraphs",
    )?;
    let mut occurrences = 0usize;
    let mut matched = None::<PptxCrossRunTextMatch>;
    let mut unsupported_reason = None::<String>;
    for paragraph in paragraphs {
        let paragraph_xml = &xml[paragraph.start..paragraph.end];
        let visible_text = pptx_visible_text(paragraph_xml)?;
        for start in overlapping_pptx_text_match_starts(visible_text.as_str(), selection) {
            occurrences = occurrences.saturating_add(1);
            let candidate = pptx_cross_run_match_in_paragraph(
                xml,
                paragraph,
                visible_text.as_str(),
                start,
                start + selection.len(),
            );
            match candidate {
                Ok(candidate) => matched = Some(candidate),
                Err(error) => unsupported_reason = Some(error.to_string()),
            }
        }
    }
    Ok(PptxCrossRunScan {
        occurrences,
        matched: (occurrences == 1).then_some(matched).flatten(),
        unsupported_reason,
    })
}

fn pptx_cross_run_match_in_paragraph(
    slide_xml: &str,
    paragraph: PptxXmlElementRange,
    visible_text: &str,
    selection_start: usize,
    selection_end: usize,
) -> Result<PptxCrossRunTextMatch> {
    let paragraph_xml = &slide_xml[paragraph.start..paragraph.end];
    if pptx_paragraph_has_unsupported_cross_run_content(paragraph_xml) {
        return Err(anyhow!(
            "paragraph contains a field, line break, hyperlink, extension, or other unsupported DrawingML content"
        ));
    }
    let runs = simple_pptx_text_runs(slide_xml, paragraph)?;
    let combined = runs
        .iter()
        .map(|run| run.decoded.as_str())
        .collect::<String>();
    if combined != visible_text {
        return Err(anyhow!(
            "paragraph visible text is not represented by direct simple DrawingML runs"
        ));
    }
    let mut cumulative = 0usize;
    let mut first = None::<(usize, usize)>;
    let mut last = None::<(usize, usize)>;
    for (index, run) in runs.iter().enumerate() {
        let next = cumulative.saturating_add(run.decoded.len());
        if first.is_none() && selection_start >= cumulative && selection_start < next {
            first = Some((index, selection_start - cumulative));
        }
        if selection_end > cumulative && selection_end <= next {
            last = Some((index, selection_end - cumulative));
            break;
        }
        cumulative = next;
    }
    let (first_index, first_offset) = first
        .ok_or_else(|| anyhow!("selection start does not map to a simple DrawingML text run"))?;
    let (last_index, last_offset) =
        last.ok_or_else(|| anyhow!("selection end does not map to a simple DrawingML text run"))?;
    if first_index == last_index {
        return Err(anyhow!(
            "selection is contained inside one run; use replace_pptx_text instead"
        ));
    }
    let touched = last_index - first_index + 1;
    if touched > MAX_PPTX_CROSS_RUNS {
        return Err(anyhow!(
            "selection spans {touched} runs, exceeding the {MAX_PPTX_CROSS_RUNS} run safety limit"
        ));
    }
    let formatting = runs[first_index].formatting.as_str();
    if runs[first_index..=last_index]
        .iter()
        .any(|run| run.formatting != formatting)
    {
        return Err(anyhow!(
            "selection crosses runs with different DrawingML run properties"
        ));
    }
    for pair in runs[first_index..=last_index].windows(2) {
        if !slide_xml[pair[0].run_end..pair[1].run_start]
            .trim()
            .is_empty()
        {
            return Err(anyhow!(
                "selection crosses non-run markup between adjacent DrawingML runs"
            ));
        }
    }
    Ok(PptxCrossRunTextMatch {
        runs: runs[first_index..=last_index].to_vec(),
        first_offset,
        last_offset,
    })
}

fn simple_pptx_text_runs(
    slide_xml: &str,
    paragraph: PptxXmlElementRange,
) -> Result<Vec<SimplePptxTextRun>> {
    let paragraph_xml = &slide_xml[paragraph.start..paragraph.end];
    let ranges = pptx_xml_element_ranges(
        paragraph_xml,
        "<a:r",
        "</a:r>",
        MAX_PPTX_RUNS_PER_PARAGRAPH,
        "PPTX paragraph runs",
    )?;
    if ranges.is_empty() {
        return Err(anyhow!("paragraph contains no simple DrawingML runs"));
    }
    let mut runs = Vec::with_capacity(ranges.len());
    for range in ranges {
        let stack = pptx_xml_open_element_stack_at(paragraph_xml, range.start)?;
        if stack.len() != 1 || stack[0] != "a:p" {
            return Err(anyhow!(
                "paragraph contains a DrawingML run that is not a direct child"
            ));
        }
        if range.open_end == range.end {
            return Err(anyhow!("paragraph contains a self-closing DrawingML run"));
        }
        let run_start = paragraph.start + range.start;
        let run_end = paragraph.start + range.end;
        let run_xml = &slide_xml[run_start..run_end];
        let run_opening = &run_xml[..range.open_end - range.start];
        if run_opening != "<a:r>" {
            return Err(anyhow!(
                "PPTX cross-run replacement supports only standard a:r opening tags"
            ));
        }
        let text_ranges =
            pptx_xml_element_ranges(run_xml, "<a:t", "</a:t>", 2, "PPTX run text elements")?;
        if text_ranges.len() != 1 || text_ranges[0].open_end == text_ranges[0].end {
            return Err(anyhow!(
                "paragraph contains a run that is not one simple DrawingML text run"
            ));
        }
        let text = text_ranges[0];
        let text_stack = pptx_xml_open_element_stack_at(run_xml, text.start)?;
        if text_stack.len() != 1 || text_stack[0] != "a:r" {
            return Err(anyhow!("DrawingML text is not a direct child of its run"));
        }
        let text_opening = &run_xml[text.start..text.open_end];
        if !matches!(text_opening, "<a:t>" | "<a:t xml:space=\"preserve\">") {
            return Err(anyhow!(
                "PPTX cross-run replacement supports only standard a:t opening tags"
            ));
        }
        let raw_text = &run_xml[text.open_end..text.close_start];
        if raw_text.contains('<') {
            return Err(anyhow!(
                "PPTX cross-run text contains unsupported nested XML"
            ));
        }
        let prefix = run_xml[range.open_end - range.start..text.start].trim();
        let formatting = simple_pptx_run_properties(prefix)?;
        if !run_xml[text.end..range.close_start - range.start]
            .trim()
            .is_empty()
        {
            return Err(anyhow!(
                "simple DrawingML text run contains content after a:t"
            ));
        }
        let decoded = unescape(raw_text)
            .context("decode PPTX cross-run DrawingML text")?
            .into_owned();
        runs.push(SimplePptxTextRun {
            run_start,
            run_end,
            text_start: run_start + text.start,
            text_open_end: run_start + text.open_end,
            text_close_end: run_start + text.end,
            formatting,
            decoded,
        });
    }
    let all_text = pptx_text_values(paragraph_xml)?;
    if all_text.len() != runs.len()
        || all_text.concat()
            != runs
                .iter()
                .map(|run| run.decoded.as_str())
                .collect::<String>()
    {
        return Err(anyhow!(
            "paragraph contains text outside the direct simple DrawingML runs"
        ));
    }
    Ok(runs)
}

fn simple_pptx_run_properties(prefix: &str) -> Result<String> {
    if prefix.is_empty() {
        return Ok(String::new());
    }
    let ranges = pptx_xml_element_ranges(prefix, "<a:rPr", "</a:rPr>", 1, "PPTX run properties")?;
    if ranges.len() != 1
        || !prefix[..ranges[0].start].trim().is_empty()
        || !prefix[ranges[0].end..].trim().is_empty()
    {
        return Err(anyhow!(
            "simple DrawingML run contains content other than one run-properties element"
        ));
    }
    Ok(prefix[ranges[0].start..ranges[0].end].to_string())
}

fn pptx_visible_text(xml: &str) -> Result<String> {
    Ok(pptx_text_values(xml)?.concat())
}

fn pptx_text_values(xml: &str) -> Result<Vec<String>> {
    let ranges = pptx_xml_element_ranges(
        xml,
        "<a:t",
        "</a:t>",
        MAX_PPTX_RUNS_PER_PARAGRAPH.saturating_mul(2),
        "PPTX text elements",
    )?;
    let mut values = Vec::with_capacity(ranges.len());
    let mut characters = 0usize;
    for range in ranges {
        if range.open_end == range.end {
            values.push(String::new());
            continue;
        }
        let raw = &xml[range.open_end..range.close_start];
        if raw.contains('<') {
            return Err(anyhow!(
                "PPTX DrawingML text contains unsupported nested XML"
            ));
        }
        let decoded = unescape(raw)
            .context("decode PPTX DrawingML paragraph text")?
            .into_owned();
        characters = characters.saturating_add(decoded.chars().count());
        if characters > MAX_SLIDE_TEXT_CHARS {
            return Err(anyhow!(
                "PPTX paragraph text exceeds the {MAX_SLIDE_TEXT_CHARS} character safety limit"
            ));
        }
        values.push(decoded);
    }
    Ok(values)
}

fn overlapping_pptx_text_match_starts(text: &str, selection: &str) -> Vec<usize> {
    text.char_indices()
        .filter_map(|(index, _)| text[index..].starts_with(selection).then_some(index))
        .collect()
}

fn pptx_paragraph_has_unsupported_cross_run_content(paragraph_xml: &str) -> bool {
    [
        "<a:fld",
        "<a:br",
        "<a:tab",
        "<a:hlinkClick",
        "<a:hlinkMouseOver",
        "<a:extLst",
    ]
    .iter()
    .any(|marker| find_next_pptx_xml_tag_start(paragraph_xml, marker, 0).is_some())
}

fn rewrite_pptx_cross_run_match(
    slide_xml: &str,
    matched: &PptxCrossRunTextMatch,
    replacement: &str,
) -> Result<(String, usize, usize)> {
    let mut replacements = Vec::<(usize, usize, String)>::with_capacity(matched.runs.len());
    let mut emptied_runs = 0usize;
    let last_index = matched.runs.len() - 1;
    for (index, run) in matched.runs.iter().enumerate() {
        let text = if index == 0 {
            format!("{}{}", &run.decoded[..matched.first_offset], replacement)
        } else if index == last_index {
            run.decoded[matched.last_offset..].to_string()
        } else {
            String::new()
        };
        if text.is_empty() {
            emptied_runs = emptied_runs.saturating_add(1);
        }
        let opening = &slide_xml[run.text_start..run.text_open_end];
        let opening = pptx_text_opening_for_value(opening, text.as_str())?;
        replacements.push((
            run.text_start,
            run.text_close_end,
            format!("{opening}{}</a:t>", escape_xml(text.as_str())),
        ));
    }
    let mut output = slide_xml.to_string();
    for (start, end, replacement) in replacements.into_iter().rev() {
        output.replace_range(start..end, replacement.as_str());
    }
    if output.len() > super::MAX_XML_BYTES {
        return Err(anyhow!(
            "updated PPTX slide XML exceeds the local XML size limit"
        ));
    }
    Ok((output, matched.runs.len(), emptied_runs))
}

fn pptx_text_opening_for_value(opening: &str, value: &str) -> Result<String> {
    let needs_preserve = value.chars().next().is_some_and(char::is_whitespace)
        || value.chars().next_back().is_some_and(char::is_whitespace);
    match (opening, needs_preserve) {
        ("<a:t>", true) => Ok("<a:t xml:space=\"preserve\">".to_string()),
        ("<a:t>", false) | ("<a:t xml:space=\"preserve\">", _) => Ok(opening.to_string()),
        _ => Err(anyhow!(
            "PPTX cross-run replacement supports only standard a:t opening tags"
        )),
    }
}

fn pptx_xml_element_ranges(
    xml: &str,
    opening: &str,
    closing: &str,
    maximum: usize,
    label: &str,
) -> Result<Vec<PptxXmlElementRange>> {
    let mut ranges = Vec::new();
    let mut cursor = 0usize;
    let mut depth = 0usize;
    let mut current = None::<(usize, usize)>;
    loop {
        let next_open = find_next_pptx_xml_tag_start(xml, opening, cursor);
        let next_close = xml[cursor..].find(closing).map(|offset| cursor + offset);
        if next_open.is_none() && next_close.is_none() {
            break;
        }
        if next_open.is_some_and(|open| next_close.is_none_or(|close| open < close)) {
            let open_start = next_open.expect("opening tag was selected");
            let open_end = pptx_xml_tag_end(xml, open_start, xml.len())?;
            let self_closing = xml[open_start..open_end - 1].trim_end().ends_with('/');
            if self_closing {
                if depth == 0 {
                    ranges.push(PptxXmlElementRange {
                        start: open_start,
                        open_end,
                        close_start: open_end,
                        end: open_end,
                    });
                    if ranges.len() > maximum {
                        return Err(anyhow!("{label} exceed the {maximum} item safety limit"));
                    }
                }
                cursor = open_end;
                continue;
            }
            if depth == 0 {
                current = Some((open_start, open_end));
            }
            depth = depth
                .checked_add(1)
                .ok_or_else(|| anyhow!("{label} nesting exceeds the local safety limit"))?;
            cursor = open_end;
        } else {
            let close_start = next_close.expect("closing tag was selected");
            if depth == 0 {
                return Err(anyhow!("{label} contain an unmatched closing tag"));
            }
            let close_end = close_start + closing.len();
            depth -= 1;
            if depth == 0 {
                let (start, open_end) = current
                    .take()
                    .ok_or_else(|| anyhow!("{label} have an invalid element boundary"))?;
                ranges.push(PptxXmlElementRange {
                    start,
                    open_end,
                    close_start,
                    end: close_end,
                });
                if ranges.len() > maximum {
                    return Err(anyhow!("{label} exceed the {maximum} item safety limit"));
                }
            }
            cursor = close_end;
        }
    }
    if depth != 0 {
        return Err(anyhow!("{label} contain an unclosed element"));
    }
    Ok(ranges)
}

fn find_next_pptx_xml_tag_start(xml: &str, prefix: &str, mut cursor: usize) -> Option<usize> {
    while let Some(offset) = xml[cursor..].find(prefix) {
        let index = cursor + offset;
        let suffix = xml.as_bytes().get(index + prefix.len()).copied();
        if suffix.is_some_and(|byte| byte == b'>' || byte == b'/' || byte.is_ascii_whitespace()) {
            return Some(index);
        }
        cursor = index + prefix.len();
    }
    None
}

fn pptx_xml_open_element_stack_at(xml: &str, boundary: usize) -> Result<Vec<String>> {
    if boundary > xml.len() || !xml.is_char_boundary(boundary) {
        return Err(anyhow!("PPTX XML boundary is invalid"));
    }
    let mut stack = Vec::<String>::new();
    let mut cursor = 0usize;
    while let Some(offset) = xml[cursor..boundary].find('<') {
        let start = cursor + offset;
        if xml[start..boundary].starts_with("<?") {
            let end = xml[start + 2..boundary]
                .find("?>")
                .map(|offset| start + 2 + offset + 2)
                .ok_or_else(|| anyhow!("PPTX XML processing instruction is unterminated"))?;
            cursor = end;
            continue;
        }
        if xml[start..boundary].starts_with("<!") {
            return Err(anyhow!(
                "PPTX cross-run replacement does not support declarations inside slide XML"
            ));
        }
        let end = pptx_xml_tag_end(xml, start, boundary)?;
        let tag = xml[start + 1..end - 1].trim();
        if let Some(closing) = tag.strip_prefix('/') {
            let name = closing
                .split_ascii_whitespace()
                .next()
                .filter(|name| !name.is_empty())
                .ok_or_else(|| anyhow!("PPTX XML contains an invalid closing tag"))?;
            let opened = stack
                .pop()
                .ok_or_else(|| anyhow!("PPTX XML contains an unmatched closing tag"))?;
            if opened != name {
                return Err(anyhow!("PPTX XML contains mismatched element boundaries"));
            }
        } else {
            let self_closing = tag.trim_end().ends_with('/');
            let name = tag
                .trim_end_matches('/')
                .split_ascii_whitespace()
                .next()
                .filter(|name| !name.is_empty())
                .ok_or_else(|| anyhow!("PPTX XML contains an invalid opening tag"))?;
            if !self_closing {
                stack.push(name.to_string());
                if stack.len() > 256 {
                    return Err(anyhow!("PPTX XML nesting exceeds the safety limit"));
                }
            }
        }
        cursor = end;
    }
    Ok(stack)
}

fn pptx_xml_tag_end(xml: &str, start: usize, boundary: usize) -> Result<usize> {
    let mut quote = None::<u8>;
    for (offset, byte) in xml.as_bytes()[start + 1..boundary]
        .iter()
        .copied()
        .enumerate()
    {
        match (quote, byte) {
            (Some(active), current) if active == current => quote = None,
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'>') => return Ok(start + 1 + offset + 1),
            _ => {}
        }
    }
    Err(anyhow!("PPTX XML contains an unterminated tag"))
}

fn parse_relationship_document(xml: &str, source_part: &str) -> Result<RelationshipDocument> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut relationships = Vec::new();
    let mut ids = HashSet::new();
    let mut relationship_tag_name = None;
    let mut root_count = 0usize;
    loop {
        match reader
            .read_event()
            .with_context(|| format!("parse PPTX relationships for {source_part}"))?
        {
            Event::Start(event) if event.local_name().as_ref() == b"Relationships" => {
                root_count = root_count.saturating_add(1);
            }
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"Relationship" =>
            {
                let id = required_xml_attribute(&reader, &event, "Id")?;
                if !ids.insert(id.clone()) {
                    return Err(anyhow!(
                        "PPTX relationship document contains duplicate Id: {id}"
                    ));
                }
                let relationship_type = required_xml_attribute(&reader, &event, "Type")?;
                let target = required_xml_attribute(&reader, &event, "Target")?;
                let external = optional_xml_attribute(&reader, &event, "TargetMode")?
                    .is_some_and(|value| value.eq_ignore_ascii_case("external"));
                if !external {
                    resolve_part_target(source_part, target.as_str())?;
                }
                let current_tag = String::from_utf8_lossy(event.name().as_ref()).into_owned();
                if relationship_tag_name
                    .as_ref()
                    .is_some_and(|existing| existing != &current_tag)
                {
                    return Err(anyhow!("PPTX relationship document mixes namespaces"));
                }
                relationship_tag_name = Some(current_tag);
                relationships.push(PackageRelationship {
                    id,
                    relationship_type,
                    target,
                    external,
                });
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if root_count != 1 {
        return Err(anyhow!(
            "PPTX relationship document must contain exactly one Relationships root"
        ));
    }
    Ok(RelationshipDocument {
        relationships,
        relationship_tag_name: relationship_tag_name.unwrap_or_else(|| "Relationship".to_string()),
    })
}

fn content_types_metadata(xml: &str) -> Result<ContentTypesMetadata> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut defaults = HashMap::new();
    let mut overrides = HashSet::new();
    let mut default_tag_name = None;
    let mut override_tag_name = None;
    let mut root_count = 0usize;
    loop {
        match reader.read_event().context("parse PPTX content types")? {
            Event::Start(event) if event.local_name().as_ref() == b"Types" => {
                root_count = root_count.saturating_add(1);
            }
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"Default" =>
            {
                let extension =
                    required_xml_attribute(&reader, &event, "Extension")?.to_ascii_lowercase();
                let content_type = required_xml_attribute(&reader, &event, "ContentType")?;
                if defaults.insert(extension, content_type).is_some() {
                    return Err(anyhow!(
                        "PPTX content types contain a duplicate Default extension"
                    ));
                }
                default_tag_name =
                    Some(String::from_utf8_lossy(event.name().as_ref()).into_owned());
            }
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"Override" =>
            {
                let part_name = required_xml_attribute(&reader, &event, "PartName")?;
                if !part_name.starts_with('/') || !overrides.insert(part_name) {
                    return Err(anyhow!(
                        "PPTX content types contain an invalid or duplicate Override"
                    ));
                }
                override_tag_name =
                    Some(String::from_utf8_lossy(event.name().as_ref()).into_owned());
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if root_count != 1 {
        return Err(anyhow!(
            "PPTX content types must contain exactly one Types root"
        ));
    }
    let default_tag_name = default_tag_name.unwrap_or_else(|| "Default".to_string());
    let override_tag_name = override_tag_name
        .unwrap_or_else(|| sibling_qualified_name(default_tag_name.as_str(), "Override"));
    Ok(ContentTypesMetadata {
        defaults,
        overrides,
        default_tag_name,
        override_tag_name,
    })
}

fn append_presentation_slide_ids(
    xml: &str,
    metadata: &PresentationSlideMetadata,
    additions: &[(u32, String)],
) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(
        xml.len().saturating_add(additions.len().saturating_mul(64)),
    ));
    let mut inserted = false;
    loop {
        let event = reader
            .read_event()
            .context("rewrite PPTX presentation slide list")?;
        match &event {
            Event::End(end) if end.local_name().as_ref() == b"sldIdLst" => {
                if inserted {
                    return Err(anyhow!("PPTX presentation contains duplicate slide lists"));
                }
                for (slide_id, relationship_id) in additions {
                    let slide_id_text = slide_id.to_string();
                    let mut slide = BytesStart::new(metadata.slide_tag_name.as_str());
                    slide.push_attribute(("id", slide_id_text.as_str()));
                    slide.push_attribute((
                        metadata.relationship_attribute_name.as_str(),
                        relationship_id.as_str(),
                    ));
                    writer.write_event(Event::Empty(slide))?;
                }
                inserted = true;
            }
            Event::Eof => {
                writer.write_event(event.into_owned())?;
                break;
            }
            _ => {}
        }
        writer.write_event(event.into_owned())?;
    }
    if !inserted {
        return Err(anyhow!(
            "PPTX presentation is missing a writable slide list"
        ));
    }
    xml_output(writer, "updated PPTX presentation XML")
}

fn rewrite_presentation_slide_ids(
    xml: &str,
    metadata: &PresentationSlideMetadata,
    slide_positions: &[usize],
) -> Result<String> {
    if slide_positions.is_empty() {
        return Err(anyhow!("PPTX slide list must retain at least one slide"));
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(xml.len()));
    let mut in_slide_list = false;
    let mut replaced = false;
    let mut existing_slides = 0usize;
    loop {
        let event = reader
            .read_event()
            .context("reorder PPTX presentation slide list")?;
        match event {
            Event::Start(start) if start.local_name().as_ref() == b"sldIdLst" => {
                if in_slide_list || replaced {
                    return Err(anyhow!("PPTX presentation contains duplicate slide lists"));
                }
                in_slide_list = true;
                writer.write_event(Event::Start(start.into_owned()))?;
            }
            Event::Empty(empty) if empty.local_name().as_ref() == b"sldIdLst" => {
                return Err(anyhow!("PPTX presentation slide list is empty"));
            }
            Event::Empty(slide) if in_slide_list && slide.local_name().as_ref() == b"sldId" => {
                existing_slides = existing_slides.saturating_add(1);
            }
            Event::Start(slide) if in_slide_list && slide.local_name().as_ref() == b"sldId" => {
                return Err(anyhow!(
                    "PPTX slide reordering requires empty slide-id elements without extension content"
                ));
            }
            Event::End(end) if end.local_name().as_ref() == b"sldIdLst" => {
                if !in_slide_list || existing_slides != metadata.relationship_ids.len() {
                    return Err(anyhow!(
                        "PPTX presentation slide list changed during validation"
                    ));
                }
                for position in slide_positions {
                    let index = position - 1;
                    if index >= metadata.relationship_ids.len() {
                        return Err(anyhow!(
                            "PPTX rewritten slide position is outside the visible slide list"
                        ));
                    }
                    let slide_id_text = metadata.slide_ids[index].to_string();
                    let mut slide = BytesStart::new(metadata.slide_tag_name.as_str());
                    slide.push_attribute(("id", slide_id_text.as_str()));
                    slide.push_attribute((
                        metadata.relationship_attribute_name.as_str(),
                        metadata.relationship_ids[index].as_str(),
                    ));
                    writer.write_event(Event::Empty(slide))?;
                }
                writer.write_event(Event::End(end.into_owned()))?;
                in_slide_list = false;
                replaced = true;
            }
            Event::Text(text)
                if in_slide_list && String::from_utf8_lossy(text.as_ref()).trim().is_empty() =>
            {
                writer.write_event(Event::Text(text.into_owned()))?;
            }
            Event::Eof => {
                writer.write_event(Event::Eof)?;
                break;
            }
            event => {
                if in_slide_list {
                    return Err(anyhow!(
                        "PPTX slide list contains unsupported extension or mixed content"
                    ));
                }
                writer.write_event(event.into_owned())?;
            }
        }
    }
    if in_slide_list || !replaced {
        return Err(anyhow!(
            "PPTX presentation is missing a writable slide list"
        ));
    }
    xml_output(writer, "reordered PPTX presentation XML")
}

fn append_relationship_entries(
    xml: &str,
    relationship_tag_name: &str,
    additions: &[RelationshipAddition],
) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(
        xml.len()
            .saturating_add(additions.len().saturating_mul(180)),
    ));
    let mut inserted = false;
    loop {
        let event = reader
            .read_event()
            .context("rewrite PPTX relationship document")?;
        match &event {
            Event::End(end) if end.local_name().as_ref() == b"Relationships" => {
                if inserted {
                    return Err(anyhow!("PPTX relationship document has duplicate roots"));
                }
                for addition in additions {
                    let mut relationship = BytesStart::new(relationship_tag_name);
                    relationship.push_attribute(("Id", addition.id.as_str()));
                    relationship.push_attribute(("Type", addition.relationship_type));
                    relationship.push_attribute(("Target", addition.target.as_str()));
                    writer.write_event(Event::Empty(relationship))?;
                }
                inserted = true;
            }
            Event::Eof => {
                writer.write_event(event.into_owned())?;
                break;
            }
            _ => {}
        }
        writer.write_event(event.into_owned())?;
    }
    if !inserted {
        return Err(anyhow!(
            "PPTX relationship document is missing a writable root"
        ));
    }
    xml_output(writer, "updated PPTX relationship XML")
}

fn remove_relationship_entries(xml: &str, removed_ids: &HashSet<String>) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(xml.len()));
    let mut removed = HashSet::new();
    loop {
        let event = reader
            .read_event()
            .context("remove PPTX relationship entries")?;
        match event {
            Event::Empty(relationship) if relationship.local_name().as_ref() == b"Relationship" => {
                let id = required_xml_attribute(&reader, &relationship, "Id")?;
                if removed_ids.contains(id.as_str()) {
                    removed.insert(id);
                } else {
                    writer.write_event(Event::Empty(relationship.into_owned()))?;
                }
            }
            Event::Start(relationship) if relationship.local_name().as_ref() == b"Relationship" => {
                return Err(anyhow!(
                    "PPTX relationship removal requires empty Relationship elements"
                ));
            }
            Event::Eof => {
                writer.write_event(Event::Eof)?;
                break;
            }
            event => writer.write_event(event.into_owned())?,
        }
    }
    if &removed != removed_ids {
        return Err(anyhow!(
            "PPTX relationship document is missing a relationship selected for removal"
        ));
    }
    xml_output(writer, "updated PPTX relationship XML")
}

fn remove_content_type_overrides(
    xml: &str,
    removed_part_names: &HashSet<String>,
) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(xml.len()));
    let mut removed = HashSet::new();
    loop {
        let event = reader
            .read_event()
            .context("remove PPTX content-type overrides")?;
        match event {
            Event::Empty(override_entry) if override_entry.local_name().as_ref() == b"Override" => {
                let part_name = required_xml_attribute(&reader, &override_entry, "PartName")?;
                if removed_part_names.contains(part_name.as_str()) {
                    removed.insert(part_name);
                } else {
                    writer.write_event(Event::Empty(override_entry.into_owned()))?;
                }
            }
            Event::Start(override_entry) if override_entry.local_name().as_ref() == b"Override" => {
                return Err(anyhow!(
                    "PPTX content-type removal requires empty Override elements"
                ));
            }
            Event::Eof => {
                writer.write_event(Event::Eof)?;
                break;
            }
            event => writer.write_event(event.into_owned())?,
        }
    }
    if &removed != removed_part_names {
        return Err(anyhow!(
            "PPTX content types are missing an override selected for removal"
        ));
    }
    xml_output(writer, "updated PPTX content types")
}

fn append_content_type_entries(
    xml: &str,
    metadata: &ContentTypesMetadata,
    required_defaults: &HashMap<String, &'static str>,
    overrides: &[(String, &'static str)],
) -> Result<String> {
    let mut missing_defaults = required_defaults
        .iter()
        .filter_map(|(extension, content_type)| {
            if let Some(existing) = metadata.defaults.get(extension) {
                if existing != content_type {
                    return Some(Err(anyhow!(
                        "PPTX content type for .{extension} conflicts with appended image data"
                    )));
                }
                None
            } else {
                Some(Ok((extension.as_str(), *content_type)))
            }
        })
        .collect::<Result<Vec<_>>>()?;
    missing_defaults.sort_by_key(|(extension, _)| *extension);
    for (part_name, _) in overrides {
        if metadata.overrides.contains(part_name) {
            return Err(anyhow!(
                "PPTX content types already contain appended part name: {part_name}"
            ));
        }
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(
        xml.len()
            .saturating_add(missing_defaults.len().saturating_mul(96))
            .saturating_add(overrides.len().saturating_mul(180)),
    ));
    let mut defaults_inserted = missing_defaults.is_empty();
    let mut overrides_inserted = false;
    loop {
        let event = reader.read_event().context("rewrite PPTX content types")?;
        if !defaults_inserted
            && matches!(
                &event,
                Event::Start(start) | Event::Empty(start)
                    if start.local_name().as_ref() == b"Override"
            )
        {
            write_content_type_defaults(
                &mut writer,
                metadata.default_tag_name.as_str(),
                missing_defaults.as_slice(),
            )?;
            defaults_inserted = true;
        }
        match &event {
            Event::End(end) if end.local_name().as_ref() == b"Types" => {
                if !defaults_inserted {
                    write_content_type_defaults(
                        &mut writer,
                        metadata.default_tag_name.as_str(),
                        missing_defaults.as_slice(),
                    )?;
                    defaults_inserted = true;
                }
                if overrides_inserted {
                    return Err(anyhow!("PPTX content types contain duplicate roots"));
                }
                for (part_name, content_type) in overrides {
                    let mut entry = BytesStart::new(metadata.override_tag_name.as_str());
                    entry.push_attribute(("PartName", part_name.as_str()));
                    entry.push_attribute(("ContentType", *content_type));
                    writer.write_event(Event::Empty(entry))?;
                }
                overrides_inserted = true;
            }
            Event::Eof => {
                writer.write_event(event.into_owned())?;
                break;
            }
            _ => {}
        }
        writer.write_event(event.into_owned())?;
    }
    if !defaults_inserted || !overrides_inserted {
        return Err(anyhow!("PPTX content types are missing a writable root"));
    }
    xml_output(writer, "updated PPTX content types XML")
}

fn write_content_type_defaults(
    writer: &mut Writer<Vec<u8>>,
    tag_name: &str,
    defaults: &[(&str, &'static str)],
) -> Result<()> {
    for (extension, content_type) in defaults {
        let mut entry = BytesStart::new(tag_name);
        entry.push_attribute(("Extension", *extension));
        entry.push_attribute(("ContentType", *content_type));
        writer.write_event(Event::Empty(entry))?;
    }
    Ok(())
}

fn xml_output(writer: Writer<Vec<u8>>, label: &str) -> Result<String> {
    let bytes = writer.into_inner();
    if bytes.len() > super::MAX_XML_BYTES {
        return Err(anyhow!("{label} exceeds the local XML size limit"));
    }
    String::from_utf8(bytes).with_context(|| format!("encode {label}"))
}

fn next_relationship_id(used: &mut HashSet<String>) -> Result<String> {
    for number in 1..=100_000usize {
        let candidate = format!("rId{number}");
        if used.insert(candidate.clone()) {
            return Ok(candidate);
        }
    }
    Err(anyhow!(
        "PPTX relationship id space exceeds the conservative safety limit"
    ))
}

fn numbered_part(name: &str, prefix: &str, suffix: &str) -> Option<usize> {
    name.strip_prefix(prefix)?
        .strip_suffix(suffix)?
        .parse()
        .ok()
}

fn relationships_part_path(part: &str) -> Result<String> {
    let path = Path::new(part);
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("PPTX part has no relationship parent"))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("PPTX part name is not valid UTF-8"))?;
    let parent = parent
        .to_str()
        .ok_or_else(|| anyhow!("PPTX part parent is not valid UTF-8"))?;
    Ok(format!("{parent}/_rels/{file_name}.rels"))
}

fn relative_part_target(source_part: &str, target_part: &str) -> Result<String> {
    let source_parent = Path::new(source_part)
        .parent()
        .ok_or_else(|| anyhow!("PPTX source part has no parent"))?;
    let source = normal_part_components(source_parent)?;
    let target = normal_part_components(Path::new(target_part))?;
    let common = source
        .iter()
        .zip(target.iter())
        .take_while(|(left, right)| left == right)
        .count();
    let mut output = Vec::new();
    output.extend(std::iter::repeat_n("..".to_string(), source.len() - common));
    output.extend(target.into_iter().skip(common));
    if output.is_empty() {
        return Err(anyhow!("PPTX relationship target cannot be empty"));
    }
    Ok(output.join("/"))
}

fn normal_part_components(path: &Path) -> Result<Vec<String>> {
    path.components()
        .map(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .map(str::to_string)
                .ok_or_else(|| anyhow!("PPTX part path is not valid UTF-8")),
            _ => Err(anyhow!("PPTX part path contains unsafe components")),
        })
        .collect()
}

fn sibling_qualified_name(existing: &str, local_name: &str) -> String {
    existing.rsplit_once(':').map_or_else(
        || local_name.to_string(),
        |(prefix, _)| format!("{prefix}:{local_name}"),
    )
}

fn appended_slide_relationships(
    layout_target: &str,
    image_target: Option<&str>,
    chart_target: Option<&str>,
    notes_target: Option<&str>,
) -> String {
    let mut relationships = format!(
        "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout\" Target=\"{}\"/>",
        escape_xml(layout_target)
    );
    let mut next_id = 2usize;
    if let Some(image_target) = image_target {
        relationships.push_str(
            format!("<Relationship Id=\"rId{next_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"{}\"/>", escape_xml(image_target)).as_str(),
        );
        next_id += 1;
    }
    if let Some(chart_target) = chart_target {
        relationships.push_str(
            format!("<Relationship Id=\"rId{next_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart\" Target=\"{}\"/>", escape_xml(chart_target)).as_str(),
        );
        next_id += 1;
    }
    if let Some(notes_target) = notes_target {
        relationships.push_str(
            format!("<Relationship Id=\"rId{next_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide\" Target=\"{}\"/>", escape_xml(notes_target)).as_str(),
        );
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{relationships}</Relationships>"#
    )
}

fn appended_notes_slide_relationships(notes_master_target: &str, slide_target: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster" Target="{}"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="{}"/></Relationships>"#,
        escape_xml(notes_master_target),
        escape_xml(slide_target)
    )
}

pub(super) fn validate_pptx_for_render(path: &Path) -> Result<()> {
    let package_names = validate_pptx_package(path)?;
    for required in [
        "[Content_Types].xml",
        "_rels/.rels",
        "ppt/presentation.xml",
        "ppt/_rels/presentation.xml.rels",
    ] {
        if !package_names.contains(required) {
            return Err(anyhow!("PPTX rendering requires package part: {required}"));
        }
    }
    for name in &package_names {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with("vbaproject.bin")
            || lower.starts_with("ppt/activex/")
            || lower.starts_with("ppt/controls/")
            || lower.starts_with("ppt/embeddings/")
            || lower.starts_with("ppt/oleobjects/")
            || lower.starts_with("ppt/externallinks/")
            || lower.starts_with("ppt/webextensions/")
            || lower.starts_with("customui/")
        {
            return Err(anyhow!(
                "PPTX rendering rejects active, embedded, or externally connected presentation content: {name}"
            ));
        }
    }

    let mut archive = ZipArchive::new(File::open(path)?)
        .with_context(|| format!("open PPTX {} for render validation", path.display()))?;
    let content_types = read_zip_text(&mut archive, "[Content_Types].xml")?;
    let normalized_content_types = content_types.to_ascii_lowercase();
    if [
        "macroenabled",
        "vbaproject",
        "activex",
        "oleobject",
        "externaldata",
        "externallink",
        "webextension",
        "customui",
    ]
    .iter()
    .any(|token| normalized_content_types.contains(token))
    {
        return Err(anyhow!(
            "PPTX rendering rejects active, embedded, or externally connected content types"
        ));
    }

    let mut names = package_names.into_iter().collect::<Vec<_>>();
    names.sort();
    let names_set = names.iter().map(String::as_str).collect::<HashSet<_>>();
    for name in names.iter().filter(|name| name.ends_with(".rels")) {
        let source_part = relationship_source_part_for_render(name.as_str())?;
        let relationships = read_zip_text(&mut archive, name.as_str())?;
        let relationships = parse_relationship_document(relationships.as_str(), &source_part)?;
        for relationship in relationships.relationships {
            let relationship_type = relationship.relationship_type.to_ascii_lowercase();
            if relationship_type.ends_with("/oleobject")
                || relationship_type.ends_with("/package")
                || relationship_type.ends_with("/activex")
                || relationship_type.ends_with("/control")
                || relationship_type.ends_with("/externallink")
                || relationship_type.ends_with("/externaldata")
                || relationship_type.ends_with("/vbaproject")
                || relationship_type.contains("/webextension")
                || relationship_type.contains("/customui")
                || relationship_type.ends_with("/attachedtemplate")
            {
                return Err(anyhow!(
                    "PPTX rendering rejects active, embedded, or externally connected relationships"
                ));
            }
            if relationship.external {
                if !relationship_type.ends_with("/hyperlink") {
                    return Err(anyhow!(
                        "PPTX rendering rejects external non-hyperlink relationships"
                    ));
                }
                continue;
            }
            let target = resolve_part_target(&source_part, relationship.target.as_str())?;
            if !names_set.contains(target.as_str()) {
                return Err(anyhow!(
                    "PPTX rendering found a missing internal relationship target: {target}"
                ));
            }
        }
    }
    Ok(())
}

fn relationship_source_part_for_render(relationships_path: &str) -> Result<String> {
    if relationships_path == "_rels/.rels" {
        return Ok("package.xml".to_string());
    }
    let (parent, file) = relationships_path
        .split_once("/_rels/")
        .ok_or_else(|| anyhow!("PPTX relationship part path is invalid"))?;
    let source_file = file
        .strip_suffix(".rels")
        .ok_or_else(|| anyhow!("PPTX relationship part suffix is invalid"))?;
    if parent.is_empty() || source_file.is_empty() {
        return Err(anyhow!("PPTX relationship source part is invalid"));
    }
    Ok(format!("{parent}/{source_file}"))
}

fn validate_pptx_package(path: &Path) -> Result<HashSet<String>> {
    let mut archive = ZipArchive::new(File::open(path)?)
        .with_context(|| format!("open PPTX {}", path.display()))?;
    if archive.is_empty() || archive.len() > MAX_PPTX_ZIP_ENTRIES {
        return Err(anyhow!(
            "PPTX ZIP must contain between 1 and {MAX_PPTX_ZIP_ENTRIES} entries"
        ));
    }
    let mut names = HashSet::new();
    let mut expanded = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if entry.enclosed_name().is_none() || entry.is_symlink() || !names.insert(name) {
            return Err(anyhow!("PPTX ZIP contains an unsafe or duplicate entry"));
        }
        expanded = expanded.saturating_add(entry.size());
        if expanded > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "PPTX ZIP exceeds the 100 MiB expanded safety limit"
            ));
        }
    }
    Ok(names)
}

fn slide_part_number(name: &str) -> Option<usize> {
    name.strip_prefix("ppt/slides/slide")?
        .strip_suffix(".xml")?
        .parse()
        .ok()
}

fn presentation_slide_size(xml: &str) -> Result<(u64, u64)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    loop {
        match reader.read_event().context("parse PPTX presentation XML")? {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"sldSz" =>
            {
                let cx = required_xml_attribute(&reader, &event, "cx")?.parse()?;
                let cy = required_xml_attribute(&reader, &event, "cy")?.parse()?;
                return Ok((cx, cy));
            }
            Event::Eof => return Err(anyhow!("PPTX presentation is missing slide size")),
            _ => {}
        }
    }
}

fn drawing_text_runs(xml: &str, max_chars: usize) -> Result<Vec<String>> {
    let mut runs = Vec::new();
    let mut cursor = 0usize;
    let mut chars = 0usize;
    while let Some(start) = find_next_pptx_xml_tag_start(xml, "<a:t", cursor) {
        let Some(open_end) = xml[start..].find('>') else {
            return Err(anyhow!("PPTX text run has an invalid opening tag"));
        };
        let content_start = start + open_end + 1;
        let Some(end) = xml[content_start..].find("</a:t>") else {
            return Err(anyhow!("PPTX text run has an invalid closing tag"));
        };
        let value = unescape_xml(&xml[content_start..content_start + end]);
        chars = chars.saturating_add(value.chars().count());
        if chars > max_chars {
            runs.push(
                value
                    .chars()
                    .take(max_chars.saturating_sub(chars - value.chars().count()))
                    .collect(),
            );
            break;
        }
        runs.push(value);
        cursor = content_start + end + "</a:t>".len();
    }
    Ok(runs)
}

fn standard_pptx_chart_parts(names: &HashSet<String>) -> Result<HashSet<String>> {
    let mut parts = HashSet::new();
    for name in names {
        let Some(suffix) = name
            .strip_prefix("ppt/charts/chart")
            .and_then(|value| value.strip_suffix(".xml"))
        else {
            continue;
        };
        if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(anyhow!(
                "PPTX contains a nonstandard chart part name: {name}"
            ));
        }
        parts.insert(name.clone());
    }
    Ok(parts)
}

fn resolve_standard_pptx_chart_references(
    slide_xml: &str,
    relationships_xml: Option<&str>,
    slide_path: &str,
    names: &HashSet<String>,
) -> Result<Vec<ResolvedPptxChartReference>> {
    let relationship_ids = standard_pptx_slide_chart_relationship_ids(slide_xml)?;
    let Some(relationships_xml) = relationships_xml else {
        if relationship_ids.is_empty() {
            return Ok(Vec::new());
        }
        return Err(anyhow!(
            "PPTX slide chart references require a relationship part"
        ));
    };
    let relationships = parse_relationship_document(relationships_xml, slide_path)?;
    let chart_relationships = relationships
        .relationships
        .iter()
        .filter(|relationship| relationship.relationship_type.ends_with("/chart"))
        .collect::<Vec<_>>();
    if chart_relationships.len() != relationship_ids.len() {
        return Err(anyhow!(
            "PPTX slide chart relationships do not exactly match standard chart references"
        ));
    }
    let relationships_by_id = relationships
        .relationships
        .iter()
        .map(|relationship| (relationship.id.as_str(), relationship))
        .collect::<HashMap<_, _>>();
    let expected_ids = relationship_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let actual_ids = chart_relationships
        .iter()
        .map(|relationship| relationship.id.as_str())
        .collect::<HashSet<_>>();
    if expected_ids != actual_ids {
        return Err(anyhow!(
            "PPTX slide chart relationship ids do not match visible chart references"
        ));
    }
    let mut parts = HashSet::new();
    let mut resolved = Vec::with_capacity(relationship_ids.len());
    for relationship_id in relationship_ids {
        let relationship = relationships_by_id
            .get(relationship_id.as_str())
            .ok_or_else(|| anyhow!("PPTX chart references a missing relationship"))?;
        if relationship.external || !relationship.relationship_type.ends_with("/chart") {
            return Err(anyhow!(
                "PPTX standard chart relationship must be internal and use the chart relationship type"
            ));
        }
        let part = resolve_part_target(slide_path, relationship.target.as_str())?;
        let standard_part = part
            .strip_prefix("ppt/charts/chart")
            .and_then(|value| value.strip_suffix(".xml"))
            .is_some_and(|value| {
                !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
            });
        if !standard_part || !names.contains(part.as_str()) || !parts.insert(part.clone()) {
            return Err(anyhow!(
                "PPTX slide chart relationship must resolve to one unique standard chart part"
            ));
        }
        resolved.push(ResolvedPptxChartReference {
            relationship_id,
            part,
        });
    }
    Ok(resolved)
}

fn standard_pptx_slide_chart_relationship_ids(xml: &str) -> Result<Vec<String>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut stack = Vec::<(String, Option<String>)>::new();
    let mut relationship_ids = Vec::new();
    let mut seen = HashSet::new();
    let mut root_count = 0usize;
    loop {
        match reader
            .read_event()
            .context("inspect PPTX slide chart references")?
        {
            Event::Start(event) => {
                let qualified = String::from_utf8_lossy(event.name().as_ref()).into_owned();
                if stack.is_empty() {
                    if qualified != "p:sld" {
                        return Err(anyhow!(
                            "PPTX chart inspection requires a standard p:sld root"
                        ));
                    }
                    root_count = root_count.saturating_add(1);
                }
                if event.local_name().as_ref() == b"chart" {
                    return Err(anyhow!(
                        "PPTX chart references must use an empty standard c:chart element"
                    ));
                }
                let chart_uri = if event.local_name().as_ref() == b"graphicData" {
                    if qualified != "a:graphicData" {
                        return Err(anyhow!(
                            "PPTX chart graphicData must use the standard a namespace"
                        ));
                    }
                    optional_xml_attribute(&reader, &event, "uri")?
                } else {
                    None
                };
                stack.push((qualified, chart_uri));
                if stack.len() > 256 {
                    return Err(anyhow!(
                        "PPTX slide chart reference nesting exceeds the safety limit"
                    ));
                }
            }
            Event::Empty(event) if event.local_name().as_ref() == b"chart" => {
                if event.name().as_ref() != b"c:chart" {
                    return Err(anyhow!(
                        "PPTX chart references must use the standard c namespace"
                    ));
                }
                let in_standard_graphic_data = stack.iter().rev().any(|(name, uri)| {
                    name == "a:graphicData"
                        && uri.as_deref()
                            == Some("http://schemas.openxmlformats.org/drawingml/2006/chart")
                });
                if !in_standard_graphic_data {
                    return Err(anyhow!(
                        "PPTX c:chart must be inside standard chart graphicData"
                    ));
                }
                let mut relationship_id = None;
                let mut relationship_attribute_count = 0usize;
                for attribute in event.attributes().with_checks(false) {
                    let attribute = attribute.context("parse PPTX chart reference attribute")?;
                    let value = attribute
                        .decoded_and_normalized_value(XmlVersion::Explicit1_0, reader.decoder())?
                        .into_owned();
                    if attribute.key.as_ref() == b"r:id" {
                        relationship_attribute_count =
                            relationship_attribute_count.saturating_add(1);
                        relationship_id = Some(value);
                    } else if (attribute.key.as_ref() == b"xmlns:c"
                        && value
                            == "http://schemas.openxmlformats.org/drawingml/2006/chart")
                        || (attribute.key.as_ref() == b"xmlns:r"
                            && value
                                == "http://schemas.openxmlformats.org/officeDocument/2006/relationships")
                    {
                    } else {
                        return Err(anyhow!(
                            "PPTX chart reference contains an unsupported attribute"
                        ));
                    }
                }
                if relationship_attribute_count != 1 {
                    return Err(anyhow!(
                        "PPTX chart reference must contain exactly one r:id attribute"
                    ));
                }
                let relationship_id = relationship_id
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| anyhow!("PPTX chart reference is missing r:id"))?;
                if !seen.insert(relationship_id.clone()) {
                    return Err(anyhow!(
                        "PPTX slide contains duplicate standard chart references"
                    ));
                }
                relationship_ids.push(relationship_id);
                if relationship_ids.len() > MAX_PPTX_CHARTS_PER_SLIDE {
                    return Err(anyhow!(
                        "PPTX slide charts exceed the {MAX_PPTX_CHARTS_PER_SLIDE} item safety limit"
                    ));
                }
            }
            Event::Empty(event) => {
                if event.local_name().as_ref() == b"graphicData"
                    && event.name().as_ref() != b"a:graphicData"
                {
                    return Err(anyhow!(
                        "PPTX chart graphicData must use the standard a namespace"
                    ));
                }
            }
            Event::End(event) => {
                let expected = stack
                    .pop()
                    .ok_or_else(|| anyhow!("PPTX slide contains an unmatched closing tag"))?;
                if expected.0.as_bytes() != event.name().as_ref() {
                    return Err(anyhow!("PPTX slide contains mismatched element boundaries"));
                }
            }
            Event::DocType(_) | Event::PI(_) | Event::CData(_) => {
                return Err(anyhow!(
                    "PPTX chart inspection does not support slide declarations, processing instructions, or CDATA"
                ));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if root_count != 1 || !stack.is_empty() {
        return Err(anyhow!(
            "PPTX chart inspection requires one complete standard slide root"
        ));
    }
    Ok(relationship_ids)
}

fn ensure_standard_pptx_chart_content_type(xml: &str, chart_part: &str) -> Result<()> {
    let expected_part_name = format!("/{chart_part}");
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut matches = 0usize;
    loop {
        match reader
            .read_event()
            .context("inspect PPTX chart content type")?
        {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"Override" =>
            {
                if required_xml_attribute(&reader, &event, "PartName")? == expected_part_name {
                    matches = matches.saturating_add(1);
                    if required_xml_attribute(&reader, &event, "ContentType")?
                        != "application/vnd.openxmlformats-officedocument.drawingml.chart+xml"
                    {
                        return Err(anyhow!("PPTX chart part has an unexpected content type"));
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if matches != 1 {
        return Err(anyhow!(
            "PPTX standard chart part must have exactly one chart content-type override"
        ));
    }
    Ok(())
}

fn inspect_pptx_chart_ownership(
    archive: &mut ZipArchive<File>,
    names: &HashSet<String>,
) -> Result<PptxChartOwnership> {
    if names
        .iter()
        .any(|name| name.starts_with("ppt/charts/chartEx") && name.ends_with(".xml"))
    {
        return Err(anyhow!(
            "PPTX chart inspection does not support chartEx parts"
        ));
    }
    let presentation_xml = read_zip_text(archive, "ppt/presentation.xml")?;
    let presentation_relationships_xml = read_zip_text(archive, "ppt/_rels/presentation.xml.rels")?;
    let slide_metadata = presentation_slide_metadata(presentation_xml.as_str())?;
    let presentation_relationships = parse_relationship_document(
        presentation_relationships_xml.as_str(),
        "ppt/presentation.xml",
    )?;
    let ordered_slide_paths =
        ordered_presentation_slide_paths(&slide_metadata, &presentation_relationships, names)?;
    ensure_all_slide_parts_are_referenced(&ordered_slide_paths, names)?;
    if ordered_slide_paths.is_empty() || ordered_slide_paths.len() > 1_000 {
        return Err(anyhow!(
            "PPTX slide count is outside the chart inspection safety limit"
        ));
    }

    let mut charts_by_slide = Vec::with_capacity(ordered_slide_paths.len());
    let mut chart_owners = HashMap::<String, (usize, String)>::new();
    let mut chart_count = 0usize;
    for (slide_index, slide_path) in ordered_slide_paths.iter().enumerate() {
        let slide_xml = read_zip_text(archive, slide_path.as_str())?;
        let relationships_path = relationships_part_path(slide_path.as_str())?;
        let relationships_xml = if names.contains(relationships_path.as_str()) {
            Some(read_zip_text(archive, relationships_path.as_str())?)
        } else {
            None
        };
        let references = resolve_standard_pptx_chart_references(
            slide_xml.as_str(),
            relationships_xml.as_deref(),
            slide_path.as_str(),
            names,
        )?;
        chart_count = chart_count
            .checked_add(references.len())
            .ok_or_else(|| anyhow!("PPTX chart count overflow"))?;
        if chart_count > MAX_PPTX_CHARTS_TOTAL {
            return Err(anyhow!(
                "PPTX charts exceed the {MAX_PPTX_CHARTS_TOTAL} item safety limit"
            ));
        }
        for reference in &references {
            if let Some((owner_index, owner_relationship)) = chart_owners.insert(
                reference.part.clone(),
                (slide_index + 1, reference.relationship_id.clone()),
            ) {
                return Err(anyhow!(
                    "PPTX chart part is shared by slide {owner_index} relationship {owner_relationship} and another visible slide"
                ));
            }
        }
        charts_by_slide.push(references);
    }
    let package_chart_parts = standard_pptx_chart_parts(names)?;
    let referenced_chart_parts = chart_owners.keys().cloned().collect::<HashSet<_>>();
    if package_chart_parts != referenced_chart_parts {
        return Err(anyhow!(
            "PPTX contains unreferenced or missing standard chart parts"
        ));
    }
    Ok(PptxChartOwnership {
        ordered_slide_paths,
        charts_by_slide,
        chart_count,
    })
}

fn inspect_pptx_chart_relationships(
    archive: &mut ZipArchive<File>,
    names: &HashSet<String>,
    chart_part: &str,
) -> Result<PptxChartRelationshipInspection> {
    let relationships_path = relationships_part_path(chart_part)?;
    if !names.contains(relationships_path.as_str()) {
        return Ok(PptxChartRelationshipInspection {
            data_source: "cached_only",
            relationship_count: 0,
            embedded_workbook: None,
            relationships_part_present: false,
        });
    }
    let relationships_xml = read_zip_text(archive, relationships_path.as_str())?;
    let relationships = parse_relationship_document(relationships_xml.as_str(), chart_part)?;
    let mut embedded_workbook = None;
    for relationship in &relationships.relationships {
        if relationship.external {
            return Err(anyhow!(
                "PPTX chart inspection refuses external chart relationships"
            ));
        }
        let target = resolve_part_target(chart_part, relationship.target.as_str())?;
        if !names.contains(target.as_str()) {
            return Err(anyhow!(
                "PPTX chart relationship references a missing package part"
            ));
        }
        if relationship.relationship_type.ends_with("/package")
            && (!target.starts_with("ppt/embeddings/")
                || embedded_workbook.replace(target).is_some())
        {
            return Err(anyhow!(
                "PPTX chart must reference at most one internal embedded workbook"
            ));
        }
    }
    Ok(PptxChartRelationshipInspection {
        data_source: if embedded_workbook.is_some() {
            "cached_with_embedded_workbook"
        } else {
            "cached_only"
        },
        relationship_count: relationships.relationships.len(),
        embedded_workbook,
        relationships_part_present: true,
    })
}

fn inspect_standard_pptx_chart_xml(xml: &str) -> Result<PptxChartInspection> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut stack = Vec::<String>::new();
    let mut root_count = 0usize;
    let mut chart_count = 0usize;
    let mut plot_area_count = 0usize;
    let mut chart_types = BTreeSet::<String>::new();
    let mut chart_groups = Vec::<PptxChartGroupInspection>::new();
    let mut current_chart_group = None::<PptxChartGroupInspection>;
    let mut axes = Vec::<PptxChartAxisInspection>::new();
    let mut current_axis = None::<PptxChartAxisInspection>;
    let mut title = String::new();
    let mut title_formula = None;
    let mut series = Vec::<PptxChartSeriesInspection>::new();
    let mut current_series = None::<PptxChartSeriesInspection>;
    let mut cached_points = 0usize;
    let mut legend_count = 0usize;
    let mut legend_positions = Vec::<String>::new();
    let mut data_label_group_count = 0usize;
    let mut data_label_show_value_count = 0usize;
    let mut data_label_show_percentage_count = 0usize;
    let mut total_chars = 0usize;
    loop {
        match reader.read_event().context("inspect PPTX chart XML")? {
            Event::Start(event) => {
                let qualified = event.name().as_ref().to_vec();
                let local = String::from_utf8_lossy(event.local_name().as_ref()).into_owned();
                ensure_standard_pptx_chart_namespace(qualified.as_slice(), local.as_str())?;
                if stack.is_empty() {
                    if qualified.as_slice() != b"c:chartSpace" {
                        return Err(anyhow!(
                            "PPTX chart part requires a standard c:chartSpace root"
                        ));
                    }
                    root_count = root_count.saturating_add(1);
                }
                if qualified.as_slice() == b"c:chart" {
                    chart_count = chart_count.saturating_add(1);
                } else if qualified.as_slice() == b"c:plotArea" {
                    plot_area_count = plot_area_count.saturating_add(1);
                } else if qualified.as_slice() == b"c:legend" {
                    legend_count = legend_count.saturating_add(1);
                } else if qualified.as_slice() == b"c:dLbls" {
                    data_label_group_count = data_label_group_count.saturating_add(1);
                } else if qualified.as_slice() == b"c:legendPos" {
                    if stack.last().map(String::as_str) != Some("legend") {
                        return Err(anyhow!(
                            "PPTX chart legend position must be a direct legend child"
                        ));
                    }
                    legend_positions.push(required_xml_attribute(&reader, &event, "val")?);
                } else if qualified.as_slice() == b"c:showVal"
                    && stack.last().map(String::as_str) == Some("dLbls")
                    && pptx_chart_boolean_attribute(&reader, &event, "showVal")?
                {
                    data_label_show_value_count = data_label_show_value_count.saturating_add(1);
                } else if qualified.as_slice() == b"c:showPercent"
                    && stack.last().map(String::as_str) == Some("dLbls")
                    && pptx_chart_boolean_attribute(&reader, &event, "showPercent")?
                {
                    data_label_show_percentage_count =
                        data_label_show_percentage_count.saturating_add(1);
                }
                if let Some(chart_type) = standard_pptx_chart_type(qualified.as_slice()) {
                    chart_types.insert(chart_type.to_string());
                    if current_chart_group.is_some() {
                        return Err(anyhow!("PPTX chart contains nested chart groups"));
                    }
                    current_chart_group = Some(PptxChartGroupInspection {
                        chart_type: chart_type.to_string(),
                        axis_ids: Vec::new(),
                    });
                }
                if qualified.as_slice() == b"c:catAx" || qualified.as_slice() == b"c:valAx" {
                    if current_axis.is_some() {
                        return Err(anyhow!("PPTX chart contains nested axes"));
                    }
                    current_axis = Some(PptxChartAxisInspection {
                        axis_type: if qualified.as_slice() == b"c:catAx" {
                            "category".to_string()
                        } else {
                            "value".to_string()
                        },
                        ..PptxChartAxisInspection::default()
                    });
                }
                record_pptx_chart_structure_element(
                    &reader,
                    &event,
                    stack.as_slice(),
                    current_chart_group.as_mut(),
                    current_axis.as_mut(),
                )?;
                if qualified.as_slice() == b"c:ser" {
                    if current_series.is_some() {
                        return Err(anyhow!("PPTX chart contains nested series"));
                    }
                    let chart_group = current_chart_group
                        .as_ref()
                        .ok_or_else(|| anyhow!("PPTX chart series has no supported chart group"))?;
                    current_series = Some(PptxChartSeriesInspection {
                        chart_type: chart_group.chart_type.clone(),
                        chart_group_index: chart_groups.len(),
                        ..PptxChartSeriesInspection::default()
                    });
                }
                record_pptx_chart_series_color_element(
                    &reader,
                    &event,
                    stack.as_slice(),
                    current_series.as_mut(),
                    false,
                )?;
                record_pptx_chart_series_marker_element(
                    &reader,
                    &event,
                    stack.as_slice(),
                    current_series.as_mut(),
                    false,
                )?;
                record_pptx_chart_series_smooth_element(
                    &reader,
                    &event,
                    stack.as_slice(),
                    current_series.as_mut(),
                    false,
                )?;
                stack.push(local);
                if stack.len() > 256 {
                    return Err(anyhow!("PPTX chart XML nesting exceeds the safety limit"));
                }
            }
            Event::Empty(event) => {
                let qualified = event.name().as_ref().to_vec();
                let local = String::from_utf8_lossy(event.local_name().as_ref()).into_owned();
                ensure_standard_pptx_chart_namespace(qualified.as_slice(), local.as_str())?;
                if standard_pptx_chart_type(qualified.as_slice()).is_some()
                    || qualified.as_slice() == b"c:ser"
                {
                    return Err(anyhow!(
                        "PPTX chart types and series must not be empty elements"
                    ));
                }
                if qualified.as_slice() == b"c:legend" {
                    legend_count = legend_count.saturating_add(1);
                } else if qualified.as_slice() == b"c:dLbls" {
                    data_label_group_count = data_label_group_count.saturating_add(1);
                } else if qualified.as_slice() == b"c:legendPos" {
                    if stack.last().map(String::as_str) != Some("legend") {
                        return Err(anyhow!(
                            "PPTX chart legend position must be a direct legend child"
                        ));
                    }
                    legend_positions.push(required_xml_attribute(&reader, &event, "val")?);
                } else if qualified.as_slice() == b"c:showVal"
                    && stack.last().map(String::as_str) == Some("dLbls")
                    && pptx_chart_boolean_attribute(&reader, &event, "showVal")?
                {
                    data_label_show_value_count = data_label_show_value_count.saturating_add(1);
                } else if qualified.as_slice() == b"c:showPercent"
                    && stack.last().map(String::as_str) == Some("dLbls")
                    && pptx_chart_boolean_attribute(&reader, &event, "showPercent")?
                {
                    data_label_show_percentage_count =
                        data_label_show_percentage_count.saturating_add(1);
                }
                record_pptx_chart_structure_element(
                    &reader,
                    &event,
                    stack.as_slice(),
                    current_chart_group.as_mut(),
                    current_axis.as_mut(),
                )?;
                record_pptx_chart_series_color_element(
                    &reader,
                    &event,
                    stack.as_slice(),
                    current_series.as_mut(),
                    true,
                )?;
                record_pptx_chart_series_marker_element(
                    &reader,
                    &event,
                    stack.as_slice(),
                    current_series.as_mut(),
                    true,
                )?;
                record_pptx_chart_series_smooth_element(
                    &reader,
                    &event,
                    stack.as_slice(),
                    current_series.as_mut(),
                    true,
                )?;
            }
            Event::Text(text) => {
                let value = text
                    .xml_content(XmlVersion::Explicit1_0)
                    .context("decode PPTX chart text")?
                    .into_owned();
                let Some(current) = stack.last().map(String::as_str) else {
                    if !value.trim().is_empty() {
                        return Err(anyhow!("PPTX chart contains text outside its root"));
                    }
                    continue;
                };
                if let Some(series) = current_series.as_mut() {
                    if pptx_chart_series_shape_properties_index(stack.as_slice()).is_some()
                        && !value.trim().is_empty()
                    {
                        series.color_style_custom = true;
                    }
                    if pptx_chart_series_marker_index(stack.as_slice()).is_some()
                        && !value.trim().is_empty()
                    {
                        series.marker_style_custom = true;
                    }
                }
                if current == "t"
                    && current_series.is_none()
                    && pptx_chart_is_chart_level_title(stack.as_slice())
                {
                    total_chars = add_pptx_chart_text_chars(total_chars, value.as_str())?;
                    title.push_str(value.as_str());
                } else if current == "t" && current_series.is_none() {
                    if let Some(axis_type) = pptx_chart_axis_title_context(stack.as_slice()) {
                        total_chars = add_pptx_chart_text_chars(total_chars, value.as_str())?;
                        let axis = current_axis.as_mut().ok_or_else(|| {
                            anyhow!("PPTX chart axis title is outside a category or value axis")
                        })?;
                        if axis.axis_type != axis_type {
                            return Err(anyhow!("PPTX chart axis title context is inconsistent"));
                        }
                        axis.title.push_str(value.as_str());
                    }
                } else if current == "f" {
                    let formula = value.trim();
                    if formula.is_empty() || formula.chars().count() > MAX_PPTX_CHART_FORMULA_CHARS
                    {
                        return Err(anyhow!(
                            "PPTX chart formula is empty or exceeds the safety limit"
                        ));
                    }
                    total_chars = add_pptx_chart_text_chars(total_chars, formula)?;
                    if current_series.is_none() && pptx_chart_is_chart_level_title(stack.as_slice())
                    {
                        set_unique_pptx_chart_formula(&mut title_formula, formula)?;
                        continue;
                    }
                    if current_series.is_none() {
                        if let Some(axis_type) = pptx_chart_axis_title_context(stack.as_slice()) {
                            let axis = current_axis.as_mut().ok_or_else(|| {
                                anyhow!(
                                    "PPTX chart axis title formula is outside a category or value axis"
                                )
                            })?;
                            if axis.axis_type != axis_type {
                                return Err(anyhow!(
                                    "PPTX chart axis title formula context is inconsistent"
                                ));
                            }
                            set_unique_pptx_chart_formula(&mut axis.title_formula, formula)?;
                        }
                        continue;
                    }
                    let context =
                        pptx_chart_series_data_context(stack.as_slice()).ok_or_else(|| {
                            anyhow!("PPTX chart formula is outside a supported series field")
                        })?;
                    let series = current_series
                        .as_mut()
                        .ok_or_else(|| anyhow!("PPTX chart formula is outside a series"))?;
                    let slot = match context {
                        "tx" => &mut series.name_formula,
                        "cat" | "xVal" => &mut series.category_formula,
                        "val" | "yVal" => &mut series.value_formula,
                        "bubbleSize" => &mut series.bubble_size_formula,
                        _ => unreachable!("validated chart series context"),
                    };
                    set_unique_pptx_chart_formula(slot, formula)?;
                } else if current == "v" {
                    if current_series.is_none() && pptx_chart_is_chart_level_title(stack.as_slice())
                    {
                        total_chars = add_pptx_chart_text_chars(total_chars, value.as_str())?;
                        title.push_str(value.as_str());
                        continue;
                    }
                    if current_series.is_none() {
                        if let Some(axis_type) = pptx_chart_axis_title_context(stack.as_slice()) {
                            total_chars = add_pptx_chart_text_chars(total_chars, value.as_str())?;
                            let axis = current_axis.as_mut().ok_or_else(|| {
                                anyhow!(
                                    "PPTX chart cached axis title is outside a category or value axis"
                                )
                            })?;
                            if axis.axis_type != axis_type {
                                return Err(anyhow!(
                                    "PPTX chart cached axis title context is inconsistent"
                                ));
                            }
                            axis.title.push_str(value.as_str());
                        }
                        continue;
                    }
                    let context =
                        pptx_chart_series_data_context(stack.as_slice()).ok_or_else(|| {
                            anyhow!("PPTX chart cache value is outside a supported series field")
                        })?;
                    total_chars = add_pptx_chart_text_chars(total_chars, value.as_str())?;
                    let series = current_series
                        .as_mut()
                        .ok_or_else(|| anyhow!("PPTX chart cache value is outside a series"))?;
                    match context {
                        "tx" => {
                            if !series.name.is_empty() {
                                return Err(anyhow!(
                                    "PPTX chart series contains multiple cached names"
                                ));
                            }
                            series.name = value;
                        }
                        "cat" | "xVal" => {
                            series.categories.push(value);
                            cached_points = cached_points.saturating_add(1);
                        }
                        "val" | "yVal" => {
                            series.values.push(value);
                            cached_points = cached_points.saturating_add(1);
                        }
                        "bubbleSize" => {
                            series.bubble_sizes.push(value);
                            cached_points = cached_points.saturating_add(1);
                        }
                        _ => unreachable!("validated chart series context"),
                    }
                    if cached_points > MAX_PPTX_CHART_POINTS {
                        return Err(anyhow!(
                            "PPTX chart cached points exceed the {MAX_PPTX_CHART_POINTS} item safety limit"
                        ));
                    }
                }
            }
            Event::End(event) => {
                let local = String::from_utf8_lossy(event.local_name().as_ref()).into_owned();
                let expected = stack
                    .pop()
                    .ok_or_else(|| anyhow!("PPTX chart contains an unmatched closing tag"))?;
                if expected != local {
                    return Err(anyhow!("PPTX chart contains mismatched element boundaries"));
                }
                if event.name().as_ref() == b"c:ser" {
                    let mut item = current_series
                        .take()
                        .ok_or_else(|| anyhow!("PPTX chart series boundary is invalid"))?;
                    finalize_pptx_chart_series_color(&mut item);
                    finalize_pptx_chart_series_marker(&mut item);
                    finalize_pptx_chart_series_smooth(&mut item);
                    series.push(item);
                    if series.len() > MAX_PPTX_CHART_SERIES {
                        return Err(anyhow!(
                            "PPTX chart series exceed the {MAX_PPTX_CHART_SERIES} item safety limit"
                        ));
                    }
                }
                if event.name().as_ref() == b"c:catAx" || event.name().as_ref() == b"c:valAx" {
                    let mut axis = current_axis
                        .take()
                        .ok_or_else(|| anyhow!("PPTX chart axis boundary is invalid"))?;
                    axis.title_truncated = axis.title.chars().count() > 1_000;
                    axis.title = axis.title.chars().take(1_000).collect();
                    axes.push(axis);
                    if axes.len() > 16 {
                        return Err(anyhow!("PPTX chart axes exceed the safety limit"));
                    }
                }
                if let Some(chart_type) = standard_pptx_chart_type(event.name().as_ref()) {
                    let chart_group = current_chart_group
                        .take()
                        .ok_or_else(|| anyhow!("PPTX chart group boundary is invalid"))?;
                    if chart_group.chart_type != chart_type {
                        return Err(anyhow!("PPTX chart group type boundary is inconsistent"));
                    }
                    chart_groups.push(chart_group);
                    if chart_groups.len() > 16 {
                        return Err(anyhow!("PPTX chart groups exceed the safety limit"));
                    }
                }
            }
            Event::DocType(_) | Event::PI(_) | Event::CData(_) => {
                return Err(anyhow!(
                    "PPTX chart inspection does not support declarations, processing instructions, or CDATA"
                ));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if root_count != 1
        || chart_count != 1
        || plot_area_count != 1
        || chart_types.is_empty()
        || current_series.is_some()
        || current_chart_group.is_some()
        || current_axis.is_some()
        || !stack.is_empty()
    {
        return Err(anyhow!(
            "PPTX chart must contain one complete chart, one plot area, and at least one supported chart type"
        ));
    }
    for item in &mut series {
        item.value_axis =
            resolve_pptx_chart_series_value_axis(item, chart_groups.as_slice(), axes.as_slice());
    }
    let title_truncated = title.chars().count() > 1_000;
    let category_axis = axes
        .iter()
        .find(|axis| axis.axis_type == "category" && axis.position.as_deref() == Some("b"))
        .or_else(|| axes.iter().find(|axis| axis.axis_type == "category"));
    let value_axis = axes
        .iter()
        .find(|axis| axis.axis_type == "value" && axis.position.as_deref() == Some("l"))
        .or_else(|| {
            axes.iter()
                .find(|axis| axis.axis_type == "value" && axis.position.as_deref() != Some("r"))
        })
        .or_else(|| axes.iter().find(|axis| axis.axis_type == "value"));
    let secondary_value_axis = axes
        .iter()
        .find(|axis| axis.axis_type == "value" && axis.position.as_deref() == Some("r"));
    let (category_axis_title, category_axis_title_formula, category_axis_title_truncated) =
        pptx_chart_axis_title_fields(category_axis);
    let (value_axis_title, value_axis_title_formula, value_axis_title_truncated) =
        pptx_chart_axis_title_fields(value_axis);
    let (
        secondary_value_axis_title,
        secondary_value_axis_title_formula,
        secondary_value_axis_title_truncated,
    ) = pptx_chart_axis_title_fields(secondary_value_axis);
    Ok(PptxChartInspection {
        chart_types: chart_types.into_iter().collect(),
        chart_groups,
        axes,
        title: title.chars().take(1_000).collect(),
        title_formula,
        title_truncated,
        series,
        cached_points,
        legend_count,
        legend_positions,
        data_label_group_count,
        data_label_show_value_count,
        data_label_show_percentage_count,
        category_axis_title,
        category_axis_title_formula,
        category_axis_title_truncated,
        value_axis_title,
        value_axis_title_formula,
        value_axis_title_truncated,
        secondary_value_axis_title,
        secondary_value_axis_title_formula,
        secondary_value_axis_title_truncated,
    })
}

fn record_pptx_chart_structure_element(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    stack: &[String],
    chart_group: Option<&mut PptxChartGroupInspection>,
    axis: Option<&mut PptxChartAxisInspection>,
) -> Result<()> {
    let qualified = event.name().as_ref().to_vec();
    let parent = stack.last().map(String::as_str);
    if qualified.as_slice() == b"c:axId" {
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

fn record_pptx_chart_series_color_element(
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
        "line" => match (relative_ancestors, local.as_slice()) {
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

fn pptx_chart_series_shape_properties_index(stack: &[String]) -> Option<usize> {
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

fn finalize_pptx_chart_series_color(series: &mut PptxChartSeriesInspection) {
    if !series.color_style_present {
        return;
    }
    let expected_line_count = usize::from(series.chart_type == "line");
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

fn record_pptx_chart_series_marker_element(
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

fn pptx_chart_series_marker_index(stack: &[String]) -> Option<usize> {
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

fn finalize_pptx_chart_series_marker(series: &mut PptxChartSeriesInspection) {
    if series.chart_type != "line" {
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

fn record_pptx_chart_series_smooth_element(
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

fn finalize_pptx_chart_series_smooth(series: &mut PptxChartSeriesInspection) {
    if series.chart_type == "line" {
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

fn resolve_pptx_chart_series_value_axis(
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
    match positions.iter().next().copied() {
        Some("l") => "primary".to_string(),
        Some("r") => "secondary".to_string(),
        _ => "unknown".to_string(),
    }
}

fn pptx_chart_axis_title_fields(
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

fn pptx_chart_value_axis_by_position<'a>(
    axes: &'a [PptxChartAxisInspection],
    position: &str,
) -> Option<&'a PptxChartAxisInspection> {
    axes.iter()
        .find(|axis| axis.axis_type == "value" && axis.position.as_deref() == Some(position))
}

fn pptx_chart_axis_number_format_name(
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

fn pptx_chart_axis_tick_mark_name(
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

fn canonical_pptx_chart_axis_options(
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

fn ensure_standard_pptx_chart_namespace(qualified: &[u8], local: &str) -> Result<()> {
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

fn standard_pptx_chart_type(qualified: &[u8]) -> Option<&'static str> {
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

fn pptx_chart_series_data_context(stack: &[String]) -> Option<&str> {
    for item in stack.iter().rev() {
        match item.as_str() {
            "tx" | "cat" | "val" | "xVal" | "yVal" | "bubbleSize" => return Some(item.as_str()),
            "ser" => break,
            _ => {}
        }
    }
    None
}

fn pptx_chart_is_chart_level_title(stack: &[String]) -> bool {
    stack.iter().any(|item| item == "title") && !stack.iter().any(|item| item == "plotArea")
}

fn pptx_chart_axis_title_context(stack: &[String]) -> Option<&'static str> {
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

fn pptx_chart_boolean_attribute(
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

fn set_unique_pptx_chart_formula(slot: &mut Option<String>, formula: &str) -> Result<()> {
    if slot.replace(formula.to_string()).is_some() {
        return Err(anyhow!(
            "PPTX chart series contains multiple formulas for one field"
        ));
    }
    Ok(())
}

fn add_pptx_chart_text_chars(current: usize, value: &str) -> Result<usize> {
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

fn pptx_chart_series_json(series: &PptxChartSeriesInspection) -> Value {
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
        "cached_bubble_size_points": series.bubble_sizes.len(),
        "categories_preview": series.categories.iter().take(MAX_PPTX_CHART_PREVIEW_POINTS).collect::<Vec<_>>(),
        "values_preview": series.values.iter().take(MAX_PPTX_CHART_PREVIEW_POINTS).collect::<Vec<_>>(),
        "bubble_sizes_preview": series.bubble_sizes.iter().take(MAX_PPTX_CHART_PREVIEW_POINTS).collect::<Vec<_>>(),
        "preview_truncated": categories_truncated || values_truncated || bubble_sizes_truncated,
    })
}

#[derive(Default)]
struct SlideRelationshipInspection {
    image_count: usize,
    notes_path: Option<String>,
}

fn inspect_slide_relationships(xml: &str, slide_path: &str) -> Result<SlideRelationshipInspection> {
    if xml.is_empty() {
        return Ok(SlideRelationshipInspection::default());
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut inspection = SlideRelationshipInspection::default();
    loop {
        match reader
            .read_event()
            .context("parse PPTX slide relationships")?
        {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"Relationship" =>
            {
                let relationship_type = required_xml_attribute(&reader, &event, "Type")?;
                if optional_xml_attribute(&reader, &event, "TargetMode")?
                    .is_some_and(|value| value.eq_ignore_ascii_case("external"))
                {
                    continue;
                }
                if relationship_type.ends_with("/image") {
                    inspection.image_count = inspection.image_count.saturating_add(1);
                } else if relationship_type.ends_with("/notesSlide") {
                    if inspection.notes_path.is_some() {
                        return Err(anyhow!("PPTX slide contains duplicate notes relationships"));
                    }
                    inspection.notes_path = Some(resolve_part_target(
                        slide_path,
                        required_xml_attribute(&reader, &event, "Target")?.as_str(),
                    )?);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(inspection)
}

fn resolve_part_target(source_part: &str, target: &str) -> Result<String> {
    if target.is_empty() || target.starts_with('/') || target.contains(['\\', '\0']) {
        return Err(anyhow!("PPTX relationship target is invalid"));
    }
    let mut parts = Path::new(source_part)
        .parent()
        .ok_or_else(|| anyhow!("PPTX relationship source has no parent"))?
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str().map(str::to_string),
            _ => None,
        })
        .collect::<Vec<_>>();
    for component in Path::new(target).components() {
        match component {
            Component::Normal(value) => parts.push(
                value
                    .to_str()
                    .ok_or_else(|| anyhow!("PPTX relationship target is not UTF-8"))?
                    .to_string(),
            ),
            Component::ParentDir => {
                if parts.pop().is_none() {
                    return Err(anyhow!("PPTX relationship target escapes the package"));
                }
            }
            Component::CurDir => {}
            _ => return Err(anyhow!("PPTX relationship target escapes the package")),
        }
    }
    if parts.is_empty() {
        return Err(anyhow!("PPTX relationship target is empty"));
    }
    Ok(parts.join("/"))
}

fn required_xml_attribute(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    name: &str,
) -> Result<String> {
    optional_xml_attribute(reader, event, name)?
        .ok_or_else(|| anyhow!("PPTX XML element is missing required {name} attribute"))
}

fn optional_xml_attribute(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    name: &str,
) -> Result<Option<String>> {
    for attribute in event.attributes().with_checks(false) {
        let attribute = attribute.context("parse PPTX XML attribute")?;
        let local = attribute.key.as_ref().rsplit(|byte| *byte == b':').next();
        if attribute.key.as_ref() == name.as_bytes() || local == Some(name.as_bytes()) {
            return Ok(Some(
                attribute
                    .decoded_and_normalized_value(XmlVersion::Explicit1_0, reader.decoder())?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn ensure_distinct_pptx_paths(source: &Path, target: &Path) -> Result<()> {
    let source_metadata = fs::symlink_metadata(source)
        .with_context(|| format!("inspect PPTX source {}", source.display()))?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_file() {
        return Err(anyhow!("PPTX source must be a regular non-symlink file"));
    }
    if source == target {
        return Err(anyhow!(
            "PPTX editing requires a distinct target_path; source files are never modified in place"
        ));
    }
    if target.exists() {
        let target_metadata = fs::symlink_metadata(target)
            .with_context(|| format!("inspect PPTX target {}", target.display()))?;
        if target_metadata.file_type().is_symlink() || !target_metadata.is_file() {
            return Err(anyhow!(
                "PPTX target exists and is not a regular non-symlink file"
            ));
        }
        if source.canonicalize()? == target.canonicalize()? {
            return Err(anyhow!(
                "PPTX editing requires a distinct target_path; source files are never modified in place"
            ));
        }
    }
    Ok(())
}

fn rewrite_pptx_package(
    source: &Path,
    target: &Path,
    replacements: &BTreeMap<String, Vec<u8>>,
    additions: Vec<(String, Vec<u8>)>,
    overwrite: bool,
) -> Result<u64> {
    let removals = HashSet::new();
    rewrite_pptx_package_with_removals(
        source,
        target,
        replacements,
        &removals,
        additions,
        overwrite,
    )
}

fn rewrite_pptx_package_with_removals(
    source: &Path,
    target: &Path,
    replacements: &BTreeMap<String, Vec<u8>>,
    removals: &HashSet<String>,
    additions: Vec<(String, Vec<u8>)>,
    overwrite: bool,
) -> Result<u64> {
    if target.exists() && !overwrite {
        return Err(anyhow!(
            "refusing to overwrite existing PPTX without overwrite=true"
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("PPTX output path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create PPTX output directory {}", parent.display()))?;
    let mut archive = ZipArchive::new(File::open(source)?)
        .with_context(|| format!("open PPTX {}", source.display()))?;
    if archive.is_empty() || archive.len() > MAX_PPTX_ZIP_ENTRIES {
        return Err(anyhow!("PPTX ZIP entry count is outside the safety limit"));
    }
    if archive.len().saturating_add(additions.len()) > MAX_PPTX_ZIP_ENTRIES {
        return Err(anyhow!(
            "edited PPTX would exceed the {MAX_PPTX_ZIP_ENTRIES} entry safety limit"
        ));
    }
    let temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary PPTX in {}", parent.display()))?;
    let mut writer = ZipWriter::new(temporary);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut names = HashSet::new();
    let mut replaced = HashSet::new();
    let mut removed = HashSet::new();
    let mut expanded = 0u64;
    let addition_names = additions
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<HashSet<_>>();
    if addition_names.len() != additions.len() {
        return Err(anyhow!("edited PPTX contains duplicate added ZIP entries"));
    }
    if removals
        .iter()
        .any(|name| replacements.contains_key(name) || addition_names.contains(name.as_str()))
    {
        return Err(anyhow!(
            "edited PPTX cannot replace or add an entry selected for removal"
        ));
    }
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if entry.enclosed_name().is_none() || entry.is_symlink() || !names.insert(name.clone()) {
            return Err(anyhow!("PPTX ZIP contains an unsafe or duplicate entry"));
        }
        if addition_names.contains(name.as_str()) {
            return Err(anyhow!(
                "edited PPTX would add a duplicate ZIP entry: {name}"
            ));
        }
        if removals.contains(name.as_str()) {
            removed.insert(name);
            continue;
        }
        expanded = expanded.saturating_add(
            replacements
                .get(name.as_str())
                .map_or(entry.size(), |content| content.len() as u64),
        );
        if expanded > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "edited PPTX exceeds the 100 MiB expanded safety limit"
            ));
        }
        if let Some(content) = replacements.get(name.as_str()) {
            writer.start_file(name.as_str(), options)?;
            writer.write_all(content.as_slice())?;
            replaced.insert(name);
        } else if entry.is_dir() {
            writer.add_directory(name.as_str(), entry.options())?;
        } else {
            writer.raw_copy_file(entry)?;
        }
    }
    for name in replacements.keys() {
        if !replaced.contains(name) {
            return Err(anyhow!(
                "PPTX ZIP is missing required replacement entry: {name}"
            ));
        }
    }
    if &removed != removals {
        return Err(anyhow!("PPTX ZIP is missing an entry selected for removal"));
    }
    for (name, content) in additions {
        if name.is_empty()
            || name.starts_with('/')
            || name
                .split('/')
                .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        {
            return Err(anyhow!("edited PPTX contains an unsafe added ZIP entry"));
        }
        expanded = expanded.saturating_add(content.len() as u64);
        if expanded > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "edited PPTX exceeds the 100 MiB expanded safety limit"
            ));
        }
        writer.start_file(name.as_str(), options)?;
        writer.write_all(content.as_slice())?;
    }
    let temporary = writer.finish().context("finalize edited PPTX")?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("sync temporary PPTX for {}", target.display()))?;
    let bytes = temporary.as_file().metadata()?.len();
    if bytes > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("edited PPTX exceeds the 100 MiB safety limit"));
    }
    if target.exists() {
        fs::remove_file(target)
            .with_context(|| format!("replace existing PPTX {}", target.display()))?;
    }
    temporary
        .persist(target)
        .map_err(|error| anyhow!("persist PPTX {}: {}", target.display(), error.error))?;
    Ok(bytes)
}

fn write_new_pptx(target: &Path, entries: Vec<(String, Vec<u8>)>, overwrite: bool) -> Result<u64> {
    if target.exists() {
        let metadata = fs::symlink_metadata(target)
            .with_context(|| format!("inspect PPTX target {}", target.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(anyhow!(
                "PPTX target exists and is not a regular non-symlink file"
            ));
        }
        if !overwrite {
            return Err(anyhow!(
                "refusing to overwrite existing PPTX without overwrite=true"
            ));
        }
    }
    if entries.is_empty() || entries.len() > MAX_PPTX_ZIP_ENTRIES {
        return Err(anyhow!(
            "generated PPTX ZIP entry count is outside the safety limit"
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("PPTX output path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create PPTX output directory {}", parent.display()))?;
    let temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary PPTX in {}", parent.display()))?;
    let mut writer = ZipWriter::new(temporary);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut names = HashSet::new();
    let mut expanded = 0u64;
    for (name, content) in entries {
        if name.is_empty() || !names.insert(name.clone()) {
            return Err(anyhow!(
                "generated PPTX contains an invalid or duplicate ZIP entry"
            ));
        }
        expanded = expanded.saturating_add(content.len() as u64);
        if expanded > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "generated PPTX exceeds the 100 MiB expanded safety limit"
            ));
        }
        writer.start_file(name.as_str(), options)?;
        writer.write_all(content.as_slice())?;
    }
    let temporary = writer.finish().context("finalize generated PPTX")?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("sync temporary PPTX for {}", target.display()))?;
    let bytes = temporary.as_file().metadata()?.len();
    if bytes > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("generated PPTX exceeds the 100 MiB safety limit"));
    }
    if target.exists() {
        fs::remove_file(target)
            .with_context(|| format!("replace existing PPTX {}", target.display()))?;
    }
    temporary
        .persist(target)
        .map_err(|error| anyhow!("persist PPTX {}: {}", target.display(), error.error))?;
    Ok(bytes)
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn unescape_xml(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;

    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;

    use super::*;
    use crate::WorkspaceState;

    fn presentation_test_context(root: &Path) -> (LocalState, RelayRequest) {
        let state = LocalState {
            workspaces: vec![WorkspaceState {
                id: "workspace-1".to_string(),
                absolute_root: root.to_path_buf(),
                alias: "test".to_string(),
                fingerprint: "fp".to_string(),
                project_config_trust: None,
            }],
            ..LocalState::default()
        };
        let request = RelayRequest {
            _message_type: "skill_execute_request".to_string(),
            request_id: "request-1".to_string(),
            owner_user_id: Some("owner-1".to_string()),
            device_id: Some("device-1".to_string()),
            workspace_id: "workspace-1".to_string(),
            method: Some("POST".to_string()),
            path: Some("/skills/execute".to_string()),
            headers: BTreeMap::new(),
            body: Value::Null,
        };
        (state, request)
    }

    fn render_validation_fixture_entries(slide_relationships: &str) -> Vec<(String, Vec<u8>)> {
        vec![
            (
                "[Content_Types].xml".to_string(),
                br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/></Types>"#.to_vec(),
            ),
            (
                "_rels/.rels".to_string(),
                br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#.to_vec(),
            ),
            (
                "ppt/presentation.xml".to_string(),
                br#"<?xml version="1.0"?><p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst><p:sldSz cx="12192000" cy="6858000"/></p:presentation>"#.to_vec(),
            ),
            (
                "ppt/_rels/presentation.xml.rels".to_string(),
                br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>"#.to_vec(),
            ),
            (
                "ppt/slides/slide1.xml".to_string(),
                br#"<?xml version="1.0"?><p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree/></p:cSld></p:sld>"#.to_vec(),
            ),
            (
                "ppt/slides/_rels/slide1.xml.rels".to_string(),
                slide_relationships.as_bytes().to_vec(),
            ),
            (
                "ppt/slideLayouts/slideLayout1.xml".to_string(),
                br#"<?xml version="1.0"?><p:sldLayout xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree/></p:cSld></p:sldLayout>"#.to_vec(),
            ),
        ]
    }

    #[test]
    fn render_validation_allows_internal_content_and_external_hyperlinks() {
        let temp = tempfile::tempdir().expect("temp");
        let path = temp.path().join("safe.pptx");
        let relationships = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com" TargetMode="External"/></Relationships>"#;
        write_new_pptx(
            path.as_path(),
            render_validation_fixture_entries(relationships),
            false,
        )
        .expect("write safe PPTX");
        validate_pptx_for_render(path.as_path()).expect("safe PPTX render validation");
    }

    #[test]
    fn render_validation_rejects_vba_and_embedded_parts() {
        let relationships = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/></Relationships>"#;
        for (name, entry) in [
            ("active.pptx", "ppt/vbaProject.bin"),
            ("embedded.pptx", "ppt/embeddings/object1.bin"),
        ] {
            let temp = tempfile::tempdir().expect("temp");
            let path = temp.path().join(name);
            let mut entries = render_validation_fixture_entries(relationships);
            entries.push((entry.to_string(), vec![0, 1, 2, 3]));
            write_new_pptx(path.as_path(), entries, false).expect("write unsafe PPTX");
            assert!(validate_pptx_for_render(path.as_path())
                .expect_err("active or embedded content must be rejected")
                .to_string()
                .contains("rejects active, embedded"));
        }
    }

    #[test]
    fn render_validation_rejects_external_non_hyperlink_relationships() {
        let temp = tempfile::tempdir().expect("temp");
        let path = temp.path().join("external-image.pptx");
        let relationships = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="https://example.com/image.png" TargetMode="External"/></Relationships>"#;
        write_new_pptx(
            path.as_path(),
            render_validation_fixture_entries(relationships),
            false,
        )
        .expect("write external PPTX");
        assert!(validate_pptx_for_render(path.as_path())
            .expect_err("external image must be rejected")
            .to_string()
            .contains("external non-hyperlink"));
    }

    #[test]
    fn image_fit_is_bounded_and_relationship_targets_stay_inside_package() {
        let image = PresentationImage {
            source_path: "image.png".to_string(),
            bytes: Vec::new(),
            format: PresentationImageFormat::Png,
            width: 1920,
            height: 1080,
            alt_text: "image".to_string(),
            fit: ImageFit::Contain,
        };
        let (x, y, cx, cy, crop) = fitted_image_box(&image, 0, 0, 1_000, 1_000);
        assert_eq!((x, y, cx), (0, 218, 1_000));
        assert_eq!(cy, 563);
        assert!(crop.is_empty());
        assert_eq!(
            resolve_part_target("ppt/slides/slide1.xml", "../notesSlides/notesSlide1.xml")
                .expect("notes target"),
            "ppt/notesSlides/notesSlide1.xml"
        );
        assert!(resolve_part_target("ppt/slides/slide1.xml", "../../../escape.xml").is_err());
    }

    #[test]
    fn exact_text_replacement_decodes_entities_and_never_crosses_runs() {
        let xml = r#"<p:sld xmlns:p="p" xmlns:a="a"><a:t>A &amp; B</a:t><a:t>C</a:t></p:sld>"#;
        let (updated, count, limited) =
            replace_drawing_text_runs(xml, "A & B", "D & E", 10).expect("replace entity text");
        assert_eq!(count, 1);
        assert!(!limited);
        assert!(updated.contains("<a:t>D &amp; E</a:t>"));
        let (_, count, _) =
            replace_drawing_text_runs(xml, "B C", "joined", 10).expect("cross-run scan");
        assert_eq!(count, 0);
    }

    #[test]
    fn cross_run_replacement_rewrites_one_unique_same_format_paragraph() {
        let xml = r#"<p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:pPr/><a:r><a:rPr lang="zh-CN" sz="2400"/><a:t xml:space="preserve">Prefix </a:t></a:r><a:r><a:rPr lang="zh-CN" sz="2400"/><a:t>Quarter</a:t></a:r><a:r><a:rPr lang="zh-CN" sz="2400"/><a:t>ly Rev</a:t></a:r><a:r><a:rPr lang="zh-CN" sz="2400"/><a:t xml:space="preserve">iew suffix</a:t></a:r><a:endParaRPr/></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#;
        let scan = scan_pptx_cross_run_text(xml, "Quarterly Review").expect("scan cross-run text");
        assert_eq!(scan.occurrences, 1);
        let matched = scan.matched.expect("eligible cross-run match");
        let formatting_before = xml.matches("<a:rPr lang=\"zh-CN\" sz=\"2400\"/>").count();
        let (updated, runs_touched, emptied_runs) =
            rewrite_pptx_cross_run_match(xml, &matched, "Annual Summary")
                .expect("rewrite cross-run match");
        assert_eq!(runs_touched, 3);
        assert_eq!(emptied_runs, 1);
        assert_eq!(
            pptx_visible_text(updated.as_str()).expect("updated visible text"),
            "Prefix Annual Summary suffix"
        );
        assert_eq!(
            updated
                .matches("<a:rPr lang=\"zh-CN\" sz=\"2400\"/>")
                .count(),
            formatting_before
        );
    }

    #[test]
    fn cross_run_replacement_rejects_ambiguous_and_different_format_matches() {
        let paragraph = r#"<a:p><a:r><a:rPr b="1"/><a:t>Quarter</a:t></a:r><a:r><a:rPr b="1"/><a:t>ly</a:t></a:r></a:p>"#;
        let ambiguous = format!("<p:sld>{paragraph}{paragraph}</p:sld>");
        let scan =
            scan_pptx_cross_run_text(ambiguous.as_str(), "Quarterly").expect("scan ambiguous text");
        assert_eq!(scan.occurrences, 2);
        assert!(scan.matched.is_none());

        let different = r#"<p:sld><a:p><a:r><a:rPr b="1"/><a:t>Quarter</a:t></a:r><a:r><a:rPr b="0"/><a:t>ly</a:t></a:r></a:p></p:sld>"#;
        let scan =
            scan_pptx_cross_run_text(different, "Quarterly").expect("scan different-format text");
        assert_eq!(scan.occurrences, 1);
        assert!(scan.matched.is_none());
        assert!(scan
            .unsupported_reason
            .is_some_and(|reason| reason.contains("different DrawingML run properties")));
    }

    #[test]
    fn cross_run_replacement_rejects_fields_breaks_and_hyperlinks() {
        for (label, paragraph) in [
            (
                "field",
                r#"<a:p><a:fld id="1" type="slidenum"><a:rPr/><a:t>Quarter</a:t></a:fld><a:r><a:rPr/><a:t>ly</a:t></a:r></a:p>"#,
            ),
            (
                "break",
                r#"<a:p><a:r><a:rPr/><a:t>Quarter</a:t></a:r><a:br/><a:r><a:rPr/><a:t>ly</a:t></a:r></a:p>"#,
            ),
            (
                "hyperlink",
                r#"<a:p><a:r><a:rPr><a:hlinkClick r:id="rId2"/></a:rPr><a:t>Quarter</a:t></a:r><a:r><a:rPr><a:hlinkClick r:id="rId2"/></a:rPr><a:t>ly</a:t></a:r></a:p>"#,
            ),
        ] {
            let xml = format!("<p:sld>{paragraph}</p:sld>");
            let scan = scan_pptx_cross_run_text(xml.as_str(), "Quarterly")
                .unwrap_or_else(|error| panic!("scan {label}: {error}"));
            assert_eq!(scan.occurrences, 1, "{label}");
            assert!(scan.matched.is_none(), "{label}");
            assert!(scan.unsupported_reason.is_some(), "{label}");
        }
    }

    #[test]
    #[ignore = "requires CHATOS_DOCUMENT_RUNTIME_DIR with a real packaged runtime"]
    fn packaged_runtime_smoke_renders_cross_run_replacement() {
        let workspace = tempfile::tempdir().expect("presentation smoke workspace");
        let (state, request) = presentation_test_context(workspace.path());
        create_pptx(
            &json!({
                "target_path":"base.pptx",
                "slides":[
                    {
                        "title":"Cross-run Replacement QA",
                        "body":"Quarterly Review for Visual QA",
                        "notes":"The body must read Annual Summary for Visual QA."
                    },
                    {
                        "title":"Safety Contract",
                        "body":"- Unique selection\n- Same run properties\n- Source remains unchanged"
                    }
                ]
            }),
            &state,
            &request,
        )
        .expect("create cross-run render smoke PPTX");
        let base = workspace.path().join("base.pptx");
        let source = workspace.path().join("source.pptx");
        let mut archive =
            ZipArchive::new(File::open(base.as_path()).expect("base PPTX")).expect("base PPTX ZIP");
        let slide_path = "ppt/slides/slide1.xml";
        let slide_xml = read_zip_text(&mut archive, slide_path).expect("base slide XML");
        drop(archive);
        let text_element = "<a:t xml:space=\"preserve\">Quarterly Review for Visual QA</a:t>";
        let text_start = slide_xml.find(text_element).expect("smoke body text");
        let run_start = slide_xml[..text_start]
            .rfind("<a:r>")
            .expect("smoke body run start");
        let run_close = slide_xml[text_start + text_element.len()..]
            .find("</a:r>")
            .map(|offset| text_start + text_element.len() + offset)
            .expect("smoke body run end");
        let run_end = run_close + "</a:r>".len();
        let properties = &slide_xml[run_start + "<a:r>".len()..text_start];
        let split_runs = ["Quarter", "ly Rev", "iew for Visual QA"]
            .into_iter()
            .map(|chunk| {
                format!("<a:r>{properties}<a:t xml:space=\"preserve\">{chunk}</a:t></a:r>")
            })
            .collect::<String>();
        let split_slide = format!(
            "{}{}{}",
            &slide_xml[..run_start],
            split_runs,
            &slide_xml[run_end..]
        );
        rewrite_pptx_package(
            base.as_path(),
            source.as_path(),
            &BTreeMap::from([(slide_path.to_string(), split_slide.into_bytes())]),
            Vec::new(),
            false,
        )
        .expect("write split-run PPTX fixture");
        let source_before = fs::read(source.as_path()).expect("split source bytes");
        let updated = replace_pptx_text_across_runs(
            &json!({
                "path":"source.pptx",
                "target_path":"replaced.pptx",
                "selection":"Quarterly Review",
                "replacement":"Annual Summary"
            }),
            &state,
            &request,
        )
        .expect("replace cross-run smoke text");
        assert_eq!(updated.get("runs_touched").and_then(Value::as_u64), Some(3));
        assert_eq!(
            fs::read(source.as_path()).expect("source after replacement"),
            source_before
        );
        let inspected = inspect_pptx(&json!({"path":"replaced.pptx"}), &state, &request)
            .expect("inspect cross-run smoke output");
        let preview = inspected
            .pointer("/slide_metadata/0/text_preview")
            .and_then(Value::as_str)
            .expect("smoke text preview");
        assert!(preview.contains("Annual Summary"));
        assert!(preview.contains("for Visual QA"));
        let replaced_path = workspace.path().join("replaced.pptx");
        let mut replaced_archive =
            ZipArchive::new(File::open(replaced_path.as_path()).expect("replaced PPTX"))
                .expect("replaced PPTX ZIP");
        let replaced_slide =
            read_zip_text(&mut replaced_archive, slide_path).expect("replaced slide XML");
        assert!(pptx_visible_text(replaced_slide.as_str())
            .expect("replaced visible text")
            .contains("Annual Summary for Visual QA"));
        drop(replaced_archive);
        let rendered = super::super::docx_render::render_presentation_pages(
            &json!({
                "path":"replaced.pptx",
                "first_slide":1,
                "last_slide":2,
                "dpi":120,
                "pdf_target_path":"replaced.pdf"
            }),
            &state,
            &request,
            Some(&AtomicBool::new(false)),
        )
        .expect("render cross-run smoke PPTX");
        assert_eq!(
            rendered
                .pointer("/_structured_result/pages_total")
                .and_then(Value::as_u64),
            Some(2)
        );
        if let Some(output) = std::env::var_os("CHATOS_PRESENTATION_CROSS_RUN_SMOKE_OUTPUT_DIR") {
            let output = PathBuf::from(output);
            fs::create_dir_all(output.as_path()).expect("create smoke output directory");
            fs::copy(
                workspace.path().join("replaced.pptx"),
                output.join("presentation-cross-run.pptx"),
            )
            .expect("write smoke PPTX");
            fs::copy(
                workspace.path().join("replaced.pdf"),
                output.join("presentation-cross-run.pdf"),
            )
            .expect("write smoke PDF");
            for (index, slide) in rendered
                .get("_model_input")
                .and_then(Value::as_array)
                .expect("smoke slide images")
                .iter()
                .enumerate()
            {
                let encoded = slide
                    .get("image_url")
                    .and_then(Value::as_str)
                    .and_then(|value| value.strip_prefix("data:image/png;base64,"))
                    .expect("smoke PNG data URL");
                fs::write(
                    output.join(format!("slide-{}.png", index + 1)),
                    STANDARD.decode(encoded).expect("decode smoke slide PNG"),
                )
                .expect("write smoke slide PNG");
            }
        }
    }

    #[test]
    fn slide_deletion_rejects_cross_slide_structures_and_removes_exact_entries() {
        let presentation = r#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst><p:custShowLst/></p:presentation>"#;
        assert!(reject_unsupported_slide_deletion_references(presentation)
            .expect_err("custom shows must block deletion")
            .to_string()
            .contains("custom shows"));

        let relationships = r#"<Relationships><Relationship Id="rId1" Type="slide" Target="slides/slide1.xml"/><Relationship Id="rId2" Type="theme" Target="theme/theme1.xml"/></Relationships>"#;
        let updated =
            remove_relationship_entries(relationships, &HashSet::from(["rId1".to_string()]))
                .expect("remove exact slide relationship");
        assert!(!updated.contains("rId1"));
        assert!(updated.contains("rId2"));

        let content_types = r#"<Types><Override PartName="/ppt/slides/slide1.xml" ContentType="slide"/><Override PartName="/ppt/theme/theme1.xml" ContentType="theme"/></Types>"#;
        let updated = remove_content_type_overrides(
            content_types,
            &HashSet::from(["/ppt/slides/slide1.xml".to_string()]),
        )
        .expect("remove exact content type");
        assert!(!updated.contains("slide1.xml"));
        assert!(updated.contains("theme1.xml"));
    }
}
