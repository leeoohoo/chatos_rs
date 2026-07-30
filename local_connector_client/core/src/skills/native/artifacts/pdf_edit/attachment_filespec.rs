// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use lopdf::{decode_text_string, Dictionary, Document, Object, ObjectId};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::annotation_common::pdf_annotation_number_array;
use super::attachment_common::{
    pdf_attachment_format_from_filename, validate_pdf_attachment_content,
    validate_pdf_attachment_filename, InspectedPdfFileAttachment,
};
use super::{
    optional_bounded_pdf_text, pdf_page_bounds, resolved_pdf_dictionary,
    MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS, MAX_PDF_ANNOTATION_CHARACTERS, MAX_PDF_ATTACHMENT_BYTES,
};

pub(super) fn inspect_pdf_file_attachment(
    document: &Document,
    annotation: &Dictionary,
    page_id: ObjectId,
    label: &str,
) -> Result<InspectedPdfFileAttachment> {
    if let Ok(page) = annotation.get(b"P") {
        let referenced_page = page
            .as_reference()
            .with_context(|| format!("{label} P must be an indirect page reference"))?;
        if referenced_page != page_id {
            return Err(anyhow!("{label} P does not reference its physical page"));
        }
    }
    let rect = pdf_annotation_number_array(annotation, b"Rect", 4, 4, label)?;
    if rect[2] <= rect[0] || rect[3] <= rect[1] {
        return Err(anyhow!("{label} Rect must have positive width and height"));
    }
    let (left, bottom, right, top) = pdf_page_bounds(document, page_id)?;
    if rect[0] < left - 0.01
        || rect[1] < bottom - 0.01
        || rect[2] > right + 0.01
        || rect[3] > top + 0.01
    {
        return Err(anyhow!("{label} Rect exceeds the effective page bounds"));
    }

    let filespec_id = annotation
        .get(b"FS")
        .and_then(Object::as_reference)
        .with_context(|| format!("{label} FS must be an indirect Filespec reference"))?;
    let mut attachment = inspect_pdf_embedded_filespec(document, filespec_id, label)?;
    let contents = optional_bounded_pdf_text(
        annotation,
        b"Contents",
        label,
        MAX_PDF_ANNOTATION_CHARACTERS,
        true,
    )?;
    let author = optional_bounded_pdf_text(
        annotation,
        b"T",
        label,
        MAX_PDF_ANNOTATION_AUTHOR_CHARACTERS,
        false,
    )?;
    let icon = annotation
        .get(b"Name")
        .and_then(Object::as_name)
        .ok()
        .map(|value| String::from_utf8_lossy(value).to_string())
        .unwrap_or_else(|| "PushPin".to_string());
    if !matches!(icon.as_str(), "Graph" | "PushPin" | "Paperclip" | "Tag") {
        return Err(anyhow!("{label} uses an unsupported FileAttachment icon"));
    }
    attachment.metadata["contents"] = contents.map(Value::String).unwrap_or(Value::Null);
    attachment.metadata["author"] = author.map(Value::String).unwrap_or(Value::Null);
    attachment.metadata["icon"] = Value::String(icon);
    attachment.metadata["rect"] = json!(rect);
    Ok(attachment)
}

