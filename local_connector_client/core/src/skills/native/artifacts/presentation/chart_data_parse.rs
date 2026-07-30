// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use anyhow::{anyhow, Result};
use serde_json::{Map, Value};

use super::chart_model::{
    PresentationChartSeries, PresentationChartType, PresentationChartValueAxis,
    PresentationChartValueAxisOptions,
};
use super::chart_parse::{
    parse_presentation_chart_series_color, parse_presentation_chart_series_marker,
    parse_presentation_chart_series_smooth, validate_presentation_chart_numeric_axis_bounds,
};
use super::limits::{
    MAX_PPTX_CREATE_CHART_CATEGORIES, MAX_PPTX_CREATE_CHART_SERIES, MAX_PPTX_CREATE_CHART_VALUE_ABS,
};
use super::text_validation::validate_slide_text;

pub(super) struct ParsedPresentationChartData {
    pub(super) categories: Option<Vec<String>>,
    pub(super) x_values: Option<Vec<f64>>,
    pub(super) series: Vec<PresentationChartSeries>,
}

pub(super) fn parse_presentation_chart_data(
    object: &Map<String, Value>,
    chart_type: PresentationChartType,
    x_axis_options: PresentationChartValueAxisOptions,
    slide_number: usize,
) -> Result<ParsedPresentationChartData> {
    let (categories, x_values, point_count) = if chart_type.uses_numeric_x_axis() {
        if object
            .get("categories")
            .is_some_and(|value| !value.is_null())
        {
            return Err(anyhow!(
                "slide {slide_number} {} chart categories must be null or omitted",
                chart_type.as_str()
            ));
        }
        let x_values = object
            .get("x_values")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                anyhow!(
                    "slide {slide_number} {} chart x_values must be an array",
                    chart_type.as_str()
                )
            })?;
        if x_values.is_empty() || x_values.len() > MAX_PPTX_CREATE_CHART_CATEGORIES {
            return Err(anyhow!(
                "slide {slide_number} {} chart must contain between 1 and {MAX_PPTX_CREATE_CHART_CATEGORIES} x_values",
                chart_type.as_str()
            ));
        }
        let x_values = x_values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let value = value.as_f64().ok_or_else(|| {
                    anyhow!(
                        "slide {slide_number} {} chart x_value {} must be a finite number",
                        chart_type.as_str(),
                        index + 1
                    )
                })?;
                if !value.is_finite() || value.abs() > MAX_PPTX_CREATE_CHART_VALUE_ABS {
                    return Err(anyhow!(
                        "slide {slide_number} {} chart x_value {} exceeds the numeric safety limit",
                        chart_type.as_str(),
                        index + 1
                    ));
                }
                Ok(value)
            })
            .collect::<Result<Vec<_>>>()?;
        validate_presentation_chart_numeric_axis_bounds(
            x_values.as_slice(),
            x_axis_options.minimum,
            x_axis_options.maximum,
            x_axis_options.log_base,
            x_axis_options.major_unit,
            x_axis_options.minor_unit,
            slide_number,
            format!("{} X-axis", chart_type.as_str()).as_str(),
            format!("{} logarithmic X-axis", chart_type.as_str()).as_str(),
            "X value",
            "X values",
        )?;
        let point_count = x_values.len();
        (None, Some(x_values), point_count)
    } else {
        if object.get("x_values").is_some_and(|value| !value.is_null()) {
            return Err(anyhow!(
                "slide {slide_number} chart without a numeric X axis x_values must be null or omitted"
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
        let point_count = categories.len();
        (Some(categories), None, point_count)
    };
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
                    | "bubble_sizes"
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
        if values.len() != point_count {
            return Err(anyhow!(
                "slide {slide_number} chart series {} must contain exactly one value per {}",
                series_index + 1,
                if chart_type.uses_numeric_x_axis() {
                    "x_value"
                } else {
                    "category"
                }
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
        let bubble_sizes = if chart_type == PresentationChartType::Bubble {
            let bubble_sizes = series
                .get("bubble_sizes")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    anyhow!(
                        "slide {slide_number} bubble chart series {} bubble_sizes must be an array",
                        series_index + 1
                    )
                })?;
            if bubble_sizes.len() != point_count {
                return Err(anyhow!(
                    "slide {slide_number} bubble chart series {} must contain exactly one bubble_size per x_value",
                    series_index + 1
                ));
            }
            Some(
                bubble_sizes
                    .iter()
                    .enumerate()
                    .map(|(value_index, value)| {
                        let value = value.as_f64().ok_or_else(|| {
                            anyhow!(
                                "slide {slide_number} bubble chart series {} bubble_size {} must be a finite positive number",
                                series_index + 1,
                                value_index + 1
                            )
                        })?;
                        if !value.is_finite()
                            || value <= 0.0
                            || value > MAX_PPTX_CREATE_CHART_VALUE_ABS
                        {
                            return Err(anyhow!(
                                "slide {slide_number} bubble chart series {} bubble_size {} must be positive and within the numeric safety limit",
                                series_index + 1,
                                value_index + 1
                            ));
                        }
                        Ok(value)
                    })
                    .collect::<Result<Vec<_>>>()?,
            )
        } else {
            if series
                .get("bubble_sizes")
                .is_some_and(|value| !value.is_null())
            {
                return Err(anyhow!(
                    "slide {slide_number} non-bubble chart series {} bubble_sizes must be null or omitted",
                    series_index + 1
                ));
            }
            None
        };
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
            bubble_sizes,
            value_axis,
            color,
            marker_style,
            marker_size,
            smooth,
        });
    }
    Ok(ParsedPresentationChartData {
        categories,
        x_values,
        series: parsed_series,
    })
}
