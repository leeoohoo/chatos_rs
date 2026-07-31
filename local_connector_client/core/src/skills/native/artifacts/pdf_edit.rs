// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use lopdf::{decode_text_string, Dictionary, Document, Object, ObjectId};
use serde_json::Value;

use crate::relay::RelayRequest;
use crate::LocalState;

mod annotation_common;
mod annotation_delete_operation;
mod annotation_inspection;
mod annotation_link;
mod annotation_link_operation;
mod annotation_markup_operation;
mod annotation_operation_common;
mod annotation_reply_operation;
mod annotation_text_operation;
mod annotation_text_update;
mod attachment_add_operation;
mod attachment_common;
mod attachment_extract_operation;
mod attachment_filespec;
mod embedded_file_inspection;
mod embedded_image;
mod form_decode;
mod form_field_description;
mod form_field_options;
mod form_inspection;
mod form_model;
mod form_operations;
mod form_tree;
mod form_validation;
mod generation_common;
mod image_generation;
mod metadata;
mod package_write;
mod page_operations;
mod page_selection;
mod page_tree;
mod stamp_image_operation;
mod stamp_resource_common;
mod stamp_text_common;
mod stamp_text_operation;
mod text_generation;

use generation_common::bounded_pdf_number;
use package_write::{load_editable_pdf, pdf_output_path, save_pdf_document};
use page_selection::{
    optional_page_numbers, required_page_numbers, required_page_sequence, required_pdf_paths,
};
use page_tree::{
    inherited_page_attribute, materialized_page, merge_documents, validate_arrangeable_pdf,
};

const MAX_PDF_INPUTS: usize = 20;
const MAX_PDF_PAGES: usize = 5_000;
const MAX_PDF_STAMP_CHARACTERS: usize = 256;
const MAX_PDF_ANNOTATIONS: usize = 10_000;
const MAX_PDF_ANNOTATION_PREVIEW: usize = 100;
const MAX_PDF_ANNOTATION_CHARACTERS: usize = 4_096;
const MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS: usize = 256;
const MAX_PDF_LINK_URL_CHARACTERS: usize = 2_048;
const MAX_PDF_MARKUP_RECTANGLES: usize = 64;
const MAX_PDF_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;
const MAX_PDF_ATTACHMENT_TOTAL_BYTES: usize = 100 * 1024 * 1024;
const MAX_PDF_ATTACHMENT_FILENAME_CHARACTERS: usize = 255;
const MAX_PDF_EMBEDDED_FILE_NAME_CHARACTERS: usize = 512;
const MAX_PDF_EMBEDDED_FILE_TREE_DEPTH: usize = 32;
const MAX_PDF_EMBEDDED_FILE_TREE_NODES: usize = 10_000;
const MAX_PDF_STAMP_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_PDF_STAMP_IMAGE_EDGE: u32 = 10_000;
const MAX_PDF_STAMP_IMAGE_PIXELS: u64 = 16_000_000;
const MAX_PDF_STAMP_DECODED_BYTES: usize = 64 * 1024 * 1024;
const MAX_MERGED_INPUT_BYTES: u64 = 200 * 1024 * 1024;

struct ValidatedPdfHttpsLink {
    uri: String,
    origin: String,
    sha256: String,
    has_query: bool,
    has_fragment: bool,
}

struct PdfFileGuard<'a> {
    path: &'a Path,
    expected_sha256: &'a str,
    changed_message: &'a str,
    require_regular_non_symlink: bool,
}

pub(super) fn create_text_pdf(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    text_generation::create_text_pdf(arguments, state, request)
}

pub(super) fn create_pdf_from_images(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    image_generation::create_pdf_from_images(arguments, state, request)
}

pub(super) fn update_pdf_metadata(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    metadata::update_pdf_metadata(arguments, state, request)
}

pub(super) fn merge_pdfs(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    page_operations::merge_pdfs(arguments, state, request)
}

pub(super) fn extract_pdf_pages(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    page_operations::extract_pdf_pages(arguments, state, request)
}

