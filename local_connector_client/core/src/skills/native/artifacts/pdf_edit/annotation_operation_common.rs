// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use lopdf::{Dictionary, Document, Object, ObjectId};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{
    input_file, optional_bool, required_lowercase_sha256, required_text, sha256_file,
    MAX_ARTIFACT_BYTES,
};
use super::annotation_common::pdf_page_annotations;
use super::package_write::{
    load_editable_pdf, pdf_output_path, save_pdf_document, save_pdf_document_with_file_guards,
};
use super::{
    inspect_pdf_annotations, resolved_pdf_dictionary, PdfFileGuard, MAX_PDF_ANNOTATIONS,
    MAX_PDF_ANNOTATION_PREVIEW, MAX_PDF_PAGES,
};

pub(super) struct AnnotatablePdf {
    pub(super) source: PathBuf,
    pub(super) source_relative: String,
    pub(super) document: Document,
    pub(super) page_map: BTreeMap<u32, ObjectId>,
}

pub(super) struct GuardedAnnotationPdf {
    pub(super) source: PathBuf,
    pub(super) source_relative: String,
    pub(super) expected_source_sha256: String,
    pub(super) document: Document,
    pub(super) page_map: BTreeMap<u32, ObjectId>,
    pub(super) inspection: Value,
}

pub(super) struct SelectedAnnotation {
    pub(super) page_number: u32,
    pub(super) page_id: ObjectId,
    pub(super) annotation_index: usize,
    pub(super) annotations: Vec<Object>,
    pub(super) selected_id: Option<ObjectId>,
    pub(super) label: String,
    pub(super) dictionary: Dictionary,
    pub(super) subtype: String,
}

pub(super) fn load_annotatable_pdf(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<AnnotatablePdf> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let document = load_editable_pdf(source.as_path())?;
    let (page_map, inspection) = inspect_annotation_state(&document)?;
    ensure_annotation_capacity(&inspection)?;
    Ok(AnnotatablePdf {
        source,
        source_relative,
        document,
        page_map,
    })
}

pub(super) fn load_guarded_annotation_pdf(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<GuardedAnnotationPdf> {
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
            "encrypted PDFs cannot be edited without an explicit decryption workflow"
        ));
    }
    if document.get_pages().is_empty() {
        return Err(anyhow!("PDF contains no pages: {}", source.display()));
    }
    let (page_map, inspection) = inspect_annotation_state(&document)?;
    Ok(GuardedAnnotationPdf {
        source,
        source_relative,
        expected_source_sha256,
        document,
        page_map,
        inspection,
    })
}

pub(super) fn inspect_annotation_state(
    document: &Document,
) -> Result<(BTreeMap<u32, ObjectId>, Value)> {
    let page_map = document.get_pages();
    if page_map.len() > MAX_PDF_PAGES {
        return Err(anyhow!(
            "PDF exceeds the {MAX_PDF_PAGES} page annotation safety limit"
        ));
    }
    let inspection = inspect_pdf_annotations(document, &page_map, None)?;
    Ok((page_map, inspection))
}

pub(super) fn ensure_annotation_capacity(inspection: &Value) -> Result<()> {
    if inspection
        .get("count")
        .and_then(Value::as_u64)
        .is_some_and(|count| count >= MAX_PDF_ANNOTATIONS as u64)
    {
        return Err(anyhow!(
            "PDF already reaches the {MAX_PDF_ANNOTATIONS} annotation limit"
        ));
    }
    Ok(())
}

pub(super) fn select_annotation(
    arguments: &Value,
    document: &Document,
    page_map: &BTreeMap<u32, ObjectId>,
) -> Result<SelectedAnnotation> {
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
    let annotation_index = arguments
        .get("annotation_index")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| (1..=MAX_PDF_ANNOTATION_PREVIEW).contains(value))
        .ok_or_else(|| {
            anyhow!(
                "annotation_index must be an integer between 1 and {MAX_PDF_ANNOTATION_PREVIEW}"
            )
        })?;
    let annotations = pdf_page_annotations(
        document,
        page_id,
        format!("page {page_number} Annots").as_str(),
    )?;
    let selected_object = annotations
        .get(annotation_index - 1)
        .cloned()
        .ok_or_else(|| {
            anyhow!("page {page_number} annotation_index {annotation_index} does not exist")
        })?;
    let selected_id = selected_object.as_reference().ok();
    let label = format!("page {page_number} annotation {annotation_index}");
    let dictionary = resolved_pdf_dictionary(document, selected_object, label.as_str())?;
    let subtype = dictionary
        .get(b"Subtype")
        .and_then(Object::as_name)
        .with_context(|| format!("{label} is missing a valid Subtype"))?;
    let subtype = String::from_utf8_lossy(subtype).to_string();
    Ok(SelectedAnnotation {
        page_number,
        page_id,
        annotation_index,
        annotations,
        selected_id,
        label,
        dictionary,
        subtype,
    })
}

pub(super) fn annotation_relation_type(
    annotation: &Dictionary,
    label: &str,
) -> Result<&'static str> {
    match annotation.get(b"IRT") {
        Err(_) => Ok("root"),
        Ok(_) => match annotation.get(b"RT") {
            Err(_) => Ok("reply"),
            Ok(value) => match value
                .as_name()
                .with_context(|| format!("{label} RT must be a name"))?
            {
                b"R" => Ok("reply"),
                b"Group" => Ok("group"),
                _ => Err(anyhow!("{label} RT must be /R or /Group")),
            },
        },
    }
}

pub(super) fn append_pdf_annotation(
    document: &mut Document,
    page_number: u32,
    page_id: ObjectId,
    annotation: Dictionary,
) -> Result<(ObjectId, usize)> {
    let annotation_id = document.add_object(annotation);
    let mut annotations = pdf_page_annotations(
        document,
        page_id,
        format!("page {page_number} Annots").as_str(),
    )?;
    if annotations.len() >= MAX_PDF_ANNOTATIONS {
        return Err(anyhow!(
            "page {page_number} already reaches the {MAX_PDF_ANNOTATIONS} annotation limit"
        ));
    }
    annotations.push(Object::Reference(annotation_id));
    let annotation_index = annotations.len();
    document
        .get_object_mut(page_id)
        .and_then(Object::as_dict_mut)
        .with_context(|| format!("read page {page_number} dictionary"))?
        .set("Annots", annotations);
    Ok((annotation_id, annotation_index))
}

pub(super) fn save_annotation_output(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    source: &Path,
    document: &mut Document,
) -> Result<(String, u64)> {
    let target_requested = required_text(arguments, "target_path")?;
    let source_path = source.to_path_buf();
    let (target, target_relative) = pdf_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source_path),
    )?;
    let bytes = save_pdf_document(
        document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok((target_relative, bytes))
}

pub(super) fn save_guarded_annotation_output(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    source: &Path,
    expected_source_sha256: &str,
    changed_message: &str,
    document: &mut Document,
) -> Result<(String, u64)> {
    let target_requested = required_text(arguments, "target_path")?;
    let source_path = source.to_path_buf();
    let (target, target_relative) = pdf_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source_path),
    )?;
    let guards = [PdfFileGuard {
        path: source,
        expected_sha256: expected_source_sha256,
        changed_message,
        require_regular_non_symlink: false,
    }];
    let bytes = save_pdf_document_with_file_guards(
        document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
        guards.as_slice(),
    )?;
    Ok((target_relative, bytes))
}
