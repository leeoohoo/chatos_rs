// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::MAX_XML_BYTES;
use super::limits::{
    MAX_PPTX_SLIDES, MAX_PPTX_TABLES_PER_SLIDE, MAX_PPTX_TABLE_CELL_TEXT_CHARS,
    MAX_PPTX_TABLE_COLUMNS, MAX_PPTX_TABLE_ROWS,
};
use super::package_io::{ensure_distinct_pptx_paths, rewrite_pptx_package};
use super::table_edit::{apply_pptx_xml_edits, validate_updated_pptx_table_cells};
use super::table_scan::scan_pptx_tables;
use super::table_selection::{
    ensure_pptx_table_cell_xml_sha256, pptx_table_cell_xml_sha256, required_pptx_index,
    required_pptx_sha256, selected_pptx_table, simple_pptx_table_cell_xml_sha256,
};
use super::table_structure::{simple_pptx_table_columns, simple_pptx_table_rows};
use super::text_edit::pptx_text_opening_for_value;
use super::text_validation::validate_slide_text;
use super::{
    escape_xml, input_file, optional_bool, require_extension, required_text, safe_workspace_path,
};

pub(super) fn inspect_pptx_table(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let slide_number = required_pptx_index(arguments, "slide_number", MAX_PPTX_SLIDES)?;
    let table_number = required_pptx_index(arguments, "table_number", MAX_PPTX_TABLES_PER_SLIDE)?;
    let (slide_path, slide_xml, table) =
        selected_pptx_table(source.as_path(), slide_number, table_number)?;
    let (eligible_for_row_editing, row_editing_unsupported_reason) = match table.simple.as_ref() {
        Some(simple) => match simple_pptx_table_rows(slide_xml.as_str(), simple) {
            Ok(_) => (true, None),
            Err(error) => (false, Some(error.to_string())),
        },
        None => (
            false,
            Some(
                table
                    .unsupported_reason
                    .clone()
                    .unwrap_or_else(|| "unsupported DrawingML table structure".to_string()),
            ),
        ),
    };
    let (eligible_for_column_editing, column_editing_unsupported_reason) =
        match table.simple.as_ref() {
            Some(simple) => match simple_pptx_table_columns(slide_xml.as_str(), simple) {
                Ok(_) => (true, None),
                Err(error) => (false, Some(error.to_string())),
            },
            None => (
                false,
                Some(
                    table
                        .unsupported_reason
                        .clone()
                        .unwrap_or_else(|| "unsupported DrawingML table structure".to_string()),
                ),
            ),
        };
    let cell_xml_sha256 = table
        .simple
        .as_ref()
        .map(|simple| simple_pptx_table_cell_xml_sha256(slide_xml.as_str(), simple));
    Ok(json!({
        "path": source_relative,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "table_number": table_number,
        "rows": table.rows,
        "columns": table.columns,
        "cells": table.cells,
        "cell_text": table.cell_text,
        "cell_text_truncated": table.cell_text_truncated,
        "cell_xml_sha256": cell_xml_sha256,
        "eligible_for_cell_replacement": table.simple.is_some(),
        "eligible_for_cell_format_copy": table.simple.is_some(),
        "unsupported_reason": table.unsupported_reason,
        "eligible_for_row_editing": eligible_for_row_editing,
        "row_editing_unsupported_reason": row_editing_unsupported_reason,
        "eligible_for_column_editing": eligible_for_column_editing,
        "column_editing_unsupported_reason": column_editing_unsupported_reason,
    }))
}