pub(super) fn arrange_pdf_pages(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    page_operations::arrange_pdf_pages(arguments, state, request)
}

pub(super) fn rotate_pdf_pages(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    page_operations::rotate_pdf_pages(arguments, state, request)
}

pub(super) fn inspect_pdf_metadata(document: &Document) -> Result<Value> {
    metadata::inspect_pdf_metadata(document)
}

pub(super) fn inspect_pdf_form(document: &Document) -> Result<Value> {
    form_inspection::inspect_pdf_form(document)
}

pub(super) fn fill_pdf_form_fields(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    form_operations::fill_pdf_form_fields(arguments, state, request)
}

pub(super) fn inspect_pdf_page_geometry(
    document: &Document,
    page_map: &BTreeMap<u32, ObjectId>,
    requested_page: Option<&Value>,
) -> Result<Value> {
    annotation_inspection::inspect_pdf_page_geometry(document, page_map, requested_page)
}

pub(super) fn inspect_pdf_annotations(
    document: &Document,
    page_map: &BTreeMap<u32, ObjectId>,
    requested_preview_page: Option<&Value>,
) -> Result<Value> {
    annotation_inspection::inspect_pdf_annotations(document, page_map, requested_preview_page)
}

pub(super) fn add_pdf_text_annotation(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    annotation_text_operation::add_pdf_text_annotation(arguments, state, request)
}

pub(super) fn add_pdf_markup_annotation(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    annotation_markup_operation::add_pdf_markup_annotation(arguments, state, request)
}

pub(super) fn add_pdf_link_annotation(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    annotation_link_operation::add_pdf_link_annotation(arguments, state, request)
}

pub(super) fn add_pdf_annotation_reply(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    annotation_reply_operation::add_pdf_annotation_reply(arguments, state, request)
}

pub(super) fn update_pdf_annotation_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    annotation_text_update::update_pdf_annotation_text(arguments, state, request)
}

pub(super) fn delete_pdf_annotation(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    annotation_delete_operation::delete_pdf_annotation(arguments, state, request)
}

pub(super) fn add_pdf_file_attachment_annotation(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    attachment_add_operation::add_pdf_file_attachment_annotation(arguments, state, request)
}

pub(super) fn extract_pdf_file_attachment(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    attachment_extract_operation::extract_pdf_file_attachment(arguments, state, request)
}

pub(super) fn extract_pdf_embedded_file(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    attachment_extract_operation::extract_pdf_embedded_file(arguments, state, request)
}

pub(super) fn stamp_pdf_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    stamp_text_operation::stamp_pdf_text(arguments, state, request)
}

pub(super) fn stamp_pdf_page_numbers(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    stamp_text_operation::stamp_pdf_page_numbers(arguments, state, request)
}

pub(super) fn stamp_pdf_image(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    stamp_image_operation::stamp_pdf_image(arguments, state, request)
}

fn pdf_page_bounds(document: &Document, page_id: ObjectId) -> Result<(f32, f32, f32, f32)> {
    let value = inherited_page_attribute(document, page_id, b"CropBox")
        .or_else(|| inherited_page_attribute(document, page_id, b"MediaBox"))
        .ok_or_else(|| anyhow!("PDF page has no inherited MediaBox or CropBox"))?;
    let values = resolved_pdf_object(document, value, "page box")?
        .as_array()
        .context("PDF page box must be an array")?
        .clone();
    if values.len() != 4 {
        return Err(anyhow!("PDF page box must contain exactly four numbers"));
    }
    let numbers = values
        .iter()
        .map(|value| value.as_float().context("PDF page box must be numeric"))
        .collect::<Result<Vec<_>>>()?;
    let (left, bottom, right, top) = (numbers[0], numbers[1], numbers[2], numbers[3]);
    if ![left, bottom, right, top]
        .iter()
        .all(|value| value.is_finite())
        || right <= left
        || top <= bottom
        || right - left > 20_000.0
        || top - bottom > 20_000.0
    {
        return Err(anyhow!("PDF page box is invalid or exceeds local limits"));
    }
    Ok((left, bottom, right, top))
}

