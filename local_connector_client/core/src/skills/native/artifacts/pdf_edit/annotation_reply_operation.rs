// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use lopdf::{dictionary, text_string};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::required_text;
use super::annotation_common::{is_pdf_markup_subtype, pdf_annotation_number_array};
use super::annotation_operation_common::{
    append_pdf_annotation, ensure_annotation_capacity, load_guarded_annotation_pdf,
    save_guarded_annotation_output, select_annotation, GuardedAnnotationPdf, SelectedAnnotation,
};
use super::{
    normalized_pdf_unicode_text, MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS,
    MAX_PDF_ANNOTATION_CHARACTERS,
};

pub(super) fn add_pdf_annotation_reply(
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

    let SelectedAnnotation {
        page_number,
        page_id,
        annotation_index,
        selected_id,
        label: parent_label,
        dictionary: parent,
        subtype,
        ..
    } = select_annotation(arguments, &document, &page_map)?;
    let parent_id = selected_id.with_context(|| {
        format!(
            "page {page_number} annotation {annotation_index} is direct and cannot receive a reply"
        )
    })?;
    if subtype != "Text" && !is_pdf_markup_subtype(subtype.as_str()) {
        return Err(anyhow!(
            "{parent_label} subtype /{subtype} cannot receive a standard annotation reply"
        ));
    }
    if parent.has(b"IRT") {
        return Err(anyhow!(
            "{parent_label} is already a reply or group member; replies-to-replies are not supported"
        ));
    }
    if let Ok(parent_page) = parent.get(b"P") {
        let parent_page_id = parent_page
            .as_reference()
            .with_context(|| format!("{parent_label} P must be an indirect page reference"))?;
        if parent_page_id != page_id {
            return Err(anyhow!(
                "{parent_label} P does not reference physical page {page_number}"
            ));
        }
    }
    let parent_rect_values =
        pdf_annotation_number_array(&parent, b"Rect", 4, 4, parent_label.as_str())?;
    if parent_rect_values[2] <= parent_rect_values[0]
        || parent_rect_values[3] <= parent_rect_values[1]
    {
        return Err(anyhow!(
            "{parent_label} Rect must have positive width and height"
        ));
    }
    let parent_rect = parent
        .get(b"Rect")
        .with_context(|| format!("{parent_label} Rect is missing"))?
        .clone();
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

    let mut reply = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Text",
        "Rect" => parent_rect,
        "Contents" => text_string(text.as_str()),
        "Name" => "Comment",
        "Open" => false,
        "F" => 4,
        "P" => page_id,
        "IRT" => parent_id,
        "RT" => "R",
    };
    if let Some(author) = author.as_deref() {
        reply.set("T", text_string(author));
    }
    let (_, reply_annotation_index) =
        append_pdf_annotation(&mut document, page_number, page_id, reply)?;
    let (target_relative, bytes) = save_guarded_annotation_output(
        arguments,
        state,
        request,
        source.as_path(),
        expected_source_sha256.as_str(),
        "PDF source changed while the annotation reply was being prepared; no output was written",
        &mut document,
    )?;
    Ok(json!({
        "created": true,
        "operation": "add_annotation_reply",
        "source_path": source_relative,
        "source_sha256": expected_source_sha256,
        "path": target_relative,
        "page": page_number,
        "parent_annotation_index": annotation_index,
        "reply_annotation_index": reply_annotation_index,
        "characters": text.chars().count(),
        "contents_sha256": hex::encode(Sha256::digest(text.as_bytes())),
        "author": author,
        "relation_type": "reply",
        "bytes": bytes,
    }))
}
