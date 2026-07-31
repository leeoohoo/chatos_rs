// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Map, Value};

use super::range_snapshot::range_snapshot_id;
use super::{
    excel_column_name, formula_contains_external_reference, number_format_preset_for_code,
    required_bool, required_usize, validate_bounded_text, A1Range, RangeReadTarget,
    MAX_CELL_TEXT_CHARACTERS, MAX_EXCEL_ROWS, MAX_NUMBER_FORMAT_CHARACTERS, MAX_RANGE_CELLS,
};

pub(super) fn normalize_range_read_response(
    response: Value,
    target: &RangeReadTarget,
    range: &A1Range,
) -> Result<Vec<Value>> {
    let object = response
        .as_object()
        .context("Excel range response must be an object")?;
    if object.get("schema_version").and_then(Value::as_u64) != Some(1) {
        bail!("Excel range response has an unsupported schema version");
    }
    for (field, expected) in [
        ("runtime_instance", target.runtime_instance.as_str()),
        ("workbook_name", target.workbook_name.as_str()),
        ("worksheet_name", target.worksheet_name.as_str()),
        ("range_address", range.canonical.as_str()),
    ] {
        if object.get(field).and_then(Value::as_str) != Some(expected) {
            bail!("Excel range response identity is stale or mismatched");
        }
    }
    for (field, expected) in [
        ("workbook_index", target.workbook_index),
        ("worksheet_index", target.worksheet_index),
        ("start_row", range.start_row),
        ("start_column", range.start_column),
        ("row_count", range.row_count),
        ("column_count", range.column_count),
        ("cell_count", range.cell_count),
    ] {
        if required_usize(object, field, MAX_EXCEL_ROWS.max(MAX_RANGE_CELLS))? != expected {
            bail!("Excel range response geometry is stale or mismatched");
        }
    }
    let cells = object
        .get("cells")
        .and_then(Value::as_array)
        .context("Excel range response cells must be an array")?;
    if cells.len() != range.cell_count {
        bail!("Excel range response cell count is inconsistent");
    }

    let mut normalized = Vec::with_capacity(cells.len());
    for (position, cell) in cells.iter().enumerate() {
        let cell = cell
            .as_object()
            .context("Excel range cell must be an object")?;
        let expected_row_offset = position / range.column_count;
        let expected_column_offset = position % range.column_count;
        if required_usize(cell, "row_offset", range.row_count)? != expected_row_offset
            || required_usize(cell, "column_offset", range.column_count)? != expected_column_offset
        {
            bail!("Excel range cell ordering is inconsistent");
        }
        let displayed_text = required_bounded_cell_text(cell, "displayed_text")?;
        let value = normalize_cell_scalar(cell.get("value"))?;
        let value_truncated = required_bool(cell, "value_truncated")?;
        let displayed_text_truncated = required_bool(cell, "displayed_text_truncated")?;
        let has_formula = required_bool(cell, "has_formula")?;
        let formula_truncated = required_bool(cell, "formula_truncated")?;
        let formula_hidden = required_bool(cell, "formula_hidden")?;
        let formula_external_reference = required_bool(cell, "formula_external_reference")?;
        let number_format_truncated = required_bool(cell, "number_format_truncated")?;
        let number_format_unavailable = required_bool(cell, "number_format_unavailable")?;
        let is_error = required_bool(cell, "is_error")?;
        if (formula_hidden || formula_external_reference) && !has_formula {
            bail!("Excel range response formula redaction metadata is inconsistent");
        }
        if is_error && !has_formula {
            bail!("Excel range response error metadata is inconsistent");
        }
        let formula = match cell.get("formula") {
            None | Some(Value::Null) => None,
            Some(Value::String(formula)) => {
                validate_bounded_text(formula, "formula", MAX_CELL_TEXT_CHARACTERS)?;
                if !has_formula || formula_hidden || formula_external_reference {
                    bail!("Excel range response exposed a disallowed formula");
                }
                if !formula.starts_with('=') || formula_contains_external_reference(formula) {
                    bail!("Excel range response formula is unsupported or externally linked");
                }
                Some(formula.clone())
            }
            _ => bail!("Excel range response formula must be text or null"),
        };
        if has_formula && !formula_hidden && !formula_external_reference && formula.is_none() {
            bail!("Excel range response omitted an accessible formula");
        }
        if formula.is_none() && formula_truncated {
            bail!("Excel range response formula truncation metadata is inconsistent");
        }
        let number_format = match cell.get("number_format") {
            None | Some(Value::Null) => None,
            Some(Value::String(number_format)) => {
                validate_bounded_text(
                    number_format,
                    "number format",
                    MAX_NUMBER_FORMAT_CHARACTERS,
                )?;
                Some(number_format.clone())
            }
            _ => bail!("Excel range response number format must be text or null"),
        };
        if number_format_unavailable {
            if number_format.is_some() || number_format_truncated {
                bail!("Excel range response unavailable number format metadata is inconsistent");
            }
        } else if number_format.as_deref().is_none_or(str::is_empty) {
            bail!("Excel range response omitted an available non-empty number format");
        }
        if value_truncated && !value.as_ref().is_some_and(Value::is_string) {
            bail!("Excel range response value truncation metadata is inconsistent");
        }
        let number_format_preset = number_format
            .as_deref()
            .and_then(number_format_preset_for_code);
        let number_format_available = !number_format_unavailable;
        let number_format_custom = number_format_available && number_format_preset.is_none();
        let row = range.start_row + expected_row_offset;
        let column = range.start_column + expected_column_offset;
        let status = if is_error {
            "error"
        } else if has_formula {
            "formula"
        } else if value
            .as_ref()
            .is_none_or(|value| value.is_null() || value.as_str().is_some_and(str::is_empty))
            && displayed_text.is_empty()
        {
            "blank"
        } else {
            "value"
        };
        normalized.push(json!({
            "address": format!("{}{}", excel_column_name(column), row),
            "value": value.unwrap_or(Value::Null),
            "value_truncated": value_truncated,
            "displayed_text": displayed_text,
            "displayed_text_truncated": displayed_text_truncated,
            "has_formula": has_formula,
            "formula": formula,
            "formula_truncated": formula_truncated,
            "formula_hidden": formula_hidden,
            "formula_external_reference": formula_external_reference,
            "number_format": number_format,
            "number_format_truncated": number_format_truncated,
            "number_format_unavailable": number_format_unavailable,
            "number_format_available": number_format_available,
            "number_format_exact": number_format_available && !number_format_truncated,
            "number_format_preset": number_format_preset,
            "number_format_custom": number_format_custom,
            "status": status,
        }));
    }
    Ok(normalized)
}

