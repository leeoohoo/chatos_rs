// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::limits::{
    MAX_PPTX_SLIDES, MAX_PPTX_TABLES_PER_SLIDE, MAX_PPTX_TABLE_CELLS, MAX_PPTX_TABLE_COLUMNS,
    MAX_PPTX_TABLE_TOTAL_TEXT_CHARS, SLIDE_WIDTH,
};
use super::package_io::{ensure_distinct_pptx_paths, rewrite_pptx_package};
use super::table_edit::{
    apply_pptx_xml_edits, ensure_changed_pptx_table_move, move_pptx_xml_element_edit,
    moved_pptx_table_index, validate_updated_pptx_table_columns,
};
use super::table_selection::{
    ensure_expected_pptx_table_column, required_pptx_index, required_pptx_table_column_cells,
    selected_pptx_table,
};
use super::table_structure::{
    canonical_pptx_table_column_opening, clone_pptx_table_cell_with_text, simple_pptx_table_columns,
};
use super::{input_file, optional_bool, require_extension, required_text, safe_workspace_path};

pub(super) fn delete_pptx_table_column(
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
    let column = required_pptx_index(arguments, "column", MAX_PPTX_TABLE_COLUMNS)?;
    let (slide_path, slide_xml, table) =
        selected_pptx_table(source.as_path(), slide_number, table_number)?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "selected PPTX table is not eligible for simple column deletion: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if simple.columns == 1 {
        return Err(anyhow!("cannot delete the only column from a PPTX table"));
    }
    if column > simple.columns {
        return Err(anyhow!(
            "table column {column} is out-of-range for a PPTX table with {} columns",
            simple.columns
        ));
    }
    let expected_cells =
        required_pptx_table_column_cells(arguments, "expected_cells", simple.rows)?;
    ensure_expected_pptx_table_column(simple, column, expected_cells.as_slice())?;
    let columns = simple_pptx_table_columns(slide_xml.as_str(), simple).map_err(|error| {
        anyhow!("selected PPTX table is not eligible for simple column deletion: {error}")
    })?;
    let deleted = columns[column - 1];
    let recipient_index = if column < columns.len() {
        column
    } else {
        column - 2
    };
    let recipient = columns[recipient_index];
    let recipient_width = recipient
        .width
        .checked_add(deleted.width)
        .filter(|width| *width <= SLIDE_WIDTH)
        .ok_or_else(|| anyhow!("PPTX table column width overflow during deletion"))?;
    let mut edits = vec![
        (
            recipient.range.start,
            recipient.range.open_end,
            canonical_pptx_table_column_opening(recipient_width),
        ),
        (deleted.range.start, deleted.range.end, String::new()),
    ];
    edits.extend(
        simple
            .cells
            .iter()
            .filter(|cell| cell.column == column)
            .map(|cell| (cell.range.start, cell.range.end, String::new())),
    );
    let updated_xml = apply_pptx_xml_edits(slide_xml.as_str(), edits)?;
    validate_updated_pptx_table_columns(
        updated_xml.as_str(),
        table_number,
        simple.rows,
        simple.columns - 1,
    )?;
    let output_recipient_column = if recipient_index + 1 > column {
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
        "operation": "delete_table_column",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "table_number": table_number,
        "deleted_column": column,
        "previous_columns": simple.columns,
        "rows": simple.rows,
        "columns": simple.columns - 1,
        "width_transferred_to_column": output_recipient_column,
        "table_frame_width_unchanged": true,
        "bytes": bytes,
    }))
}

