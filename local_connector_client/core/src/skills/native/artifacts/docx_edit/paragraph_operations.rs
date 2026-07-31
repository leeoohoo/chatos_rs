// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::File;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use zip::ZipArchive;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{input_file, optional_bool, read_zip_text, required_text};
use super::package_write::{block_result, docx_output_path, rewrite_docx};
use super::paragraph_edit::{
    delete_top_level_paragraph_at_index, delete_unique_top_level_paragraph,
    insert_blocks_at_top_level_paragraph_index, insert_blocks_at_unique_top_level_paragraph,
    move_top_level_paragraph_at_indices, move_unique_top_level_paragraph,
    replace_top_level_paragraph_at_index_with_blocks,
    replace_unique_top_level_paragraph_with_blocks,
};
use super::{render_blocks, required_docx_index, validate_xml_text, MAX_DOCX_BLOCKS};

pub(super) fn insert_docx_content_at_paragraph(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let anchor_text = required_docx_paragraph_text(arguments, "anchor_text")?;
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("position must be before or after"))?;
    if !matches!(position, "before" | "after") {
        return Err(anyhow!("position must be before or after"));
    }
    let (inserted_xml, stats) = render_blocks(arguments)?;

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, anchor_paragraph) = insert_blocks_at_unique_top_level_paragraph(
        existing_xml.as_str(),
        anchor_text,
        position,
        inserted_xml.as_str(),
    )?;
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = rewrite_docx(
        source.as_path(),
        target.as_path(),
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    let mut result = block_result("insert_at_paragraph", target_relative, bytes, &stats);
    result["source_path"] = Value::String(source_relative);
    result["anchor_paragraph"] = json!(anchor_paragraph);
    result["position"] = Value::String(position.to_string());
    Ok(result)
}

pub(super) fn insert_docx_content_at_paragraph_index(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let paragraph = required_docx_index(arguments, "paragraph", MAX_DOCX_BLOCKS)?;
    let expected_text = arguments
        .get("expected_text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("expected_text must be a string"))?;
    if expected_text.chars().count() > 4_096 {
        return Err(anyhow!(
            "expected_text exceeds the 4096 character safety limit"
        ));
    }
    validate_xml_text(expected_text, "expected_text")?;
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("position must be before or after"))?;
    if !matches!(position, "before" | "after") {
        return Err(anyhow!("position must be before or after"));
    }
    let (inserted_xml, stats) = render_blocks(arguments)?;

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, paragraphs_before) = insert_blocks_at_top_level_paragraph_index(
        existing_xml.as_str(),
        paragraph,
        expected_text,
        position,
        inserted_xml.as_str(),
    )?;
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = rewrite_docx(
        source.as_path(),
        target.as_path(),
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    let mut result = block_result("insert_at_paragraph_index", target_relative, bytes, &stats);
    result["source_path"] = Value::String(source_relative);
    result["paragraph"] = json!(paragraph);
    result["expected_characters"] = json!(expected_text.chars().count());
    result["position"] = Value::String(position.to_string());
    result["top_level_paragraphs_before"] = json!(paragraphs_before);
    result["top_level_paragraphs_after"] = json!(paragraphs_before + stats.paragraphs);
    Ok(result)
}

pub(super) fn delete_docx_paragraph(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let anchor_text = required_docx_paragraph_text(arguments, "anchor_text")?;
    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, anchor_paragraph) =
        delete_unique_top_level_paragraph(existing_xml.as_str(), anchor_text)?;
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = rewrite_docx(
        source.as_path(),
        target.as_path(),
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "delete_paragraph",
        "source_path": source_relative,
        "path": target_relative,
        "anchor_paragraph": anchor_paragraph,
        "bytes": bytes,
    }))
}

pub(super) fn delete_docx_paragraph_at_index(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let paragraph = required_docx_index(arguments, "paragraph", MAX_DOCX_BLOCKS)?;
    let expected_text = arguments
        .get("expected_text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("expected_text must be a string"))?;
    if expected_text.chars().count() > 4_096 {
        return Err(anyhow!(
            "expected_text exceeds the 4096 character safety limit"
        ));
    }
    validate_xml_text(expected_text, "expected_text")?;

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, paragraphs_before) =
        delete_top_level_paragraph_at_index(existing_xml.as_str(), paragraph, expected_text)?;
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = rewrite_docx(
        source.as_path(),
        target.as_path(),
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "delete_paragraph_at_index",
        "source_path": source_relative,
        "path": target_relative,
        "paragraph": paragraph,
        "expected_characters": expected_text.chars().count(),
        "top_level_paragraphs_before": paragraphs_before,
        "top_level_paragraphs_after": paragraphs_before - 1,
        "bytes": bytes,
    }))
}

