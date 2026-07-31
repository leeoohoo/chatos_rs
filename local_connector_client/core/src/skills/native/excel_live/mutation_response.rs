// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};

use super::range_response::{normalize_range_read_response, public_cell_rows};
use super::range_snapshot::range_snapshot_id;
use super::{A1Range, RangeFormatInput, RangeReadTarget, RangeWriteInput, WriteCell};

pub(super) fn normalize_range_write_response(
    response: Value,
    target: &RangeReadTarget,
    input: &RangeWriteInput,
    expected_cells: &[Value],
) -> Result<Vec<Value>> {
    let status = response
        .get("write_status")
        .and_then(Value::as_str)
        .context("Excel range write response is missing write_status")?
        .to_string();
    if status == "rollback_failed" {
        bail!("Excel write failed and the bridge could not verify complete rollback; inspect the workbook immediately and do not retry automatically");
    }
    let cells = normalize_range_read_response(response, target, &input.range)?;
    match status.as_str() {
        "written" => {
            if !desired_cells_match(input.cells.as_slice(), cells.as_slice())?
                || !same_number_formats(expected_cells, cells.as_slice())?
            {
                bail!(
                    "Excel bridge returned a write result that does not match the approved cells or preserve their number formats"
                );
            }
            Ok(cells)
        }
        "rolled_back" => {
            if cells != expected_cells {
                bail!("Excel write failed and rollback verification did not reproduce the exact prior snapshot");
            }
            bail!("Excel write failed after mutation, but the exact target range was restored and verified; inspect it before retrying")
        }
        _ => bail!("Excel range write response has an unsupported status"),
    }
}

pub(super) fn normalize_range_format_response(
    response: Value,
    target: &RangeReadTarget,
    input: &RangeFormatInput,
    expected_cells: &[Value],
) -> Result<Vec<Value>> {
    let status = response
        .get("write_status")
        .and_then(Value::as_str)
        .context("Excel number format response is missing write_status")?
        .to_string();
    if status == "rollback_failed" {
        bail!("Excel number format write failed and the bridge could not verify complete rollback; inspect the workbook immediately and do not retry automatically");
    }
    let cells = normalize_range_read_response(response, target, &input.range)?;
    match status.as_str() {
        "formatted" => {
            if !formatted_cells_match(
                expected_cells,
                cells.as_slice(),
                input.number_format.as_str(),
            )? {
                bail!("Excel bridge returned a number format result that changed cell contents or did not match the approved preset");
            }
            Ok(cells)
        }
        "rolled_back" => {
            if cells != expected_cells {
                bail!("Excel number format write failed and rollback verification did not reproduce the exact prior snapshot");
            }
            bail!("Excel number format write failed after mutation, but the exact target range was restored and verified; inspect it before retrying")
        }
        _ => bail!("Excel number format response has an unsupported status"),
    }
}

