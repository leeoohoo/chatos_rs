// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};
use lopdf::{dictionary, text_string, Object};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::required_text;
use super::annotation_operation_common::{
    append_pdf_annotation, load_annotatable_pdf, save_annotation_output, AnnotatablePdf,
};
use super::{
    bounded_pdf_number, normalized_pdf_unicode_text, pdf_annotation_rect, pdf_page_bounds,
    pdf_page_rotation, MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS, MAX_PDF_ANNOTATION_CHARACTERS,
    MAX_PDF_PAGES,
};

pub(super) fn add_pdf_text_annotation(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let AnnotatablePdf {
        source,
        source_relative,
        mut document,
        page_map,
        ..
    } = load_annotatable_pdf(arguments, state, request)?;
    let page_number = arguments
        .get("page")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| (1..=MAX_PDF_PAGES as u32).contains(value))
        .ok_or_else(|| anyhow!("page must be an integer between 1 and {MAX_PDF_PAGES}"))?;
    let page_id = page_map
        .get(&page_number)
        .copied()
        .ok_or_else(|| anyhow!("page {page_number} does not exist"))?;
    let text = normalized_pdf_unicode_text(
        required_text(arguments, "text")?,
        "text",
        MAX_PDF_ANNOTATION_CHARACTERS,
        true,
    )?;
    let author = match arguments.get("author") {
        None => None,
        Some(Value::String(value)) => Some(normalized_pdf_unicode_text(
            value,
            "author",
            MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS,
            false,
        )?),
        Some(_) => return Err(anyhow!("author must be a string")),
    };
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .unwrap_or("top_right");
    if !matches!(
        position,
        "top_left" | "top_right" | "bottom_left" | "bottom_right"
    ) {
        return Err(anyhow!(
            "position is not a supported PDF text-annotation position"
        ));
    }
    let icon = arguments
        .get("icon")
        .and_then(Value::as_str)
        .unwrap_or("comment");
    let icon_name = match icon {
        "note" => "Note",
        "comment" => "Comment",
        "help" => "Help",
        "key" => "Key",
        "paragraph" => "Paragraph",
        "insert" => "Insert",
        "new_paragraph" => "NewParagraph",
        _ => return Err(anyhow!("icon is not a supported PDF Text annotation icon")),
    };
    let color = arguments
        .get("color")
        .and_then(Value::as_str)
        .unwrap_or("yellow");
    let color_components = match color {
        "yellow" => [1.0, 0.92, 0.2],
        "blue" => [0.3, 0.6, 1.0],
        "green" => [0.35, 0.8, 0.4],
        "red" => [1.0, 0.35, 0.35],
        _ => return Err(anyhow!("color is not a supported PDF annotation color")),
    };
    let size = bounded_pdf_number(arguments, "size_points", 24.0, 12.0, 72.0)?;
    let margin = bounded_pdf_number(arguments, "margin_points", 36.0, 12.0, 144.0)?;
    let open = match arguments.get("open") {
        None => false,
        Some(Value::Bool(value)) => *value,
        Some(_) => return Err(anyhow!("open must be a boolean")),
    };
    let rotation = pdf_page_rotation(&document, page_id)?;
    if rotation != 0 {
        return Err(anyhow!(
            "PDF Text annotation corner placement currently requires an unrotated page"
        ));
    }
    let rect = pdf_annotation_rect(pdf_page_bounds(&document, page_id)?, position, size, margin)?;
    let mut annotation = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Text",
        "Rect" => rect.iter().copied().map(Object::Real).collect::<Vec<_>>(),
        "Contents" => text_string(text.as_str()),
        "Name" => icon_name,
        "Open" => open,
        "F" => 4,
        "C" => color_components.iter().copied().map(Object::Real).collect::<Vec<_>>(),
        "P" => page_id,
    };
    if let Some(author) = author.as_deref() {
        annotation.set("T", text_string(author));
    }
    append_pdf_annotation(&mut document, page_number, page_id, annotation)?;
    let (target_relative, bytes) =
        save_annotation_output(arguments, state, request, source.as_path(), &mut document)?;
    let contents_preview = text.chars().take(200).collect::<String>();
    Ok(json!({
        "created": true,
        "operation": "add_text_annotation",
        "source_path": source_relative,
        "path": target_relative,
        "page": page_number,
        "characters": text.chars().count(),
        "contents_preview": contents_preview,
        "contents_sha256": hex::encode(Sha256::digest(text.as_bytes())),
        "author": author,
        "position": position,
        "icon": icon,
        "color": color,
        "size_points": size,
        "margin_points": margin,
        "open": open,
        "rect": rect,
        "bytes": bytes,
    }))
}
