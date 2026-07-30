// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::chart_model::{
    PresentationChartAxisTickMark, PresentationChartType, PresentationChartValueAxisNumberFormat,
    PresentationChartValueAxisOptions,
};
use super::chart_xml_common::{presentation_chart_number_text, presentation_chart_title_xml};

pub(super) fn presentation_chart_axes_xml(
    chart_type: PresentationChartType,
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
    let category_axis_position = chart_type.primary_category_axis_position();
    let value_axis_position = chart_type.primary_value_axis_position();
    format!(
        r#"<c:catAx><c:axId val="{category_axis_id}"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="{category_axis_position}"/>{category_axis_title}<c:tickLblPos val="nextTo"/><c:crossAx val="{value_axis_id}"/><c:crosses val="autoZero"/><c:auto val="1"/><c:lblAlgn val="ctr"/><c:lblOffset val="100"/></c:catAx><c:valAx><c:axId val="{value_axis_id}"/>{value_axis_scaling}<c:delete val="0"/><c:axPos val="{value_axis_position}"/><c:majorGridlines/>{value_axis_title}{value_axis_number_format}{value_axis_tick_marks}<c:tickLblPos val="nextTo"/><c:crossAx val="{category_axis_id}"/><c:crosses val="autoZero"/><c:crossBetween val="{cross_between}"/>{value_axis_units}</c:valAx>"#
    )
}

pub(super) fn presentation_chart_secondary_axes_xml(
    chart_type: PresentationChartType,
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
    let category_axis_position = chart_type.secondary_category_axis_position();
    let value_axis_position = chart_type.secondary_value_axis_position();
    format!(
        r#"<c:catAx><c:axId val="{category_axis_id}"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="1"/><c:axPos val="{category_axis_position}"/><c:tickLblPos val="none"/><c:crossAx val="{value_axis_id}"/><c:crosses val="max"/><c:auto val="1"/><c:lblAlgn val="ctr"/><c:lblOffset val="100"/></c:catAx><c:valAx><c:axId val="{value_axis_id}"/>{value_axis_scaling}<c:delete val="0"/><c:axPos val="{value_axis_position}"/>{value_axis_title}{value_axis_number_format}{value_axis_tick_marks}<c:tickLblPos val="nextTo"/><c:crossAx val="{category_axis_id}"/><c:crosses val="max"/><c:crossBetween val="{cross_between}"/>{value_axis_units}</c:valAx>"#
    )
}

pub(super) fn presentation_scatter_chart_axes_xml(
    x_axis_id: u32,
    y_axis_id: u32,
    x_axis_title: &str,
    y_axis_title: &str,
    x_axis_options: PresentationChartValueAxisOptions,
    y_axis_options: PresentationChartValueAxisOptions,
) -> String {
    let PresentationChartValueAxisOptions {
        minimum: x_minimum,
        maximum: x_maximum,
        log_base: x_log_base,
        major_tick_mark: x_major_tick_mark,
        minor_tick_mark: x_minor_tick_mark,
        major_unit: x_major_unit,
        minor_unit: x_minor_unit,
        number_format: x_number_format,
    } = x_axis_options;
    let PresentationChartValueAxisOptions {
        minimum,
        maximum,
        log_base,
        major_tick_mark,
        minor_tick_mark,
        major_unit,
        minor_unit,
        number_format,
    } = y_axis_options;
    let x_axis_title = if x_axis_title.is_empty() {
        String::new()
    } else {
        presentation_chart_title_xml(x_axis_title)
    };
    let y_axis_title = if y_axis_title.is_empty() {
        String::new()
    } else {
        presentation_chart_title_xml(y_axis_title)
    };
    let x_axis_scaling = presentation_chart_axis_scaling_xml(x_minimum, x_maximum, x_log_base);
    let x_axis_number_format = presentation_chart_axis_number_format_xml(x_number_format);
    let x_axis_tick_marks =
        presentation_chart_axis_tick_marks_xml(x_major_tick_mark, x_minor_tick_mark);
    let x_axis_units = presentation_chart_axis_units_xml(x_major_unit, x_minor_unit);
    let y_axis_scaling = presentation_chart_axis_scaling_xml(minimum, maximum, log_base);
    let y_axis_number_format = presentation_chart_axis_number_format_xml(number_format);
    let y_axis_tick_marks =
        presentation_chart_axis_tick_marks_xml(major_tick_mark, minor_tick_mark);
    let y_axis_units = presentation_chart_axis_units_xml(major_unit, minor_unit);
    format!(
        r#"<c:valAx><c:axId val="{x_axis_id}"/>{x_axis_scaling}<c:delete val="0"/><c:axPos val="b"/>{x_axis_title}{x_axis_number_format}{x_axis_tick_marks}<c:tickLblPos val="nextTo"/><c:crossAx val="{y_axis_id}"/><c:crosses val="autoZero"/><c:crossBetween val="midCat"/>{x_axis_units}</c:valAx><c:valAx><c:axId val="{y_axis_id}"/>{y_axis_scaling}<c:delete val="0"/><c:axPos val="l"/><c:majorGridlines/>{y_axis_title}{y_axis_number_format}{y_axis_tick_marks}<c:tickLblPos val="nextTo"/><c:crossAx val="{x_axis_id}"/><c:crosses val="autoZero"/><c:crossBetween val="midCat"/>{y_axis_units}</c:valAx>"#
    )
}