pub(super) fn copy_pptx_table_cell_format(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let slide_number = required_pptx_index(arguments, "slide_number", MAX_PPTX_SLIDES)?;
    let table_number = required_pptx_index(arguments, "table_number", MAX_PPTX_TABLES_PER_SLIDE)?;
    let row = required_pptx_index(arguments, "row", MAX_PPTX_TABLE_ROWS)?;
    let column = required_pptx_index(arguments, "column", MAX_PPTX_TABLE_COLUMNS)?;
    let reference_row = required_pptx_index(arguments, "reference_row", MAX_PPTX_TABLE_ROWS)?;
    let reference_column =
        required_pptx_index(arguments, "reference_column", MAX_PPTX_TABLE_COLUMNS)?;
    if row == reference_row && column == reference_column {
        return Err(anyhow!(
            "PPTX table format copy must select different target and reference cells"
        ));
    }
    let expected_text = arguments
        .get("expected_text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("expected_text must be a string"))?;
    let reference_expected_text = arguments
        .get("reference_expected_text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("reference_expected_text must be a string"))?;
    validate_slide_text(
        expected_text,
        "expected_text",
        MAX_PPTX_TABLE_CELL_TEXT_CHARS,
    )?;
    validate_slide_text(
        reference_expected_text,
        "reference_expected_text",
        MAX_PPTX_TABLE_CELL_TEXT_CHARS,
    )?;
    let expected_cell_xml_sha256 = required_pptx_sha256(arguments, "expected_cell_xml_sha256")?;
    let reference_expected_cell_xml_sha256 =
        required_pptx_sha256(arguments, "reference_expected_cell_xml_sha256")?;

    let (slide_path, slide_xml, table) =
        selected_pptx_table(source.as_path(), slide_number, table_number)?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "selected PPTX table is not eligible for simple cell format copying: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    for (label, selected_row, selected_column) in [
        ("target", row, column),
        ("reference", reference_row, reference_column),
    ] {
        if selected_row > simple.rows || selected_column > simple.columns {
            return Err(anyhow!(
                "{label} table cell ({selected_row}, {selected_column}) is out-of-range for a {rows}x{columns} PPTX table",
                rows = simple.rows,
                columns = simple.columns
            ));
        }
    }
    let selected_cell = |selected_row: usize, selected_column: usize| {
        simple
            .cells
            .iter()
            .find(|cell| cell.row == selected_row && cell.column == selected_column)
            .ok_or_else(|| {
                anyhow!("simple PPTX table is missing cell ({selected_row}, {selected_column})")
            })
    };
    let target_cell = selected_cell(row, column)?;
    let reference_cell = selected_cell(reference_row, reference_column)?;
    if target_cell.decoded != expected_text {
        return Err(anyhow!(
            "target PPTX table cell text does not match expected_text"
        ));
    }
    if reference_cell.decoded != reference_expected_text {
        return Err(anyhow!(
            "reference PPTX table cell text does not match reference_expected_text"
        ));
    }
    ensure_pptx_table_cell_xml_sha256(
        slide_xml.as_str(),
        target_cell,
        expected_cell_xml_sha256.as_str(),
        "target",
    )?;
    ensure_pptx_table_cell_xml_sha256(
        slide_xml.as_str(),
        reference_cell,
        reference_expected_cell_xml_sha256.as_str(),
        "reference",
    )?;

    let target_cell_xml = &slide_xml[target_cell.range.start..target_cell.range.end];
    let mut replacement_cell_xml =
        slide_xml[reference_cell.range.start..reference_cell.range.end].to_string();
    let reference_text_start = reference_cell.text_start - reference_cell.range.start;
    let reference_text_open_end = reference_cell.text_open_end - reference_cell.range.start;
    let reference_text_close_end = reference_cell.text_close_end - reference_cell.range.start;
    let reference_text_opening =
        &replacement_cell_xml[reference_text_start..reference_text_open_end];
    let target_text_opening = pptx_text_opening_for_value(reference_text_opening, expected_text)?;
    replacement_cell_xml.replace_range(
        reference_text_start..reference_text_close_end,
        format!("{target_text_opening}{}</a:t>", escape_xml(expected_text)).as_str(),
    );
    if replacement_cell_xml == target_cell_xml {
        return Err(anyhow!(
            "target PPTX table cell already has the reference cell formatting"
        ));
    }
    let updated_xml = apply_pptx_xml_edits(
        slide_xml.as_str(),
        vec![(
            target_cell.range.start,
            target_cell.range.end,
            replacement_cell_xml,
        )],
    )?;
    validate_updated_pptx_table_cells(
        updated_xml.as_str(),
        table_number,
        simple.rows,
        simple.columns,
    )?;
    let updated_tables = scan_pptx_tables(updated_xml.as_str())?;
    let updated_simple = updated_tables
        .get(table_number - 1)
        .and_then(|table| table.simple.as_ref())
        .ok_or_else(|| anyhow!("validated updated PPTX table is not simple"))?;
    let updated_target = updated_simple
        .cells
        .iter()
        .find(|cell| cell.row == row && cell.column == column)
        .ok_or_else(|| anyhow!("validated updated PPTX table is missing the target cell"))?;
    if updated_target.decoded != expected_text {
        return Err(anyhow!(
            "PPTX table format copy unexpectedly changed the target cell text"
        ));
    }
    let updated_cell_xml_sha256 = pptx_table_cell_xml_sha256(updated_xml.as_str(), updated_target);
    let replacements = BTreeMap::from([(slide_path.clone(), updated_xml.into_bytes())]);
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "operation": "copy_table_cell_format",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "table_number": table_number,
        "row": row,
        "column": column,
        "reference_row": reference_row,
        "reference_column": reference_column,
        "target_text_preserved": true,
        "reference_text_not_copied": true,
        "cell_xml_sha256": updated_cell_xml_sha256,
        "bytes": bytes,
    }))
}