fn pdf_page_rotation(document: &Document, page_id: ObjectId) -> Result<i64> {
    match inherited_page_attribute(document, page_id, b"Rotate") {
        None => Ok(0),
        Some(value) => Ok(resolved_pdf_object(document, value, "page Rotate")?
            .as_i64()
            .context("PDF page Rotate must be an integer")?
            .rem_euclid(360)),
    }
}

fn resolved_pdf_object(document: &Document, mut value: Object, label: &str) -> Result<Object> {
    let mut visited = HashSet::new();
    while let Object::Reference(object_id) = value {
        if !visited.insert(object_id) {
            return Err(anyhow!("{label} contains a reference cycle"));
        }
        value = document
            .get_object(object_id)
            .with_context(|| format!("resolve {label}"))?
            .clone();
    }
    Ok(value)
}

fn resolved_pdf_dictionary(document: &Document, value: Object, label: &str) -> Result<Dictionary> {
    resolved_pdf_object(document, value, label)?
        .as_dict()
        .with_context(|| format!("{label} must be a dictionary"))
        .cloned()
}

fn pdf_annotation_text_remove_fields(arguments: &Value) -> Result<Vec<String>> {
    let Some(value) = arguments.get("remove_fields") else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| anyhow!("remove_fields must be an array"))?;
    if values.is_empty() || values.len() > 2 {
        return Err(anyhow!(
            "remove_fields must contain between 1 and 2 annotation fields"
        ));
    }
    let mut fields = Vec::with_capacity(values.len());
    let mut seen = HashSet::with_capacity(values.len());
    for value in values {
        let field = value
            .as_str()
            .filter(|field| matches!(*field, "text" | "author"))
            .ok_or_else(|| anyhow!("remove_fields entries must be text or author"))?;
        if !seen.insert(field) {
            return Err(anyhow!("remove_fields entries must be unique"));
        }
        fields.push(field.to_string());
    }
    Ok(fields)
}

pub(super) fn inspect_pdf_embedded_files(document: &Document) -> Result<Value> {
    embedded_file_inspection::inspect_pdf_embedded_files(document)
}

fn optional_bounded_pdf_text(
    dictionary: &Dictionary,
    key: &[u8],
    label: &str,
    max_characters: usize,
    multiline: bool,
) -> Result<Option<String>> {
    let Ok(value) = dictionary.get(key) else {
        return Ok(None);
    };
    let decoded = decode_text_string(value).with_context(|| {
        format!(
            "decode {label} {} text string",
            String::from_utf8_lossy(key)
        )
    })?;
    normalized_pdf_unicode_text(
        decoded.as_str(),
        format!("{label} {}", String::from_utf8_lossy(key)).as_str(),
        max_characters,
        multiline,
    )
    .map(Some)
}

fn normalized_pdf_unicode_text(
    value: &str,
    field: &str,
    max_characters: usize,
    multiline: bool,
) -> Result<String> {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    if normalized.trim().is_empty() {
        return Err(anyhow!("{field} must contain non-whitespace text"));
    }
    let characters = normalized.chars().count();
    if characters > max_characters {
        return Err(anyhow!(
            "{field} exceeds the {max_characters} character limit"
        ));
    }
    if normalized
        .chars()
        .any(|character| character.is_control() && !(multiline && matches!(character, '\n' | '\t')))
    {
        return Err(anyhow!("{field} contains an unsupported control character"));
    }
    Ok(normalized)
}

fn pdf_annotation_rect(
    bounds: (f32, f32, f32, f32),
    position: &str,
    size: f32,
    margin: f32,
) -> Result<[f32; 4]> {
    let (left, bottom, right, top) = bounds;
    if size + margin * 2.0 > right - left || size + margin * 2.0 > top - bottom {
        return Err(anyhow!(
            "PDF Text annotation does not fit inside the page bounds and margins"
        ));
    }
    let x = if position.ends_with("_left") {
        left + margin
    } else {
        right - margin - size
    };
    let y = if position.starts_with("top_") {
        top - margin - size
    } else {
        bottom + margin
    };
    Ok([x, y, x + size, y + size])
}
