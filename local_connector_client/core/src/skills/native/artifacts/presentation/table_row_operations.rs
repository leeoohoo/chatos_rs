// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::limits::{
    MAX_PPTX_SLIDES, MAX_PPTX_TABLES_PER_SLIDE, MAX_PPTX_TABLE_CELLS, MAX_PPTX_TABLE_ROWS,
    MAX_PPTX_TABLE_TOTAL_TEXT_CHARS, SLIDE_HEIGHT,
};
use super::package_io::{ensure_distinct_pptx_paths, rewrite_pptx_package};
use super::table_edit::{
    apply_pptx_xml_edits, ensure_changed_pptx_table_move, move_pptx_xml_element_edit,
    moved_pptx_table_index, validate_updated_pptx_table_rows,
};
use super::table_selection::{
    ensure_expected_pptx_table_row, required_pptx_index, required_pptx_table_row_cells,
    selected_pptx_table,
};
use super::table_structure::{
    canonical_pptx_table_row_opening, clone_pptx_table_row_with_text, pptx_table_row_with_height,
    simple_pptx_table_rows,
};
use super::{input_file, optional_bool, require_extension, required_text, safe_workspace_path};

pub(super) fn delete_pptx_table_row(
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
    let (slide_path, slide_xml, table) =
        selected_pptx_table(source.as_path(), slide_number, table_number)?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "selected PPTX table is not eligible for simple row deletion: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if simple.rows == 1 {
        return Err(anyhow!("cannot delete the only row from a PPTX table"));
    }
    if row > simple.rows {
        return Err(anyhow!(
            "table row {row} is out-of-range for a PPTX table with {} rows",
            simple.rows
        ));
    }
    let expected_cells =
        required_pptx_table_row_cells(arguments, "expected_cells", simple.columns)?;
    ensure_expected_pptx_table_row(simple, row, expected_cells.as_slice())?;
    let rows = simple_pptx_table_rows(slide_xml.as_str(), simple).map_err(|error| {
        anyhow!("selected PPTX table is not eligible for simple row deletion: {error}")
    })?;
    let deleted = rows[row - 1];
    let recipient_index = if row < rows.len() { row } else { row - 2 };
    let recipient = rows[recipient_index];
    let recipient_height = recipient
        .height
        .checked_add(deleted.height)
        .filter(|height| *height <= SLIDE_HEIGHT)
        .ok_or_else(|| anyhow!("PPTX table row height overflow during deletion"))?;
    let updated_xml = apply_pptx_xml_edits(
        slide_xml.as_str(),
        vec![
            (
                recipient.range.start,
                recipient.range.open_end,
                canonical_pptx_table_row_opening(recipient_height),
            ),
            (deleted.range.start, deleted.range.end, String::new()),
        ],
    )?;
    validate_updated_pptx_table_rows(
        updated_xml.as_str(),
        table_number,
        simple.rows - 1,
        simple.columns,
    )?;
    let output_recipient_row = if recipient_index + 1 > row {
        recipient_index
    } else {
        recipient_index + 1
    };
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
        "operation": "delete_table_row",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "table_number": table_number,
        "deleted_row": row,
        "previous_rows": simple.rows,
        "rows": simple.rows - 1,
        "columns": simple.columns,
        "height_transferred_to_row": output_recipient_row,
        "table_frame_height_unchanged": true,
        "bytes": bytes,
    }))
}