pub(super) fn replace_pptx_table_cell_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let slide_number = required_pptx_index(arguments, "slide_number", MAX_PPTX_SLIDES)?;
    let table_number = required_pptx_index(arguments, "table_number", MAX_PPTX_TABLES_PER_SLIDE)?;
    let row = required_pptx_index(arguments, "row", MAX_PPTX_TABLE_ROWS)?;
    let column = required_pptx_index(arguments, "column", MAX_PPTX_TABLE_COLUMNS)?;
    let expected_text = arguments
        .get("expected_text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("expected_text must be a string"))?;
    let replacement = arguments
        .get("replacement")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("replacement must be a string"))?;
    validate_slide_text(
        expected_text,
        "expected_text",
        MAX_PPTX_TABLE_CELL_TEXT_CHARS,
    )?;
    validate_slide_text(replacement, "replacement", MAX_PPTX_TABLE_CELL_TEXT_CHARS)?;
    if expected_text == replacement {
        return Err(anyhow!(
            "PPTX table cell replacement must change the selected cell text"
        ));
    }

    let (slide_path, slide_xml, table) =
        selected_pptx_table(source.as_path(), slide_number, table_number)?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "selected PPTX table is not eligible for simple cell replacement: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if row > simple.rows || column > simple.columns {
        return Err(anyhow!(
            "table cell ({row}, {column}) is out-of-range for a {rows}x{columns} PPTX table",
            rows = simple.rows,
            columns = simple.columns
        ));
    }
    let cell = simple
        .cells
        .iter()
        .find(|cell| cell.row == row && cell.column == column)
        .ok_or_else(|| anyhow!("simple PPTX table is missing cell ({row}, {column})"))?;
    if cell.decoded != expected_text {
        return Err(anyhow!(
            "selected PPTX table cell text does not match expected_text"
        ));
    }
    let opening = &slide_xml[cell.text_start..cell.text_open_end];
    let opening = pptx_text_opening_for_value(opening, replacement)?;
    let mut updated_xml = slide_xml.clone();
    updated_xml.replace_range(
        cell.text_start..cell.text_close_end,
        format!("{opening}{}</a:t>", escape_xml(replacement)).as_str(),
    );
    if updated_xml.len() > MAX_XML_BYTES {
        return Err(anyhow!(
            "updated PPTX slide XML exceeds the local XML size limit"
        ));
    }
    let replacements = BTreeMap::from([(slide_path.clone(), updated_xml.into_bytes())]);
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        Vec::new(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "operation": "replace_table_cell_text",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "table_number": table_number,
        "row": row,
        "column": column,
        "previous_characters": expected_text.chars().count(),
        "replacement_characters": replacement.chars().count(),
        "bytes": bytes,
    }))
}
