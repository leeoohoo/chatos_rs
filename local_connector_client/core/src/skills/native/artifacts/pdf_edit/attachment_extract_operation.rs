// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use lopdf::Document;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{
    input_file, optional_bool, required_lowercase_sha256, required_text, sha256_file,
    MAX_ARTIFACT_BYTES,
};
use super::annotation_operation_common::{select_annotation, SelectedAnnotation};
use super::attachment_common::{
    pdf_attachment_output_path, persist_extracted_pdf_attachment, InspectedPdfEmbeddedFileEntry,
    InspectedPdfFileAttachment,
};
use super::attachment_filespec::inspect_pdf_file_attachment;
use super::embedded_file_inspection::collect_pdf_embedded_files;
use super::{inspect_pdf_annotations, MAX_PDF_ANNOTATION_PREVIEW, MAX_PDF_PAGES};

struct AttachmentExtractionPdf {
    source: PathBuf,
    source_relative: String,
    expected_source_sha256: String,
    document: Document,
}

pub(super) fn extract_pdf_file_attachment(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let AttachmentExtractionPdf {
        source,
        source_relative,
        expected_source_sha256,
        document,
    } = load_attachment_extraction_pdf(arguments, state, request, "page attachment-extraction")?;
    let expected_attachment_sha256 =
        required_lowercase_sha256(arguments, "expected_attachment_sha256")?;
    let page_map = document.get_pages();
    let requested_page = arguments
        .get("page")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| (1..=MAX_PDF_PAGES as u32).contains(value))
        .ok_or_else(|| anyhow!("page must be an integer between 1 and {MAX_PDF_PAGES}"))?;
    if !page_map.contains_key(&requested_page) {
        return Err(anyhow!("page {requested_page} does not exist"));
    }
    inspect_pdf_annotations(&document, &page_map, Some(&json!(requested_page)))?;
    let SelectedAnnotation {
        page_number,
        page_id,
        annotation_index,
        selected_id,
        label,
        dictionary: annotation,
        subtype,
        ..
    } = select_annotation(arguments, &document, &page_map)?;
    selected_id.with_context(|| {
        format!(
            "page {page_number} annotation {annotation_index} is direct and cannot be extracted"
        )
    })?;
    if let Ok(annotation_type) = annotation.get(b"Type") {
        if annotation_type.as_name().ok() != Some(b"Annot") {
            return Err(anyhow!("{label} Type must be /Annot"));
        }
    }
    if subtype != "FileAttachment" {
        return Err(anyhow!(
            "{label} subtype /{subtype} is not a FileAttachment"
        ));
    }
    let attachment = inspect_pdf_file_attachment(&document, &annotation, page_id, label.as_str())?;
    require_attachment_sha256(
        &attachment,
        expected_attachment_sha256.as_str(),
        "PDF attachment",
    )?;

    let (target_relative, attachment_bytes, bytes) = persist_inspected_attachment(
        arguments,
        state,
        request,
        source.as_path(),
        expected_source_sha256.as_str(),
        &attachment,
        expected_attachment_sha256.as_str(),
    )?;
    Ok(json!({
        "created": true,
        "operation": "extract_file_attachment",
        "source_path": source_relative,
        "source_sha256": expected_source_sha256,
        "path": target_relative,
        "page": page_number,
        "annotation_index": annotation_index,
        "attachment_filename": attachment.filename,
        "attachment_mime_type": attachment.format.mime_type,
        "attachment_bytes": attachment_bytes,
        "attachment_sha256": expected_attachment_sha256,
        "bytes": bytes,
    }))
}