pub(super) fn insert_pptx_table_row(
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
    let reference_row = required_pptx_index(arguments, "reference_row", MAX_PPTX_TABLE_ROWS)?;
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("position must be before or after"))?;
    if !matches!(position, "before" | "after") {
        return Err(anyhow!("position must be before or after"));
    }
    let (slide_path, slide_xml, table) =
        selected_pptx_table(source.as_path(), slide_number, table_number)?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "selected PPTX table is not eligible for simple row insertion: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if simple.rows >= MAX_PPTX_TABLE_ROWS
        || simple.cells.len().saturating_add(simple.columns) > MAX_PPTX_TABLE_CELLS
    {
        return Err(anyhow!(
            "PPTX table cannot accept another row within the local safety limits"
        ));
    }
    if reference_row > simple.rows {
        return Err(anyhow!(
            "reference_row {reference_row} is out-of-range for a PPTX table with {} rows",
            simple.rows
        ));
    }
    let expected_cells =
        required_pptx_table_row_cells(arguments, "expected_cells", simple.columns)?;
    ensure_expected_pptx_table_row(simple, reference_row, expected_cells.as_slice())?;
    let cells = required_pptx_table_row_cells(arguments, "cells", simple.columns)?;
    let existing_text_chars = simple
        .cells
        .iter()
        .map(|cell| cell.decoded.chars().count())
        .sum::<usize>();
    let added_text_chars = cells.iter().map(|cell| cell.chars().count()).sum::<usize>();
    if existing_text_chars.saturating_add(added_text_chars) > MAX_PPTX_TABLE_TOTAL_TEXT_CHARS {
        return Err(anyhow!(
            "inserted PPTX table row would exceed the {MAX_PPTX_TABLE_TOTAL_TEXT_CHARS} character safety limit"
        ));
    }
    let rows = simple_pptx_table_rows(slide_xml.as_str(), simple).map_err(|error| {
        anyhow!("selected PPTX table is not eligible for simple row insertion: {error}")
    })?;
    let reference = rows[reference_row - 1];
    if reference.height < 2 {
        return Err(anyhow!(
            "reference PPTX table row is too short to split safely"
        ));
    }
    let inserted_height = reference.height / 2;
    let retained_height = reference.height - inserted_height;
    let reference_cells = simple
        .cells
        .iter()
        .filter(|cell| cell.row == reference_row)
        .cloned()
        .collect::<Vec<_>>();
    let retained_row_xml =
        pptx_table_row_with_height(slide_xml.as_str(), reference, retained_height)?;
    let inserted_row_xml = clone_pptx_table_row_with_text(
        slide_xml.as_str(),
        reference,
        reference_cells.as_slice(),
        cells.as_slice(),
        inserted_height,
    )?;
    let replacement = if position == "before" {
        format!("{inserted_row_xml}{retained_row_xml}")
    } else {
        format!("{retained_row_xml}{inserted_row_xml}")
    };
    let updated_xml = apply_pptx_xml_edits(
        slide_xml.as_str(),
        vec![(reference.range.start, reference.range.end, replacement)],
    )?;
    validate_updated_pptx_table_rows(
        updated_xml.as_str(),
        table_number,
        simple.rows + 1,
        simple.columns,
    )?;
    let inserted_row = if position == "before" {
        reference_row
    } else {
        reference_row + 1
    };
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
        "operation": "insert_table_row",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "table_number": table_number,
        "reference_row": reference_row,
        "position": position,
        "inserted_row": inserted_row,
        "previous_rows": simple.rows,
        "rows": simple.rows + 1,
        "columns": simple.columns,
        "format_cloned_from_reference_row": true,
        "table_frame_height_unchanged": true,
        "bytes": bytes,
    }))
}

pub(super) fn move_pptx_table_row(
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
    let reference_row = required_pptx_index(arguments, "reference_row", MAX_PPTX_TABLE_ROWS)?;
    let position = arguments
        .get("position")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("position must be before or after"))?;
    if !matches!(position, "before" | "after") {
        return Err(anyhow!("position must be before or after"));
    }
    let (slide_path, slide_xml, table) =
        selected_pptx_table(source.as_path(), slide_number, table_number)?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "selected PPTX table is not eligible for simple row movement: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if row > simple.rows {
        return Err(anyhow!(
            "table row {row} is out-of-range for a PPTX table with {} rows",
            simple.rows
        ));
    }
    if reference_row > simple.rows {
        return Err(anyhow!(
            "reference_row {reference_row} is out-of-range for a PPTX table with {} rows",
            simple.rows
        ));
    }
    ensure_changed_pptx_table_move(row, reference_row, position, "row")?;
    let expected_cells =
        required_pptx_table_row_cells(arguments, "expected_cells", simple.columns)?;
    ensure_expected_pptx_table_row(simple, row, expected_cells.as_slice())?;
    let reference_expected_cells =
        required_pptx_table_row_cells(arguments, "reference_expected_cells", simple.columns)?;
    ensure_expected_pptx_table_row(simple, reference_row, reference_expected_cells.as_slice())
        .map_err(|_| {
            anyhow!("selected PPTX reference row does not match reference_expected_cells")
        })?;
    let rows = simple_pptx_table_rows(slide_xml.as_str(), simple).map_err(|error| {
        anyhow!("selected PPTX table is not eligible for simple row movement: {error}")
    })?;
    let source_row = rows[row - 1];
    let reference = rows[reference_row - 1];
    let edit = move_pptx_xml_element_edit(
        slide_xml.as_str(),
        source_row.range,
        reference.range,
        position,
    )?;
    let updated_xml = apply_pptx_xml_edits(slide_xml.as_str(), vec![edit])?;
    validate_updated_pptx_table_rows(
        updated_xml.as_str(),
        table_number,
        simple.rows,
        simple.columns,
    )?;
    let moved_row = moved_pptx_table_index(row, reference_row, position)?;
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
        "operation": "move_table_row",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "table_number": table_number,
        "row": row,
        "reference_row": reference_row,
        "position": position,
        "moved_row": moved_row,
        "rows": simple.rows,
        "columns": simple.columns,
        "row_xml_and_formatting_preserved": true,
        "table_frame_height_unchanged": true,
        "bytes": bytes,
    }))
}
