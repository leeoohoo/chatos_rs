// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use anyhow::{anyhow, Result};
use lopdf::Document;
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{input_file, optional_bool, required_text};
use super::generation_common::bounded_pdf_number;
use super::package_write::{load_editable_pdf, pdf_output_path, save_pdf_document};
use super::page_selection::optional_page_numbers;
use super::stamp_text_common::{
    apply_pdf_text_stamps, normalized_pdf_stamp_text, PdfTextStampStyle,
};
use super::MAX_PDF_PAGES;

pub(super) fn stamp_pdf_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let mut document = load_editable_pdf(source.as_path())?;
    let page_map = document.get_pages();
    let page_count = page_map.len();
    if page_count > MAX_PDF_PAGES {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_PAGES} page stamping safety limit"
        ));
    }
    let pages = optional_page_numbers(arguments, "pages", page_count)?
        .unwrap_or_else(|| (1..=page_count as u32).collect());
    let text = normalized_pdf_stamp_text(required_text(arguments, "text")?)?;
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .unwrap_or("center");
    if !matches!(
        position,
        "top_left"
            | "top_center"
            | "top_right"
            | "center"
            | "bottom_left"
            | "bottom_center"
            | "bottom_right"
    ) {
        return Err(anyhow!("position is not a supported PDF stamp position"));
    }
    let font_size = bounded_pdf_number(arguments, "font_size", 24.0, 8.0, 72.0)?;
    let margin = bounded_pdf_number(arguments, "margin_points", 36.0, 12.0, 144.0)?;
    let opacity = bounded_pdf_number(arguments, "opacity", 0.25, 0.05, 1.0)?;
    let grayscale = bounded_pdf_number(arguments, "grayscale", 0.5, 0.0, 1.0)?;
    let rotation = arguments
        .get("rotation")
        .map(|value| {
            value
                .as_i64()
                .filter(|value| matches!(value, -45 | 0 | 45))
                .ok_or_else(|| anyhow!("rotation must be -45, 0, or 45"))
        })
        .transpose()?
        .unwrap_or(0);
    let stamps = pages
        .iter()
        .map(|page| (*page, text.clone()))
        .collect::<Vec<_>>();
    apply_pdf_text_stamps(
        &mut document,
        &page_map,
        stamps.as_slice(),
        PdfTextStampStyle {
            position,
            font_size,
            margin,
            opacity,
            grayscale,
            rotation,
        },
    )?;

    let (target_relative, bytes) =
        save_stamp_output(arguments, state, request, source.as_path(), &mut document)?;
    Ok(json!({
        "created": true,
        "operation": "stamp_text",
        "source_path": source_relative,
        "path": target_relative,
        "text": text,
        "pages": pages,
        "position": position,
        "font": "Helvetica",
        "font_size": font_size,
        "rotation": rotation,
        "opacity": opacity,
        "grayscale": grayscale,
        "bytes": bytes,
    }))
}

pub(super) fn stamp_pdf_page_numbers(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let mut document = load_editable_pdf(source.as_path())?;
    let page_map = document.get_pages();
    let page_count = page_map.len();
    if page_count > MAX_PDF_PAGES {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_PAGES} page numbering safety limit"
        ));
    }
    let pages = optional_page_numbers(arguments, "pages", page_count)?
        .unwrap_or_else(|| (1..=page_count as u32).collect());
    let page_number_format = arguments
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("page_number_of_total");
    if !matches!(
        page_number_format,
        "number" | "page_number" | "page_number_of_total"
    ) {
        return Err(anyhow!("format is not a supported PDF page-number format"));
    }
    let start_number = match arguments.get("start_number") {
        None => 1u32,
        Some(Value::Number(value)) => value
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| (1..=1_000_000).contains(value))
            .ok_or_else(|| anyhow!("start_number must be an integer between 1 and 1000000"))?,
        Some(_) => {
            return Err(anyhow!(
                "start_number must be an integer between 1 and 1000000"
            ));
        }
    };
    let end_number = start_number
        .checked_add(u32::try_from(page_count.saturating_sub(1))?)
        .filter(|value| *value <= 1_000_000)
        .ok_or_else(|| anyhow!("PDF page numbering would exceed 1000000"))?;
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .unwrap_or("bottom_center");
    if !matches!(
        position,
        "top_left" | "top_center" | "top_right" | "bottom_left" | "bottom_center" | "bottom_right"
    ) {
        return Err(anyhow!(
            "position is not a supported PDF page-number position"
        ));
    }
    let font_size = bounded_pdf_number(arguments, "font_size", 10.0, 8.0, 24.0)?;
    let margin = bounded_pdf_number(arguments, "margin_points", 36.0, 12.0, 144.0)?;
    let opacity = bounded_pdf_number(arguments, "opacity", 1.0, 0.05, 1.0)?;
    let grayscale = bounded_pdf_number(arguments, "grayscale", 0.0, 0.0, 1.0)?;
    let stamps = pages
        .iter()
        .map(|page| {
            let displayed = start_number + page - 1;
            pdf_page_number_label(page_number_format, displayed, end_number)
                .map(|label| (*page, label))
        })
        .collect::<Result<Vec<_>>>()?;
    apply_pdf_text_stamps(
        &mut document,
        &page_map,
        stamps.as_slice(),
        PdfTextStampStyle {
            position,
            font_size,
            margin,
            opacity,
            grayscale,
            rotation: 0,
        },
    )?;

    let (target_relative, bytes) =
        save_stamp_output(arguments, state, request, source.as_path(), &mut document)?;
    Ok(json!({
        "created": true,
        "operation": "stamp_page_numbers",
        "source_path": source_relative,
        "path": target_relative,
        "pages": pages,
        "format": page_number_format,
        "start_number": start_number,
        "end_number": end_number,
        "first_label": stamps.first().map(|(_, label)| label.as_str()),
        "last_label": stamps.last().map(|(_, label)| label.as_str()),
        "position": position,
        "font": "Helvetica",
        "font_size": font_size,
        "opacity": opacity,
        "grayscale": grayscale,
        "bytes": bytes,
    }))
}

fn pdf_page_number_label(
    page_number_format: &str,
    displayed: u32,
    end_number: u32,
) -> Result<String> {
    match page_number_format {
        "number" => Ok(displayed.to_string()),
        "page_number" => Ok(format!("Page {displayed}")),
        "page_number_of_total" => Ok(format!("Page {displayed} of {end_number}")),
        _ => Err(anyhow!("format is not a supported PDF page-number format")),
    }
}

fn save_stamp_output(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    source: &Path,
    document: &mut Document,
) -> Result<(String, u64)> {
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = pdf_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source.to_path_buf()),
    )?;
    let bytes = save_pdf_document(
        document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok((target_relative, bytes))
}
