// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use anyhow::{anyhow, Context, Result};
use lopdf::Object;
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{file_size, input_file, optional_bool, required_text};
use super::{
    inherited_page_attribute, load_editable_pdf, materialized_page, merge_documents,
    optional_page_numbers, pdf_output_path, required_page_numbers, required_page_sequence,
    required_pdf_paths, save_pdf_document, validate_arrangeable_pdf, MAX_MERGED_INPUT_BYTES,
    MAX_PDF_INPUTS, MAX_PDF_PAGES,
};

pub(super) fn merge_pdfs(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let requested_paths = required_pdf_paths(arguments, "paths", 2, MAX_PDF_INPUTS)?;
    let mut source_paths = Vec::with_capacity(requested_paths.len());
    let mut source_relatives = Vec::with_capacity(requested_paths.len());
    let mut documents = Vec::with_capacity(requested_paths.len());
    let mut total_bytes = 0_u64;
    let mut total_pages = 0_usize;

    for requested in requested_paths {
        let (path, relative) = input_file(state, request, requested.as_str(), ".pdf")?;
        total_bytes = total_bytes.saturating_add(file_size(path.as_path())?);
        if total_bytes > MAX_MERGED_INPUT_BYTES {
            return Err(anyhow!(
                "PDF inputs exceed the 200 MiB combined safety limit"
            ));
        }
        let document = load_editable_pdf(path.as_path())?;
        total_pages = total_pages.saturating_add(document.get_pages().len());
        if total_pages > MAX_PDF_PAGES {
            return Err(anyhow!("PDF inputs exceed the 5000 page safety limit"));
        }
        source_paths.push(path);
        source_relatives.push(relative);
        documents.push(document);
    }

    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) =
        pdf_output_path(state, request, target_requested, source_paths.as_slice())?;
    let mut merged = merge_documents(documents)?;
    let bytes = save_pdf_document(
        &mut merged,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;

    Ok(json!({
        "created": true,
        "operation": "merge",
        "path": target_relative,
        "source_paths": source_relatives,
        "source_count": source_paths.len(),
        "pages": total_pages,
        "bytes": bytes,
    }))
}

pub(super) fn extract_pdf_pages(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let mut document = load_editable_pdf(source.as_path())?;
    let page_count = document.get_pages().len();
    let pages = required_page_numbers(arguments, "pages", page_count)?;
    let selected = pages.iter().copied().collect::<HashSet<_>>();
    let deleted = (1..=page_count as u32)
        .filter(|page| !selected.contains(page))
        .collect::<Vec<_>>();
    document.delete_pages(deleted.as_slice());

    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = pdf_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = save_pdf_document(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;

    Ok(json!({
        "created": true,
        "operation": "extract_pages",
        "source_path": source_relative,
        "path": target_relative,
        "pages": pages,
        "page_count": selected.len(),
        "bytes": bytes,
    }))
}

pub(super) fn arrange_pdf_pages(
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
            "PDF exceeds the {MAX_PDF_PAGES} page arrangement safety limit"
        ));
    }
    let pages = required_page_sequence(arguments, "pages", page_count)?;
    let unchanged = pages.len() == page_count
        && pages
            .iter()
            .enumerate()
            .all(|(index, page)| *page as usize == index + 1);
    if unchanged {
        return Err(anyhow!(
            "pages must change the page order or omit at least one source page"
        ));
    }
    validate_arrangeable_pdf(&document, &page_map)?;

    let pages_root_id = document
        .catalog()
        .context("read PDF catalog")?
        .get(b"Pages")
        .and_then(Object::as_reference)
        .context("read PDF catalog Pages reference")?;
    let root_count = document
        .get_object(pages_root_id)
        .and_then(Object::as_dict)
        .and_then(|dictionary| dictionary.get(b"Count"))
        .and_then(Object::as_i64)
        .context("read PDF pages root Count")?;
    if root_count != page_count as i64 {
        return Err(anyhow!(
            "PDF pages root Count does not match the traversed page count"
        ));
    }

    let mut arranged = Vec::with_capacity(pages.len());
    for page_number in &pages {
        let page_id = page_map
            .get(page_number)
            .copied()
            .ok_or_else(|| anyhow!("page {page_number} does not exist"))?;
        arranged.push((page_id, materialized_page(&document, page_id)?));
    }
    let arranged_ids = arranged
        .iter()
        .map(|(page_id, _)| *page_id)
        .collect::<Vec<_>>();
    for (page_id, page) in arranged {
        let mut dictionary = page.as_dict()?.clone();
        dictionary.set("Parent", pages_root_id);
        document
            .objects
            .insert(page_id, Object::Dictionary(dictionary));
    }
    let pages_root = document
        .get_object_mut(pages_root_id)
        .and_then(Object::as_dict_mut)
        .context("read mutable PDF pages root")?;
    pages_root.set(
        "Kids",
        arranged_ids
            .iter()
            .copied()
            .map(Object::Reference)
            .collect::<Vec<_>>(),
    );
    pages_root.set("Count", arranged_ids.len() as u32);
    pages_root.remove(b"Parent");

    let selected = pages.iter().copied().collect::<HashSet<_>>();
    let deleted_pages = (1..=page_count as u32)
        .filter(|page| !selected.contains(page))
        .collect::<Vec<_>>();
    let reordered = pages
        .iter()
        .enumerate()
        .any(|(index, page)| *page as usize != index + 1);
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = pdf_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = save_pdf_document(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;

    Ok(json!({
        "created": true,
        "operation": "arrange_pages",
        "source_path": source_relative,
        "path": target_relative,
        "pages": pages,
        "source_page_count": page_count,
        "page_count": arranged_ids.len(),
        "deleted_pages": deleted_pages,
        "reordered": reordered,
        "bytes": bytes,
    }))
}

pub(super) fn rotate_pdf_pages(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pdf")?;
    let mut document = load_editable_pdf(source.as_path())?;
    let page_map = document.get_pages();
    let page_count = page_map.len();
    if page_count == 0 {
        return Err(anyhow!("PDF contains no pages"));
    }
    let angle = arguments
        .get("angle")
        .and_then(Value::as_i64)
        .filter(|value| matches!(value, 90 | 180 | 270))
        .ok_or_else(|| anyhow!("angle must be 90, 180, or 270"))?;
    let pages = optional_page_numbers(arguments, "pages", page_count)?
        .unwrap_or_else(|| (1..=page_count as u32).collect());

    for page_number in &pages {
        let page_id = page_map
            .get(page_number)
            .copied()
            .ok_or_else(|| anyhow!("page {page_number} does not exist"))?;
        let inherited_rotation = inherited_page_attribute(&document, page_id, b"Rotate")
            .and_then(|value| value.as_i64().ok())
            .unwrap_or(0);
        let page = document
            .get_object_mut(page_id)
            .and_then(Object::as_dict_mut)
            .with_context(|| format!("read page {page_number} dictionary"))?;
        page.set("Rotate", (inherited_rotation + angle).rem_euclid(360));
    }

    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = pdf_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = save_pdf_document(
        &mut document,
        target.as_path(),
        optional_bool(arguments, "overwrite"),
    )?;

    Ok(json!({
        "created": true,
        "operation": "rotate_pages",
        "source_path": source_relative,
        "path": target_relative,
        "pages": pages,
        "angle": angle,
        "bytes": bytes,
    }))
}