pub(super) fn same_number_formats(expected: &[Value], actual: &[Value]) -> Result<bool> {
    if expected.len() != actual.len() {
        return Ok(false);
    }
    for (expected, actual) in expected.iter().zip(actual) {
        let expected = expected
            .as_object()
            .context("normalized expected Excel cell must be an object")?;
        let actual = actual
            .as_object()
            .context("normalized actual Excel cell must be an object")?;
        if expected.get("number_format") != actual.get("number_format")
            || expected.get("number_format_truncated") != actual.get("number_format_truncated")
            || expected.get("number_format_unavailable") != actual.get("number_format_unavailable")
        {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn formatted_cells_match(
    expected: &[Value],
    actual: &[Value],
    number_format: &str,
) -> Result<bool> {
    if expected.len() != actual.len() {
        return Ok(false);
    }
    for (expected, actual) in expected.iter().zip(actual) {
        let expected = expected
            .as_object()
            .context("normalized expected Excel format cell must be an object")?;
        let actual = actual
            .as_object()
            .context("normalized actual Excel format cell must be an object")?;
        if !same_cell_content(expected, actual)
            || actual.get("number_format").and_then(Value::as_str) != Some(number_format)
            || actual
                .get("number_format_truncated")
                .and_then(Value::as_bool)
                != Some(false)
            || actual
                .get("number_format_unavailable")
                .and_then(Value::as_bool)
                != Some(false)
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn same_cell_content(expected: &Map<String, Value>, actual: &Map<String, Value>) -> bool {
    same_cell_scalar(
        expected.get("value").unwrap_or(&Value::Null),
        actual.get("value"),
    ) && [
        "value_truncated",
        "has_formula",
        "formula",
        "formula_truncated",
        "formula_hidden",
        "formula_external_reference",
        "status",
    ]
    .into_iter()
    .all(|field| expected.get(field) == actual.get(field))
}

pub(super) fn desired_cells_match(desired: &[WriteCell], actual: &[Value]) -> Result<bool> {
    if desired.len() != actual.len() {
        return Ok(false);
    }
    for (desired, actual) in desired.iter().zip(actual) {
        let actual = actual
            .as_object()
            .context("normalized Excel write result cell must be an object")?;
        let has_formula = actual
            .get("has_formula")
            .and_then(Value::as_bool)
            .context("normalized Excel write result formula state is missing")?;
        let matches = match desired {
            WriteCell::Blank => {
                !has_formula
                    && actual.get("status").and_then(Value::as_str) == Some("blank")
                    && actual.get("value").is_some_and(Value::is_null)
            }
            WriteCell::Value(value) => {
                !has_formula
                    && actual.get("status").and_then(Value::as_str) == Some("value")
                    && same_cell_scalar(value, actual.get("value"))
            }
            WriteCell::Formula(formula) => {
                has_formula
                    && actual.get("formula").and_then(Value::as_str) == Some(formula.as_str())
                    && actual.get("formula_hidden").and_then(Value::as_bool) == Some(false)
                    && actual
                        .get("formula_external_reference")
                        .and_then(Value::as_bool)
                        == Some(false)
            }
        };
        if !matches {
            return Ok(false);
        }
    }
    Ok(true)
}

fn same_cell_scalar(expected: &Value, actual: Option<&Value>) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    match (expected, actual) {
        (Value::Number(expected), Value::Number(actual)) => expected
            .as_f64()
            .zip(actual.as_f64())
            .is_some_and(|(expected, actual)| expected.to_bits() == actual.to_bits()),
        _ => expected == actual,
    }
}

pub(super) fn range_write_response(
    target: &RangeReadTarget,
    range: &A1Range,
    cells: Vec<Value>,
    cancel_requested_after_commit: bool,
) -> Result<Value> {
    let range_snapshot_id = range_snapshot_id(target, range, cells.as_slice())?;
    let rows = public_cell_rows(cells.as_slice(), range.column_count)?;
    Ok(json!({
        "platform": std::env::consts::OS,
        "excel_running": true,
        "safe_no_launch": true,
        "write_verified": true,
        "rollback_status": "not_needed",
        "save_performed": false,
        "export_performed": false,
        "explicit_recalculation_performed": false,
        "cancel_requested_after_commit": cancel_requested_after_commit,
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

pub(super) fn range_format_response(
    target: &RangeReadTarget,
    input: &RangeFormatInput,
    cells: Vec<Value>,
    cancel_requested_after_commit: bool,
) -> Result<Value> {
    let range_snapshot_id = range_snapshot_id(target, &input.range, cells.as_slice())?;
    let rows = public_cell_rows(cells.as_slice(), input.range.column_count)?;
    Ok(json!({
        "platform": std::env::consts::OS,
        "excel_running": true,
        "safe_no_launch": true,
        "format_verified": true,
        "number_format_preset": input.preset,
        "rollback_status": "not_needed",
        "save_performed": false,
        "export_performed": false,
        "explicit_recalculation_performed": false,
        "cancel_requested_after_commit": cancel_requested_after_commit,
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
            "address": input.range.canonical,
            "start_row": input.range.start_row,
            "start_column": input.range.start_column,
            "row_count": input.range.row_count,
            "column_count": input.range.column_count,
            "cell_count": input.range.cell_count,
        },
        "cells": rows,
    }))
}
