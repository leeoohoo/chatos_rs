// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use anyhow::{anyhow, Context, Result};
use lopdf::{dictionary, text_string, Object, Stream};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{optional_bool, required_text, safe_workspace_path, sha256_file};
use super::annotation_operation_common::{
    append_pdf_annotation, ensure_annotation_capacity, load_guarded_annotation_pdf,
    GuardedAnnotationPdf,
};
use super::attachment_common::{
    pdf_attachment_format_from_filename, portable_pdf_attachment_filename,
    validate_pdf_attachment_content, validate_pdf_attachment_filename,
};
use super::generation_common::{bounded_pdf_number, required_bounded_pdf_number};
use super::package_write::{pdf_output_path, save_pdf_document_with_file_guards};
use super::{
    normalized_pdf_unicode_text, pdf_page_bounds, pdf_page_rotation, PdfFileGuard,
    MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS, MAX_PDF_ANNOTATION_CHARACTERS, MAX_PDF_ATTACHMENT_BYTES,
    MAX_PDF_PAGES,
};

pub(super) fn add_pdf_file_attachment_annotation(
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

    let attachment_requested = required_text(arguments, "attachment_path")?;
    let (attachment, attachment_relative) =
        safe_workspace_path(state, request, attachment_requested)?;
    let attachment_metadata = fs::symlink_metadata(attachment.as_path())
        .with_context(|| format!("inspect PDF attachment {}", attachment.display()))?;
    if attachment_metadata.file_type().is_symlink() || !attachment_metadata.is_file() {
        return Err(anyhow!(
            "PDF attachment must be a regular non-symlink workspace file"
        ));
    }
    let attachment_filename = attachment
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("PDF attachment filename must be valid Unicode"))?
        .to_string();
    validate_pdf_attachment_filename(attachment_filename.as_str(), "attachment filename")?;
    let attachment_format = pdf_attachment_format_from_filename(attachment_filename.as_str())?;
    let attachment_bytes = fs::read(attachment.as_path())
        .with_context(|| format!("read PDF attachment {}", attachment.display()))?;
    if attachment_bytes.is_empty() || attachment_bytes.len() > MAX_PDF_ATTACHMENT_BYTES {
        return Err(anyhow!(
            "PDF attachment must contain between 1 byte and {} MiB",
            MAX_PDF_ATTACHMENT_BYTES / (1024 * 1024)
        ));
    }
    validate_pdf_attachment_content(attachment_format, attachment_bytes.as_slice())?;
    let attachment_sha256 = hex::encode(Sha256::digest(attachment_bytes.as_slice()));
    let rechecked_attachment_metadata = fs::symlink_metadata(attachment.as_path())
        .with_context(|| format!("reinspect PDF attachment {}", attachment.display()))?;
    if rechecked_attachment_metadata.file_type().is_symlink()
        || !rechecked_attachment_metadata.is_file()
        || rechecked_attachment_metadata.len() != attachment_bytes.len() as u64
        || sha256_file(attachment.as_path())? != attachment_sha256
    {
        return Err(anyhow!(
            "PDF attachment changed while it was being read; retry with the current file"
        ));
    }
    if same_file::is_same_file(source.as_path(), attachment.as_path())? {
        return Err(anyhow!(
            "PDF source and attachment must be distinct files and must not be hard links"
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
            "PDF file-attachment annotation geometry currently requires an unrotated page"
        ));
    }
    let x = required_bounded_pdf_number(arguments, "x", 0.0, 20_000.0)?;
    let y = required_bounded_pdf_number(arguments, "y", 0.0, 20_000.0)?;
    let icon_size = bounded_pdf_number(arguments, "icon_size", 24.0, 12.0, 72.0)?;
    let (left, bottom, right, top) = pdf_page_bounds(&document, page_id)?;
    let annotation_left = left + x;
    let annotation_bottom = bottom + y;
    let annotation_right = annotation_left + icon_size;
    let annotation_top = annotation_bottom + icon_size;
    if annotation_right > right + 0.01 || annotation_top > top + 0.01 {
        return Err(anyhow!(
            "PDF file-attachment annotation Rect exceeds the effective page bounds"
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
    let icon = arguments
        .get("icon")
        .and_then(Value::as_str)
        .unwrap_or("push_pin");
    let icon_name = match icon {
        "graph" => "Graph",
        "push_pin" => "PushPin",
        "paperclip" => "Paperclip",
        "tag" => "Tag",
        _ => return Err(anyhow!("icon is not a supported PDF FileAttachment icon")),
    };
    let portable_filename =
        portable_pdf_attachment_filename(attachment_filename.as_str(), attachment_format);

    let embedded_file_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "EmbeddedFile",
            "Subtype" => Object::Name(attachment_format.mime_type.as_bytes().to_vec()),
            "Params" => dictionary! {
                "Size" => i64::try_from(attachment_bytes.len())?,
            },
        },
        attachment_bytes.clone(),
    ));
    let mut filespec = dictionary! {
        "Type" => "Filespec",
        "F" => text_string(portable_filename.as_str()),
        "UF" => text_string(attachment_filename.as_str()),
        "EF" => dictionary! {
            "F" => embedded_file_id,
            "UF" => embedded_file_id,
        },
    };
    if let Some(description) = description.as_deref() {
        filespec.set("Desc", text_string(description));
    }
    let filespec_id = document.add_object(filespec);
    let mut annotation = dictionary! {
        "Type" => "Annot",
        "Subtype" => "FileAttachment",
        "Rect" => vec![
            Object::Real(annotation_left),
            Object::Real(annotation_bottom),
            Object::Real(annotation_right),
            Object::Real(annotation_top),
        ],
        "FS" => filespec_id,
        "Name" => icon_name,
        "F" => 4,
        "P" => page_id,
    };
    if let Some(description) = description.as_deref() {
        annotation.set("Contents", text_string(description));
    }
    if let Some(author) = author.as_deref() {
        annotation.set("T", text_string(author));
    }
    let (_, annotation_index) =
        append_pdf_annotation(&mut document, page_number, page_id, annotation)?;

    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = pdf_output_path(
        state,
        request,
        target_requested,
        &[source.clone(), attachment.clone()],
    )?;
    let guards = [
        PdfFileGuard {
            path: source.as_path(),
            expected_sha256: expected_source_sha256.as_str(),
            changed_message:
                "PDF source changed while the file attachment was being prepared; no output was written",
            require_regular_non_symlink: false,
        },
        PdfFileGuard {
            path: attachment.as_path(),
            expected_sha256: attachment_sha256.as_str(),
            changed_message:
                "PDF attachment changed while the file attachment was being prepared; no output was written",
            require_regular_non_symlink: true,
        },
    ];
    let bytes = save_pdf_document_with_file_guards(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
        guards.as_slice(),
    )?;
    Ok(json!({
        "created": true,
        "operation": "add_file_attachment_annotation",
        "source_path": source_relative,
        "source_sha256": expected_source_sha256,
        "path": target_relative,
        "page": page_number,
        "annotation_index": annotation_index,
        "coordinate_space": "crop_box_relative_lower_left_points",
        "rect": [x, y, x + icon_size, y + icon_size],
        "absolute_rect": [annotation_left, annotation_bottom, annotation_right, annotation_top],
        "x": x,
        "y": y,
        "icon_size": icon_size,
        "icon": icon,
        "attachment_path": attachment_relative,
        "attachment_filename": attachment_filename,
        "portable_filename": portable_filename,
        "attachment_mime_type": attachment_format.mime_type,
        "attachment_bytes": attachment_bytes.len(),
        "attachment_sha256": attachment_sha256,
        "description_characters": description.as_ref().map(|value| value.chars().count()).unwrap_or(0),
        "author": author,
        "bytes": bytes,
    }))
}