pub(super) fn extract_pdf_embedded_file(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let AttachmentExtractionPdf {
        source,
        source_relative,
        expected_source_sha256,
        document,
    } = load_attachment_extraction_pdf(arguments, state, request, "page embedded-file-extraction")?;
    let expected_attachment_sha256 =
        required_lowercase_sha256(arguments, "expected_attachment_sha256")?;
    let embedded_file_index = arguments
        .get("embedded_file_index")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| (1..=MAX_PDF_ANNOTATION_PREVIEW).contains(value))
        .ok_or_else(|| {
            anyhow!(
                "embedded_file_index must be an integer between 1 and {MAX_PDF_ANNOTATION_PREVIEW}"
            )
        })?;

    let (entries, _) = collect_pdf_embedded_files(&document)?;
    let entry = entries
        .into_iter()
        .nth(embedded_file_index - 1)
        .ok_or_else(|| {
            anyhow!("embedded_file_index {embedded_file_index} does not exist in the PDF")
        })?;
    let InspectedPdfEmbeddedFileEntry { name, attachment } = entry;
    require_attachment_sha256(
        &attachment,
        expected_attachment_sha256.as_str(),
        "PDF embedded file",
    )?;

    let (target_relative, attachment_bytes, bytes) = persist_inspected_attachment(
        arguments,
        state,
        request,
        source.as_path(),
        expected_source_sha256.as_str(),
        &attachment,
        expected_attachment_sha256.as_str(),
    )?;
    Ok(json!({
        "created": true,
        "operation": "extract_embedded_file",
        "source_path": source_relative,
        "source_sha256": expected_source_sha256,
        "path": target_relative,
        "embedded_file_index": embedded_file_index,
        "name": name,
        "attachment_filename": attachment.filename,
        "attachment_mime_type": attachment.format.mime_type,
        "attachment_bytes": attachment_bytes,
        "attachment_sha256": expected_attachment_sha256,
        "bytes": bytes,
    }))
}

fn load_attachment_extraction_pdf(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    page_limit_label: &str,
) -> Result<AttachmentExtractionPdf> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let expected_source_sha256 = required_lowercase_sha256(arguments, "expected_source_sha256")?;
    let source_bytes =
        fs::read(source.as_path()).with_context(|| format!("read PDF {}", source.display()))?;
    if source_bytes.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("local artifact exceeds the 100 MiB safety limit"));
    }
    let actual_source_sha256 = hex::encode(Sha256::digest(source_bytes.as_slice()));
    if actual_source_sha256 != expected_source_sha256 {
        return Err(anyhow!(
            "PDF source SHA-256 does not match expected_source_sha256; inspect the current file again"
        ));
    }
    if sha256_file(source.as_path())? != expected_source_sha256 {
        return Err(anyhow!(
            "PDF source changed while it was being read; inspect the current file again"
        ));
    }

    let document = Document::load_mem(source_bytes.as_slice())
        .with_context(|| format!("open PDF {}", source.display()))?;
    if document.is_encrypted() {
        return Err(anyhow!(
            "encrypted PDFs cannot be read without an explicit decryption workflow"
        ));
    }
    let page_count = document.get_pages().len();
    if page_count == 0 {
        return Err(anyhow!("PDF contains no pages: {}", source.display()));
    }
    if page_count > MAX_PDF_PAGES {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_PAGES} {page_limit_label} safety limit"
        ));
    }
    Ok(AttachmentExtractionPdf {
        source,
        source_relative,
        expected_source_sha256,
        document,
    })
}

fn require_attachment_sha256(
    attachment: &InspectedPdfFileAttachment,
    expected_attachment_sha256: &str,
    label: &str,
) -> Result<()> {
    if attachment.sha256 != expected_attachment_sha256 {
        return Err(anyhow!(
            "{label} SHA-256 does not match expected_attachment_sha256; inspect the current file again"
        ));
    }
    Ok(())
}

fn persist_inspected_attachment(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    source: &Path,
    expected_source_sha256: &str,
    attachment: &InspectedPdfFileAttachment,
    expected_attachment_sha256: &str,
) -> Result<(String, usize, u64)> {
    let overwrite = optional_bool(arguments, "overwrite");
    let (target, target_relative) = pdf_attachment_output_path(
        state,
        request,
        required_text(arguments, "target_path")?,
        source,
        attachment.format,
        overwrite,
    )?;
    let attachment_bytes = attachment.content.len();
    let bytes = persist_extracted_pdf_attachment(
        source,
        expected_source_sha256,
        target.as_path(),
        attachment.content.as_slice(),
        expected_attachment_sha256,
        overwrite,
    )?;
    Ok((target_relative, attachment_bytes, bytes))
}
