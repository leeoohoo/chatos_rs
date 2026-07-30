// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PresentationChartType {
    Column,
    Bar,
    Line,
    Pie,
    Area,
    Doughnut,
    Radar,
    Scatter,
    Bubble,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PresentationChartDataLabels {
    None,
    Value,
    Percentage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PresentationChartValueAxis {
    Primary,
    Secondary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PresentationChartValueAxisNumberFormat {
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
pub(super) enum PresentationChartAxisTickMark {
    None,
    Inside,
    Outside,
    Cross,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PresentationChartMarkerStyle {
    None,
    Circle,
    Square,
    Diamond,
    Triangle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PresentationChartValueAxisOptions {
    pub(super) minimum: Option<f64>,
    pub(super) maximum: Option<f64>,
    pub(super) log_base: Option<f64>,
    pub(super) major_tick_mark: PresentationChartAxisTickMark,
    pub(super) minor_tick_mark: PresentationChartAxisTickMark,
    pub(super) major_unit: Option<f64>,
    pub(super) minor_unit: Option<f64>,
    pub(super) number_format: PresentationChartValueAxisNumberFormat,
}

impl PresentationChartValueAxis {
    pub(super) fn parse(value: &str) -> Result<Self> {
        match value {
            "primary" => Ok(Self::Primary),
            "secondary" => Ok(Self::Secondary),
            value => Err(anyhow!("unsupported PPTX chart value_axis: {value}")),
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
        }
    }
}

impl PresentationChartValueAxisNumberFormat {
    pub(super) fn parse(value: &str) -> Result<Self> {
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

    pub(super) fn from_ooxml(format_code: &str, source_linked: bool) -> Result<Self> {
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

    pub(super) fn as_str(self) -> &'static str {
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

    pub(super) fn format_code(self) -> &'static str {
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

    pub(super) fn source_linked(self) -> bool {
        self == Self::General
    }
}

impl PresentationChartAxisTickMark {
    pub(super) fn parse(value: &str) -> Result<Self> {
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

    pub(super) fn from_ooxml(value: &str) -> Result<Self> {
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

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Inside => "inside",
            Self::Outside => "outside",
            Self::Cross => "cross",
        }
    }

    pub(super) fn as_ooxml(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Inside => "in",
            Self::Outside => "out",
            Self::Cross => "cross",
        }
    }
}

impl PresentationChartMarkerStyle {
    pub(super) fn parse(value: &str) -> Result<Self> {
        match value {
            "none" => Ok(Self::None),
            "circle" => Ok(Self::Circle),
            "square" => Ok(Self::Square),
            "diamond" => Ok(Self::Diamond),
            "triangle" => Ok(Self::Triangle),
            value => Err(anyhow!(
                "unsupported PPTX line/scatter-chart series marker style: {value}"
            )),
        }
    }

    pub(super) fn from_ooxml(value: &str) -> Result<Self> {
        Self::parse(value).with_context(|| {
            format!(
                "chart line/scatter-series marker style is outside the canonical bounded contract: {value}"
            )
        })
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Circle => "circle",
            Self::Square => "square",
            Self::Diamond => "diamond",
            Self::Triangle => "triangle",
        }
    }

    pub(super) fn as_ooxml(self) -> &'static str {
        self.as_str()
    }
}

impl PresentationChartDataLabels {
    pub(super) fn parse(value: &str) -> Result<Self> {
        match value {
            "none" => Ok(Self::None),
            "value" => Ok(Self::Value),
            "percentage" => Ok(Self::Percentage),
            value => Err(anyhow!("unsupported PPTX chart data_labels mode: {value}")),
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Value => "value",
            Self::Percentage => "percentage",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PresentationChartLegendPosition {
    Right,
    Left,
    Top,
    Bottom,
}

impl PresentationChartLegendPosition {
    pub(super) fn parse(value: &str) -> Result<Self> {
        match value {
            "right" => Ok(Self::Right),
            "left" => Ok(Self::Left),
            "top" => Ok(Self::Top),
            "bottom" => Ok(Self::Bottom),
            value => Err(anyhow!("unsupported PPTX chart legend_position: {value}")),
        }
    }

    pub(super) fn from_ooxml(value: &str) -> Result<Self> {
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

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Left => "left",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }

    pub(super) fn as_ooxml(self) -> &'static str {
        match self {
            Self::Right => "r",
            Self::Left => "l",
            Self::Top => "t",
            Self::Bottom => "b",
        }
    }
}

impl PresentationChartType {
    pub(super) fn parse(value: &str) -> Result<Self> {
        match value {
            "column" => Ok(Self::Column),
            "bar" => Ok(Self::Bar),
            "line" => Ok(Self::Line),
            "pie" => Ok(Self::Pie),
            "area" => Ok(Self::Area),
            "doughnut" => Ok(Self::Doughnut),
            "radar" => Ok(Self::Radar),
            "scatter" => Ok(Self::Scatter),
            "bubble" => Ok(Self::Bubble),
            value => Err(anyhow!("unsupported PPTX chart type: {value}")),
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Column => "column",
            Self::Bar => "bar",
            Self::Line => "line",
            Self::Pie => "pie",
            Self::Area => "area",
            Self::Doughnut => "doughnut",
            Self::Radar => "radar",
            Self::Scatter => "scatter",
            Self::Bubble => "bubble",
        }
    }

    pub(super) fn is_part_to_whole(self) -> bool {
        matches!(self, Self::Pie | Self::Doughnut)
    }

    pub(super) fn uses_line_color(self) -> bool {
        matches!(self, Self::Line | Self::Radar | Self::Scatter)
    }

    pub(super) fn uses_numeric_x_axis(self) -> bool {
        matches!(self, Self::Scatter | Self::Bubble)
    }

    pub(super) fn primary_category_axis_position(self) -> &'static str {
        if self == Self::Bar {
            "l"
        } else {
            "b"
        }
    }

    pub(super) fn secondary_category_axis_position(self) -> &'static str {
        if self == Self::Bar {
            "r"
        } else {
            "t"
        }
    }

    pub(super) fn primary_value_axis_position(self) -> &'static str {
        if self == Self::Bar {
            "b"
        } else {
            "l"
        }
    }

    pub(super) fn secondary_value_axis_position(self) -> &'static str {
        if self == Self::Bar {
            "t"
        } else {
            "r"
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct PresentationChartSeries {
    pub(super) name: String,
    pub(super) values: Vec<f64>,
    pub(super) bubble_sizes: Option<Vec<f64>>,
    pub(super) value_axis: PresentationChartValueAxis,
    pub(super) color: Option<String>,
    pub(super) marker_style: Option<PresentationChartMarkerStyle>,
    pub(super) marker_size: Option<u8>,
    pub(super) smooth: Option<bool>,
}

#[derive(Clone, Debug)]
pub(super) struct PresentationChart {
    pub(super) chart_type: PresentationChartType,
    pub(super) title: String,
    pub(super) categories: Option<Vec<String>>,
    pub(super) x_values: Option<Vec<f64>>,
    pub(super) x_axis_minimum: Option<f64>,
    pub(super) x_axis_maximum: Option<f64>,
    pub(super) x_axis_log_base: Option<f64>,
    pub(super) x_axis_major_tick_mark: PresentationChartAxisTickMark,
    pub(super) x_axis_minor_tick_mark: PresentationChartAxisTickMark,
    pub(super) x_axis_major_unit: Option<f64>,
    pub(super) x_axis_minor_unit: Option<f64>,
    pub(super) x_axis_number_format: PresentationChartValueAxisNumberFormat,
    pub(super) series: Vec<PresentationChartSeries>,
    pub(super) show_legend: bool,
    pub(super) legend_position: PresentationChartLegendPosition,
    pub(super) data_labels: PresentationChartDataLabels,
    pub(super) category_axis_title: String,
    pub(super) value_axis_title: String,
    pub(super) secondary_value_axis_title: String,
    pub(super) value_axis_minimum: Option<f64>,
    pub(super) value_axis_maximum: Option<f64>,
    pub(super) value_axis_log_base: Option<f64>,
    pub(super) value_axis_major_tick_mark: PresentationChartAxisTickMark,
    pub(super) value_axis_minor_tick_mark: PresentationChartAxisTickMark,
    pub(super) value_axis_major_unit: Option<f64>,
    pub(super) value_axis_minor_unit: Option<f64>,
    pub(super) value_axis_number_format: PresentationChartValueAxisNumberFormat,
    pub(super) secondary_value_axis_minimum: Option<f64>,
    pub(super) secondary_value_axis_maximum: Option<f64>,
    pub(super) secondary_value_axis_log_base: Option<f64>,
    pub(super) secondary_value_axis_major_tick_mark: PresentationChartAxisTickMark,
    pub(super) secondary_value_axis_minor_tick_mark: PresentationChartAxisTickMark,
    pub(super) secondary_value_axis_major_unit: Option<f64>,
    pub(super) secondary_value_axis_minor_unit: Option<f64>,
    pub(super) secondary_value_axis_number_format: PresentationChartValueAxisNumberFormat,
}