pub(super) fn move_docx_paragraph(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let anchor_text = required_docx_paragraph_text(arguments, "anchor_text")?;
    let reference_text = required_docx_paragraph_text(arguments, "reference_text")?;
    if anchor_text == reference_text {
        return Err(anyhow!(
            "anchor_text and reference_text must select distinct paragraphs"
        ));
    }
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("position must be before or after"))?;
    if !matches!(position, "before" | "after") {
        return Err(anyhow!("position must be before or after"));
    }
    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, anchor_paragraph, reference_paragraph) = move_unique_top_level_paragraph(
        existing_xml.as_str(),
        anchor_text,
        reference_text,
        position,
    )?;
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = rewrite_docx(
        source.as_path(),
        target.as_path(),
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "move_paragraph",
        "source_path": source_relative,
        "path": target_relative,
        "anchor_paragraph": anchor_paragraph,
        "reference_paragraph": reference_paragraph,
        "position": position,
        "bytes": bytes,
    }))
}

pub(super) fn move_docx_paragraph_at_index(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let paragraph = required_docx_index(arguments, "paragraph", MAX_DOCX_BLOCKS)?;
    let expected_text = arguments
        .get("expected_text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("expected_text must be a string"))?;
    if expected_text.chars().count() > 4_096 {
        return Err(anyhow!(
            "expected_text exceeds the 4096 character safety limit"
        ));
    }
    validate_xml_text(expected_text, "expected_text")?;
    let reference_paragraph =
        required_docx_index(arguments, "reference_paragraph", MAX_DOCX_BLOCKS)?;
    let reference_expected_text = arguments
        .get("reference_expected_text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("reference_expected_text must be a string"))?;
    if reference_expected_text.chars().count() > 4_096 {
        return Err(anyhow!(
            "reference_expected_text exceeds the 4096 character safety limit"
        ));
    }
    validate_xml_text(reference_expected_text, "reference_expected_text")?;
    if paragraph == reference_paragraph {
        return Err(anyhow!(
            "paragraph and reference_paragraph must select distinct paragraphs"
        ));
    }
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("position must be before or after"))?;
    if !matches!(position, "before" | "after") {
        return Err(anyhow!("position must be before or after"));
    }

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, paragraphs, moved_paragraph) = move_top_level_paragraph_at_indices(
        existing_xml.as_str(),
        paragraph,
        expected_text,
        reference_paragraph,
        reference_expected_text,
        position,
    )?;
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = rewrite_docx(
        source.as_path(),
        target.as_path(),
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "move_paragraph_at_index",
        "source_path": source_relative,
        "path": target_relative,
        "paragraph": paragraph,
        "expected_characters": expected_text.chars().count(),
        "reference_paragraph": reference_paragraph,
        "reference_expected_characters": reference_expected_text.chars().count(),
        "moved_paragraph": moved_paragraph,
        "position": position,
        "top_level_paragraphs": paragraphs,
        "bytes": bytes,
    }))
}

pub(super) fn replace_docx_paragraph_with_content(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let anchor_text = required_docx_paragraph_text(arguments, "anchor_text")?;
    let (replacement_xml, stats) = render_blocks(arguments)?;
    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, anchor_paragraph) = replace_unique_top_level_paragraph_with_blocks(
        existing_xml.as_str(),
        anchor_text,
        replacement_xml.as_str(),
    )?;
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = rewrite_docx(
        source.as_path(),
        target.as_path(),
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    let mut result = block_result(
        "replace_paragraph_with_content",
        target_relative,
        bytes,
        &stats,
    );
    result["source_path"] = Value::String(source_relative);
    result["anchor_paragraph"] = json!(anchor_paragraph);
    Ok(result)
}

pub(super) fn replace_docx_paragraph_at_index_with_content(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let paragraph = required_docx_index(arguments, "paragraph", MAX_DOCX_BLOCKS)?;
    let expected_text = arguments
        .get("expected_text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("expected_text must be a string"))?;
    if expected_text.chars().count() > 4_096 {
        return Err(anyhow!(
            "expected_text exceeds the 4096 character safety limit"
        ));
    }
    validate_xml_text(expected_text, "expected_text")?;
    let (replacement_xml, stats) = render_blocks(arguments)?;

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, paragraphs_before) = replace_top_level_paragraph_at_index_with_blocks(
        existing_xml.as_str(),
        paragraph,
        expected_text,
        replacement_xml.as_str(),
    )?;
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = rewrite_docx(
        source.as_path(),
        target.as_path(),
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    let mut result = block_result(
        "replace_paragraph_at_index_with_content",
        target_relative,
        bytes,
        &stats,
    );
    result["source_path"] = Value::String(source_relative);
    result["paragraph"] = json!(paragraph);
    result["expected_characters"] = json!(expected_text.chars().count());
    result["top_level_paragraphs_before"] = json!(paragraphs_before);
    result["top_level_paragraphs_after"] = json!(paragraphs_before - 1 + stats.paragraphs);
    Ok(result)
}

fn required_docx_paragraph_text<'a>(arguments: &'a Value, field: &str) -> Result<&'a str> {
    let value = arguments
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("{field} must be a non-empty string"))?;
    if value.chars().count() > 4_096 {
        return Err(anyhow!("{field} exceeds the 4096 character safety limit"));
    }
    validate_xml_text(value, field)?;
    Ok(value)
}
