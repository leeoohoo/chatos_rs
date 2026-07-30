// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::image::parse_image;
use super::limits::{
    MAX_PPTX_CREATE_TABLE_CELLS, MAX_PPTX_CREATE_TABLE_COLUMNS, MAX_PPTX_CREATE_TABLE_ROWS,
    MAX_PPTX_SLIDES, MAX_PPTX_TABLE_CELL_TEXT_CHARS, MAX_PPTX_TABLE_TOTAL_TEXT_CHARS,
    MAX_PPTX_TEXT_CHARS, MAX_PPTX_TOTAL_IMAGE_BYTES, MAX_SLIDE_TEXT_CHARS,
};
use super::model::{PresentationTable, SlideDefinition, SlideLayout};
use super::parse_presentation_chart;
use super::text_validation::validate_slide_text;

pub(super) fn parse_slides(
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
                                .flatten()
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