pub(super) fn insert_pptx_table_column(
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
    let reference_column =
        required_pptx_index(arguments, "reference_column", MAX_PPTX_TABLE_COLUMNS)?;
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
            "selected PPTX table is not eligible for simple column insertion: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if simple.columns >= MAX_PPTX_TABLE_COLUMNS
        || simple.cells.len().saturating_add(simple.rows) > MAX_PPTX_TABLE_CELLS
    {
        return Err(anyhow!(
            "PPTX table cannot accept another column within the local safety limits"
        ));
    }
    if reference_column > simple.columns {
        return Err(anyhow!(
            "reference_column {reference_column} is out-of-range for a PPTX table with {} columns",
            simple.columns
        ));
    }
    let expected_cells =
        required_pptx_table_column_cells(arguments, "expected_cells", simple.rows)?;
    ensure_expected_pptx_table_column(simple, reference_column, expected_cells.as_slice())?;
    let cells = required_pptx_table_column_cells(arguments, "cells", simple.rows)?;
    let existing_text_chars = simple
        .cells
        .iter()
        .map(|cell| cell.decoded.chars().count())
        .sum::<usize>();
    let added_text_chars = cells.iter().map(|cell| cell.chars().count()).sum::<usize>();
    if existing_text_chars.saturating_add(added_text_chars) > MAX_PPTX_TABLE_TOTAL_TEXT_CHARS {
        return Err(anyhow!(
            "inserted PPTX table column would exceed the {MAX_PPTX_TABLE_TOTAL_TEXT_CHARS} character safety limit"
        ));
    }
    let columns = simple_pptx_table_columns(slide_xml.as_str(), simple).map_err(|error| {
        anyhow!("selected PPTX table is not eligible for simple column insertion: {error}")
    })?;
    let reference = columns[reference_column - 1];
    if reference.width < 2 {
        return Err(anyhow!(
            "reference PPTX table column is too narrow to split safely"
        ));
    }
    let inserted_width = reference.width / 2;
    let retained_width = reference.width - inserted_width;
    let retained_column_xml = canonical_pptx_table_column_opening(retained_width);
    let inserted_column_xml = canonical_pptx_table_column_opening(inserted_width);
    let column_replacement = if position == "before" {
        format!("{inserted_column_xml}{retained_column_xml}")
    } else {
        format!("{retained_column_xml}{inserted_column_xml}")
    };
    let reference_cells = simple
        .cells
        .iter()
        .filter(|cell| cell.column == reference_column)
        .collect::<Vec<_>>();
    if reference_cells.len() != simple.rows {
        return Err(anyhow!(
            "reference PPTX table column does not contain one cell per row"
        ));
    }
    let mut edits = vec![(
        reference.range.start,
        reference.range.end,
        column_replacement,
    )];
    for (cell, value) in reference_cells.into_iter().zip(cells.iter()) {
        let retained_cell_xml = &slide_xml[cell.range.start..cell.range.end];
        let inserted_cell_xml =
            clone_pptx_table_cell_with_text(slide_xml.as_str(), cell, value.as_str())?;
        let replacement = if position == "before" {
            format!("{inserted_cell_xml}{retained_cell_xml}")
        } else {
            format!("{retained_cell_xml}{inserted_cell_xml}")
        };
        edits.push((cell.range.start, cell.range.end, replacement));
    }
    let updated_xml = apply_pptx_xml_edits(slide_xml.as_str(), edits)?;
    validate_updated_pptx_table_columns(
        updated_xml.as_str(),
        table_number,
        simple.rows,
        simple.columns + 1,
    )?;
    let inserted_column = if position == "before" {
        reference_column
    } else {
        reference_column + 1
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
        "operation": "insert_table_column",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "table_number": table_number,
        "reference_column": reference_column,
        "position": position,
        "inserted_column": inserted_column,
        "previous_columns": simple.columns,
        "rows": simple.rows,
        "columns": simple.columns + 1,
        "format_cloned_from_reference_column": true,
        "table_frame_width_unchanged": true,
        "bytes": bytes,
    }))
}

pub(super) fn move_pptx_table_column(
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
    let column = required_pptx_index(arguments, "column", MAX_PPTX_TABLE_COLUMNS)?;
    let reference_column =
        required_pptx_index(arguments, "reference_column", MAX_PPTX_TABLE_COLUMNS)?;
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
            "selected PPTX table is not eligible for simple column movement: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if column > simple.columns {
        return Err(anyhow!(
            "table column {column} is out-of-range for a PPTX table with {} columns",
            simple.columns
        ));
    }
    if reference_column > simple.columns {
        return Err(anyhow!(
            "reference_column {reference_column} is out-of-range for a PPTX table with {} columns",
            simple.columns
        ));
    }
    ensure_changed_pptx_table_move(column, reference_column, position, "column")?;
    let expected_cells =
        required_pptx_table_column_cells(arguments, "expected_cells", simple.rows)?;
    ensure_expected_pptx_table_column(simple, column, expected_cells.as_slice())?;
    let reference_expected_cells =
        required_pptx_table_column_cells(arguments, "reference_expected_cells", simple.rows)?;
    ensure_expected_pptx_table_column(
        simple,
        reference_column,
        reference_expected_cells.as_slice(),
    )
    .map_err(|_| {
        anyhow!("selected PPTX reference column does not match reference_expected_cells")
    })?;
    let columns = simple_pptx_table_columns(slide_xml.as_str(), simple).map_err(|error| {
        anyhow!("selected PPTX table is not eligible for simple column movement: {error}")
    })?;
    let mut edits = vec![move_pptx_xml_element_edit(
        slide_xml.as_str(),
        columns[column - 1].range,
        columns[reference_column - 1].range,
        position,
    )?];
    for row in 1..=simple.rows {
        let source_cell = simple
            .cells
            .iter()
            .find(|cell| cell.row == row && cell.column == column)
            .ok_or_else(|| {
                anyhow!("simple PPTX table is missing source column {column} cell for row {row}")
            })?;
        let reference_cell = simple
            .cells
            .iter()
            .find(|cell| cell.row == row && cell.column == reference_column)
            .ok_or_else(|| {
                anyhow!(
                    "simple PPTX table is missing reference column {reference_column} cell for row {row}"
                )
            })?;
        edits.push(move_pptx_xml_element_edit(
            slide_xml.as_str(),
            source_cell.range,
            reference_cell.range,
            position,
        )?);
    }
    let updated_xml = apply_pptx_xml_edits(slide_xml.as_str(), edits)?;
    validate_updated_pptx_table_columns(
        updated_xml.as_str(),
        table_number,
        simple.rows,
        simple.columns,
    )?;
    let moved_column = moved_pptx_table_index(column, reference_column, position)?;
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
        "operation": "move_table_column",
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "slide_number": slide_number,
        "slide_file": slide_path,
        "table_number": table_number,
        "column": column,
        "reference_column": reference_column,
        "position": position,
        "moved_column": moved_column,
        "rows": simple.rows,
        "columns": simple.columns,
        "grid_column_and_cell_xml_preserved": true,
        "table_frame_width_unchanged": true,
        "bytes": bytes,
    }))
}
