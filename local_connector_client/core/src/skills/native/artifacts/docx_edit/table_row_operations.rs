// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::File;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use zip::ZipArchive;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{input_file, optional_bool, read_zip_text, required_text};
use super::package_write::{docx_output_path, rewrite_docx};
use super::table_row_delete::delete_simple_docx_table_row;
use super::table_row_insert::insert_simple_docx_table_row;
use super::table_row_move::move_simple_docx_table_row;
use super::{required_docx_cell_texts, required_docx_index, MAX_DOCX_BLOCKS};

pub(super) fn delete_docx_table_row(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let table_index = required_docx_index(arguments, "table", MAX_DOCX_BLOCKS)?;
    let row_index = required_docx_index(arguments, "row", MAX_DOCX_BLOCKS)?;
    let expected_cells = required_docx_cell_texts(arguments, "expected_cells")?;

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let document_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, row_count_before) = delete_simple_docx_table_row(
        document_xml.as_str(),
        table_index,
        row_index,
        expected_cells.as_slice(),
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
        "operation": "delete_table_row",
        "source_path": source_relative,
        "path": target_relative,
        "table": table_index,
        "row": row_index,
        "removed_cells": expected_cells.len(),
        "rows_before": row_count_before,
        "rows_after": row_count_before - 1,
        "bytes": bytes,
    }))
}

pub(super) fn insert_docx_table_row(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let table_index = required_docx_index(arguments, "table", MAX_DOCX_BLOCKS)?;
    let reference_row = required_docx_index(arguments, "reference_row", MAX_DOCX_BLOCKS)?;
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("position must be before or after"))?;
    if !matches!(position, "before" | "after") {
        return Err(anyhow!("position must be before or after"));
    }
    let expected_cells = required_docx_cell_texts(arguments, "expected_cells")?;
    let cells = required_docx_cell_texts(arguments, "cells")?;
    if expected_cells.len() != cells.len() {
        return Err(anyhow!(
            "cells must contain exactly the same number of items as expected_cells"
        ));
    }

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let document_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, rows_before, inserted_row, stripped_identity_attributes) =
        insert_simple_docx_table_row(
            document_xml.as_str(),
            table_index,
            reference_row,
            position,
            expected_cells.as_slice(),
            cells.as_slice(),
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
        "operation": "insert_table_row",
        "source_path": source_relative,
        "path": target_relative,
        "table": table_index,
        "reference_row": reference_row,
        "inserted_row": inserted_row,
        "position": position,
        "inserted_cells": cells.len(),
        "rows_before": rows_before,
        "rows_after": rows_before + 1,
        "formatting_cloned": true,
        "stripped_identity_attributes": stripped_identity_attributes,
        "bytes": bytes,
    }))
}

pub(super) fn move_docx_table_row(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let table_index = required_docx_index(arguments, "table", MAX_DOCX_BLOCKS)?;
    let row_index = required_docx_index(arguments, "row", MAX_DOCX_BLOCKS)?;
    let reference_row = required_docx_index(arguments, "reference_row", MAX_DOCX_BLOCKS)?;
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("position must be before or after"))?;
    if !matches!(position, "before" | "after") {
        return Err(anyhow!("position must be before or after"));
    }
    let expected_cells = required_docx_cell_texts(arguments, "expected_cells")?;
    let reference_expected_cells = required_docx_cell_texts(arguments, "reference_expected_cells")?;

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let document_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (updated_xml, rows, moved_row) = move_simple_docx_table_row(
        document_xml.as_str(),
        table_index,
        row_index,
        expected_cells.as_slice(),
        reference_row,
        reference_expected_cells.as_slice(),
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
        "operation": "move_table_row",
        "source_path": source_relative,
        "path": target_relative,
        "table": table_index,
        "row": row_index,
        "reference_row": reference_row,
        "moved_row": moved_row,
        "position": position,
        "moved_cells": expected_cells.len(),
        "rows_before": rows,
        "rows_after": rows,
        "formatting_preserved": true,
        "bytes": bytes,
    }))
}