pub(super) fn inspect_pdf_embedded_filespec(
    document: &Document,
    filespec_id: ObjectId,
    label: &str,
) -> Result<InspectedPdfFileAttachment> {
    let filespec = document
        .get_object(filespec_id)
        .and_then(Object::as_dict)
        .with_context(|| format!("read {label} Filespec dictionary"))?;
    if filespec.get(b"Type").and_then(Object::as_name).ok() != Some(b"Filespec") {
        return Err(anyhow!("{label} Filespec Type must be /Filespec"));
    }
    let portable_filename = filespec
        .get(b"F")
        .map(decode_text_string)
        .with_context(|| format!("{label} Filespec F is missing"))?
        .with_context(|| format!("decode {label} Filespec F"))?;
    if !portable_filename.is_ascii() {
        return Err(anyhow!(
            "{label} Filespec F must be an ASCII portable filename"
        ));
    }
    validate_pdf_attachment_filename(portable_filename.as_str(), "Filespec F")?;
    let filename = filespec
        .get(b"UF")
        .map(decode_text_string)
        .with_context(|| format!("{label} Filespec UF is missing"))?
        .with_context(|| format!("decode {label} Filespec UF"))?;
    validate_pdf_attachment_filename(filename.as_str(), "Filespec UF")?;
    let format = pdf_attachment_format_from_filename(filename.as_str())?;
    let portable_format = pdf_attachment_format_from_filename(portable_filename.as_str())?;
    if portable_format.extension != format.extension {
        return Err(anyhow!(
            "{label} Filespec F and UF must use the same supported extension"
        ));
    }

    let embedded_files = resolved_pdf_dictionary(
        document,
        filespec
            .get(b"EF")
            .with_context(|| format!("{label} Filespec EF is missing"))?
            .clone(),
        format!("{label} Filespec EF").as_str(),
    )?;
    let embedded_file_id = embedded_files
        .get(b"F")
        .and_then(Object::as_reference)
        .with_context(|| format!("{label} Filespec EF/F must be an indirect stream reference"))?;
    let unicode_embedded_file_id = embedded_files
        .get(b"UF")
        .and_then(Object::as_reference)
        .with_context(|| format!("{label} Filespec EF/UF must be an indirect stream reference"))?;
    if embedded_file_id != unicode_embedded_file_id {
        return Err(anyhow!(
            "{label} Filespec EF/F and EF/UF must reference the same embedded file stream"
        ));
    }
    let embedded_file = document
        .get_object(embedded_file_id)
        .and_then(Object::as_stream)
        .with_context(|| format!("read {label} EmbeddedFile stream"))?;
    if embedded_file
        .dict
        .get(b"Type")
        .and_then(Object::as_name)
        .ok()
        != Some(b"EmbeddedFile")
    {
        return Err(anyhow!(
            "{label} embedded stream Type must be /EmbeddedFile"
        ));
    }
    let mime_type = embedded_file
        .dict
        .get(b"Subtype")
        .and_then(Object::as_name)
        .with_context(|| format!("{label} EmbeddedFile Subtype is missing"))?;
    if mime_type != format.mime_type.as_bytes() {
        return Err(anyhow!(
            "{label} EmbeddedFile MIME type does not match the attachment extension"
        ));
    }
    let content = embedded_file
        .decompressed_content_with_limit(MAX_PDF_ATTACHMENT_BYTES)
        .with_context(|| {
            format!(
                "decode {label} EmbeddedFile within the {} MiB limit",
                MAX_PDF_ATTACHMENT_BYTES / (1024 * 1024)
            )
        })?;
    if content.is_empty() {
        return Err(anyhow!("{label} EmbeddedFile must not be empty"));
    }
    validate_pdf_attachment_content(format, content.as_slice())?;
    let params = resolved_pdf_dictionary(
        document,
        embedded_file
            .dict
            .get(b"Params")
            .with_context(|| format!("{label} EmbeddedFile Params is missing"))?
            .clone(),
        format!("{label} EmbeddedFile Params").as_str(),
    )?;
    let declared_size = params
        .get(b"Size")
        .and_then(Object::as_i64)
        .ok()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| anyhow!("{label} EmbeddedFile Params/Size must be non-negative"))?;
    if declared_size != content.len() {
        return Err(anyhow!(
            "{label} EmbeddedFile Params/Size does not match the decoded attachment bytes"
        ));
    }

    let description = optional_bounded_pdf_text(
        filespec,
        b"Desc",
        format!("{label} Filespec").as_str(),
        MAX_PDF_ANNOTATION_CHARACTERS,
        true,
    )?;
    let bytes = content.len();
    let sha256 = hex::encode(Sha256::digest(content.as_slice()));
    let metadata = json!({
        "filename": filename.clone(),
        "portable_filename": portable_filename,
        "mime_type": format.mime_type,
        "bytes": bytes,
        "sha256": sha256.clone(),
        "description": description,
    });
    Ok(InspectedPdfFileAttachment {
        metadata,
        content,
        filename,
        format,
        sha256,
    })
}