pub(super) fn presentation_scatter_chart_secondary_axes_xml(
    x_axis_id: u32,
    y_axis_id: u32,
    y_axis_title: &str,
    x_axis_options: PresentationChartValueAxisOptions,
    y_axis_options: PresentationChartValueAxisOptions,
) -> String {
    let PresentationChartValueAxisOptions {
        minimum: x_minimum,
        maximum: x_maximum,
        log_base: x_log_base,
        major_tick_mark: x_major_tick_mark,
        minor_tick_mark: x_minor_tick_mark,
        major_unit: x_major_unit,
        minor_unit: x_minor_unit,
        number_format: x_number_format,
    } = x_axis_options;
    let PresentationChartValueAxisOptions {
        minimum,
        maximum,
        log_base,
        major_tick_mark,
        minor_tick_mark,
        major_unit,
        minor_unit,
        number_format,
    } = y_axis_options;
    let y_axis_title = if y_axis_title.is_empty() {
        String::new()
    } else {
        presentation_chart_title_xml(y_axis_title)
    };
    let x_axis_scaling = presentation_chart_axis_scaling_xml(x_minimum, x_maximum, x_log_base);
    let x_axis_number_format = presentation_chart_axis_number_format_xml(x_number_format);
    let x_axis_tick_marks =
        presentation_chart_axis_tick_marks_xml(x_major_tick_mark, x_minor_tick_mark);
    let x_axis_units = presentation_chart_axis_units_xml(x_major_unit, x_minor_unit);
    let y_axis_scaling = presentation_chart_axis_scaling_xml(minimum, maximum, log_base);
    let y_axis_number_format = presentation_chart_axis_number_format_xml(number_format);
    let y_axis_tick_marks =
        presentation_chart_axis_tick_marks_xml(major_tick_mark, minor_tick_mark);
    let y_axis_units = presentation_chart_axis_units_xml(major_unit, minor_unit);
    format!(
        r#"<c:valAx><c:axId val="{x_axis_id}"/>{x_axis_scaling}<c:delete val="1"/><c:axPos val="t"/>{x_axis_number_format}{x_axis_tick_marks}<c:tickLblPos val="none"/><c:crossAx val="{y_axis_id}"/><c:crosses val="max"/><c:crossBetween val="midCat"/>{x_axis_units}</c:valAx><c:valAx><c:axId val="{y_axis_id}"/>{y_axis_scaling}<c:delete val="0"/><c:axPos val="r"/>{y_axis_title}{y_axis_number_format}{y_axis_tick_marks}<c:tickLblPos val="nextTo"/><c:crossAx val="{x_axis_id}"/><c:crosses val="max"/><c:crossBetween val="midCat"/>{y_axis_units}</c:valAx>"#
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