fn normalize_cell_scalar(value: Option<&Value>) -> Result<Option<Value>> {
    match value {
        None => bail!("Excel range response is missing a cell value"),
        Some(Value::Null) => Ok(Some(Value::Null)),
        Some(Value::Bool(value)) => Ok(Some(Value::Bool(*value))),
        Some(Value::Number(value)) => Ok(Some(Value::Number(value.clone()))),
        Some(Value::String(value)) => {
            validate_bounded_text(value, "cell value", MAX_CELL_TEXT_CHARACTERS)?;
            Ok(Some(Value::String(value.clone())))
        }
        _ => bail!("Excel range response cell value must be a bounded JSON scalar"),
    }
}

fn required_bounded_cell_text<'a>(object: &'a Map<String, Value>, field: &str) -> Result<&'a str> {
    let value = object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Excel range response is missing text {field}"))?;
    validate_bounded_text(value, field, MAX_CELL_TEXT_CHARACTERS)?;
    Ok(value)
}

pub(super) fn range_read_response(
    target: &RangeReadTarget,
    range: &A1Range,
    cells: Vec<Value>,
) -> Result<Value> {
    let range_snapshot_id = range_snapshot_id(target, range, cells.as_slice())?;
    let rows = public_cell_rows(cells.as_slice(), range.column_count)?;
    Ok(json!({
        "platform": std::env::consts::OS,
        "excel_running": true,
        "read_only": true,
        "safe_no_launch": true,
        "range_snapshot_id": range_snapshot_id,
        "workbook": {
            "workbook_id": target.workbook_id,
            "name": target.workbook_name,
            "index": target.workbook_index,
        },
        "worksheet": {
            "worksheet_id": target.worksheet_id,
            "name": target.worksheet_name,
            "index": target.worksheet_index,
            "protected": target.worksheet_protected,
        },
        "range": {
            "address": range.canonical,
            "start_row": range.start_row,
            "start_column": range.start_column,
            "row_count": range.row_count,
            "column_count": range.column_count,
            "cell_count": range.cell_count,
        },
        "cells": rows,
    }))
}

fn public_cell_projection(cell: &Value) -> Result<Value> {
    let mut object = cell
        .as_object()
        .context("normalized Excel range cell must be an object")?
        .clone();
    object.remove("number_format");
    object.remove("number_format_truncated");
    object.remove("number_format_unavailable");
    Ok(Value::Object(object))
}

pub(super) fn public_cell_rows(cells: &[Value], column_count: usize) -> Result<Vec<Value>> {
    if column_count == 0 {
        bail!("normalized Excel range column count must be positive");
    }
    cells
        .chunks(column_count)
        .map(|row| {
            row.iter()
                .map(public_cell_projection)
                .collect::<Result<Vec<_>>>()
                .map(Value::Array)
        })
        .collect()
}
