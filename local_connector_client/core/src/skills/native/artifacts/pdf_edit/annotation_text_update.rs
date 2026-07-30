// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use lopdf::{text_string, Object};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::required_text;
use super::annotation_common::{is_pdf_markup_subtype, pdf_page_annotations};
use super::annotation_operation_common::{
    annotation_relation_type, load_guarded_annotation_pdf, save_guarded_annotation_output,
    select_annotation, GuardedAnnotationPdf, SelectedAnnotation,
};
use super::{
    inspect_pdf_annotations, normalized_pdf_unicode_text, optional_bounded_pdf_text,
    pdf_annotation_text_remove_fields, resolved_pdf_dictionary,
    MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS, MAX_PDF_ANNOTATION_CHARACTERS,
};

pub(super) fn update_pdf_annotation_text(
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
    let initial_annotation_count = annotation_inspection
        .get("count")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("PDF annotation inspection did not return a valid count"))?;

    let SelectedAnnotation {
        page_number,
        page_id,
        annotation_index,
        mut annotations,
        selected_id,
        label: selected_label,
        dictionary: mut selected,
        subtype,
    } = select_annotation(arguments, &document, &page_map)?;
    if subtype != "Text" && !is_pdf_markup_subtype(subtype.as_str()) {
        return Err(anyhow!(
            "{selected_label} subtype /{subtype} is not eligible for safe annotation text updates"
        ));
    }
    if required_text(arguments, "expected_subtype")? != subtype {
        return Err(anyhow!(
            "expected_subtype does not match {selected_label}; inspect the current PDF again"
        ));
    }
    let relation_type = annotation_relation_type(&selected, selected_label.as_str())?;
    if required_text(arguments, "expected_relation_type")? != relation_type {
        return Err(anyhow!(
            "expected_relation_type does not match {selected_label}; inspect the current PDF again"
        ));
    }

    let text = match arguments.get("text") {
        None => None,
        Some(Value::String(value)) => Some(normalized_pdf_unicode_text(
            value,
            "text",
            MAX_PDF_ANNOTATION_CHARACTERS,
            true,
        )?),
        Some(_) => return Err(anyhow!("text must be a string")),
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
    let remove_fields = pdf_annotation_text_remove_fields(arguments)?;
    if text.is_some() && remove_fields.iter().any(|field| field == "text") {
        return Err(anyhow!(
            "PDF annotation text cannot be both updated and removed"
        ));
    }
    if author.is_some() && remove_fields.iter().any(|field| field == "author") {
        return Err(anyhow!(
            "PDF annotation author cannot be both updated and removed"
        ));
    }
    if text.is_none() && author.is_none() && remove_fields.is_empty() {
        return Err(anyhow!(
            "PDF annotation text update requires text, author, or remove_fields"
        ));
    }

    let current_text = optional_bounded_pdf_text(
        &selected,
        b"Contents",
        selected_label.as_str(),
        MAX_PDF_ANNOTATION_CHARACTERS,
        true,
    )?;
    let current_author = optional_bounded_pdf_text(
        &selected,
        b"T",
        selected_label.as_str(),
        MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS,
        false,
    )?;
    let mut updated_fields = Vec::new();
    let mut removed_fields = Vec::new();
    if let Some(text) = text.as_deref() {
        if current_text.as_deref() != Some(text) {
            selected.set("Contents", text_string(text));
            updated_fields.push("text");
        }
    }
    if let Some(author) = author.as_deref() {
        if current_author.as_deref() != Some(author) {
            selected.set("T", text_string(author));
            updated_fields.push("author");
        }
    }
    for field in remove_fields {
        let key = match field.as_str() {
            "text" => b"Contents".as_slice(),
            "author" => b"T".as_slice(),
            _ => return Err(anyhow!("remove_fields entries must be text or author")),
        };
        if selected.has(key) {
            selected.remove(key);
            removed_fields.push(field);
        }
    }
    if updated_fields.is_empty() && removed_fields.is_empty() {
        return Err(anyhow!(
            "PDF annotation text update would not change the document"
        ));
    }

    if let Some(selected_id) = selected_id {
        document
            .objects
            .insert(selected_id, Object::Dictionary(selected));
    } else {
        annotations[annotation_index - 1] = Object::Dictionary(selected);
        document
            .get_object_mut(page_id)
            .and_then(Object::as_dict_mut)
            .with_context(|| format!("read page {page_number} dictionary"))?
            .set("Annots", annotations);
    }

    let updated_page_map = document.get_pages();
    let updated_inspection =
        inspect_pdf_annotations(&document, &updated_page_map, Some(&json!(page_number)))?;
    if updated_inspection.get("count").and_then(Value::as_u64) != Some(initial_annotation_count) {
        return Err(anyhow!(
            "PDF annotation text update changed the annotation count unexpectedly"
        ));
    }
    let updated_object = pdf_page_annotations(
        &document,
        page_id,
        format!("updated page {page_number} Annots").as_str(),
    )?
    .get(annotation_index - 1)
    .cloned()
    .ok_or_else(|| anyhow!("updated annotation disappeared unexpectedly"))?;
    let updated = resolved_pdf_dictionary(&document, updated_object, selected_label.as_str())?;
    let final_text = optional_bounded_pdf_text(
        &updated,
        b"Contents",
        selected_label.as_str(),
        MAX_PDF_ANNOTATION_CHARACTERS,
        true,
    )?;
    let final_author = optional_bounded_pdf_text(
        &updated,
        b"T",
        selected_label.as_str(),
        MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS,
        false,
    )?;

    let (target_relative, bytes) = save_guarded_annotation_output(
        arguments,
        state,
        request,
        source.as_path(),
        expected_source_sha256.as_str(),
        "PDF source changed while the annotation text update was being prepared; no output was written",
        &mut document,
    )?;
    Ok(json!({
        "updated": true,
        "operation": "update_annotation_text",
        "source_path": source_relative,
        "source_sha256": expected_source_sha256,
        "path": target_relative,
        "page": page_number,
        "annotation_index": annotation_index,
        "subtype": subtype,
        "relation_type": relation_type,
        "updated_indirect_object": selected_id.is_some(),
        "updated_fields": updated_fields,
        "removed_fields": removed_fields,
        "text_characters": final_text.as_ref().map(|value| value.chars().count()),
        "text_sha256": final_text.as_ref().map(|value| hex::encode(Sha256::digest(value.as_bytes()))),
        "author": final_author,
        "annotation_count": initial_annotation_count,
        "bytes": bytes,
    }))
}
