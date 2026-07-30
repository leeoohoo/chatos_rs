// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};
use lopdf::{dictionary, text_string, Object};
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::required_text;
use super::annotation_link::validated_pdf_https_link_uri;
use super::annotation_operation_common::{
    append_pdf_annotation, ensure_annotation_capacity, load_guarded_annotation_pdf,
    save_guarded_annotation_output, GuardedAnnotationPdf,
};
use super::generation_common::required_bounded_pdf_number;
use super::{
    normalized_pdf_unicode_text, pdf_page_bounds, pdf_page_rotation,
    MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS, MAX_PDF_ANNOTATION_CHARACTERS, MAX_PDF_PAGES,
};

pub(super) fn add_pdf_link_annotation(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let GuardedAnnotationPdf {
        source,
        source_relative,
        expected_source_sha256,
        mut document,
        page_map,
        inspection: annotation_inspection,
    } = load_guarded_annotation_pdf(arguments, state, request)?;
    ensure_annotation_capacity(&annotation_inspection)?;
    if annotation_inspection
        .get("unsafe_link_count")
        .and_then(Value::as_u64)
        .is_some_and(|count| count > 0)
    {
        return Err(anyhow!(
            "PDF contains unsafe or unsupported existing Link actions; no Link annotation was added"
        ));
    }

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
    if pdf_page_rotation(&document, page_id)? != 0 {
        return Err(anyhow!(
            "PDF Link annotation geometry currently requires an unrotated page"
        ));
    }
    let x = required_bounded_pdf_number(arguments, "x", 0.0, 20_000.0)?;
    let y = required_bounded_pdf_number(arguments, "y", 0.0, 20_000.0)?;
    let width = required_bounded_pdf_number(arguments, "width", 0.1, 20_000.0)?;
    let height = required_bounded_pdf_number(arguments, "height", 0.1, 20_000.0)?;
    let (left, bottom, right, top) = pdf_page_bounds(&document, page_id)?;
    let annotation_left = left + x;
    let annotation_bottom = bottom + y;
    let annotation_right = annotation_left + width;
    let annotation_top = annotation_bottom + height;
    if annotation_right > right + 0.01 || annotation_top > top + 0.01 {
        return Err(anyhow!(
            "PDF Link annotation Rect exceeds the effective page bounds"
        ));
    }
    let description = match arguments.get("description") {
        None => None,
        Some(Value::String(value)) => Some(normalized_pdf_unicode_text(
            value,
            "description",
            MAX_PDF_ANNOTATION_CHARACTERS,
            true,
        )?),
        Some(_) => return Err(anyhow!("description must be a string")),
    };
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
    let destination_type = arguments
        .get("destination_type")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("destination_type is required"))?;
    let mut annotation = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Link",
        "Rect" => vec![
            Object::Real(annotation_left),
            Object::Real(annotation_bottom),
            Object::Real(annotation_right),
            Object::Real(annotation_top),
        ],
        "Border" => vec![Object::Integer(0), Object::Integer(0), Object::Integer(0)],
        "H" => "I",
        "F" => 4,
        "P" => page_id,
    };
    let destination_summary = match destination_type {
        "https" => {
            if arguments.get("destination_page").is_some() {
                return Err(anyhow!(
                    "destination_page is only valid when destination_type is page"
                ));
            }
            let link = validated_pdf_https_link_uri(required_text(arguments, "url")?, "url")?;
            annotation.set(
                "A",
                dictionary! {
                    "S" => "URI",
                    "URI" => text_string(link.uri.as_str()),
                },
            );
            json!({
                "destination_type": "https",
                "origin": link.origin,
                "url_sha256": link.sha256,
                "has_query": link.has_query,
                "has_fragment": link.has_fragment,
            })
        }
        "page" => {
            if arguments.get("url").is_some() {
                return Err(anyhow!("url is only valid when destination_type is https"));
            }
            let destination_page = arguments
                .get("destination_page")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .filter(|value| (1..=MAX_PDF_PAGES as u32).contains(value))
                .ok_or_else(|| {
                    anyhow!("destination_page must be an integer between 1 and {MAX_PDF_PAGES}")
                })?;
            let destination_page_id = page_map
                .get(&destination_page)
                .copied()
                .ok_or_else(|| anyhow!("destination_page {destination_page} does not exist"))?;
            annotation.set(
                "Dest",
                vec![
                    Object::Reference(destination_page_id),
                    Object::Name(b"Fit".to_vec()),
                ],
            );
            json!({
                "destination_type": "page",
                "destination_page": destination_page,
                "destination_mode": "Fit",
            })
        }
        _ => return Err(anyhow!("destination_type must be https or page")),
    };
    if let Some(description) = description.as_deref() {
        annotation.set("Contents", text_string(description));
    }
    if let Some(author) = author.as_deref() {
        annotation.set("T", text_string(author));
    }
    let (_, annotation_index) =
        append_pdf_annotation(&mut document, page_number, page_id, annotation)?;
    let (target_relative, bytes) = save_guarded_annotation_output(
        arguments,
        state,
        request,
        source.as_path(),
        expected_source_sha256.as_str(),
        "PDF source changed while the Link annotation was being prepared; no output was written",
        &mut document,
    )?;
    Ok(json!({
        "created": true,
        "operation": "add_link_annotation",
        "source_path": source_relative,
        "source_sha256": expected_source_sha256,
        "path": target_relative,
        "page": page_number,
        "annotation_index": annotation_index,
        "coordinate_space": "crop_box_relative_lower_left_points",
        "rect": [x, y, x + width, y + height],
        "absolute_rect": [annotation_left, annotation_bottom, annotation_right, annotation_top],
        "description_characters": description.as_ref().map(|value| value.chars().count()).unwrap_or(0),
        "author": author,
        "destination": destination_summary,
        "bytes": bytes,
    }))
}
