// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::formula_safety::validate_local_formula_expression;
use super::{
    ensure_exact_arguments, formula_contains_external_reference, parse_a1_cell, parse_a1_range,
    required_text, validate_bounded_text, RangeFormatInput, RangeReadTarget, RangeWriteInput,
    WriteCell, WriteCellSummary, MAX_CELL_TEXT_CHARACTERS, MAX_SNAPSHOT_ID_CHARACTERS,
};

pub(super) fn parse_range_write_input(arguments: &Value) -> Result<RangeWriteInput> {
    ensure_exact_arguments(
        arguments,
        &[
            "workbook_id",
            "worksheet_id",
            "range",
            "expected_snapshot_id",
            "cells",
        ],
    )?;
    let workbook_id = required_text(arguments, "workbook_id", 96)?.to_string();
    let worksheet_id = required_text(arguments, "worksheet_id", 96)?.to_string();
    let range = parse_a1_range(required_text(arguments, "range", 32)?)?;
    let expected_snapshot_id = required_text(
        arguments,
        "expected_snapshot_id",
        MAX_SNAPSHOT_ID_CHARACTERS,
    )?;
    expected_snapshot_id
        .strip_prefix("excel_range_")
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .ok_or_else(|| anyhow!("expected_snapshot_id must come from excel_read_range"))?;

    let rows = arguments
        .get("cells")
        .and_then(Value::as_array)
        .context("cells must be an exact rectangular row matrix")?;
    if rows.len() != range.row_count {
        bail!("cells row count must exactly match the target range");
    }
    let mut cells = Vec::with_capacity(range.cell_count);
    for row in rows {
        let row = row.as_array().context("each cells row must be an array")?;
        if row.len() != range.column_count {
            bail!("cells column count must exactly match the target range");
        }
        for cell in row {
            cells.push(parse_write_cell(cell)?);
        }
    }
    if cells.len() != range.cell_count {
        bail!("cells count must exactly match the target range");
    }
    Ok(RangeWriteInput {
        workbook_id,
        worksheet_id,
        range,
        expected_snapshot_id: expected_snapshot_id.to_string(),
        cells,
    })
}

pub(super) fn parse_range_format_input(arguments: &Value) -> Result<RangeFormatInput> {
    ensure_exact_arguments(
        arguments,
        &[
            "workbook_id",
            "worksheet_id",
            "range",
            "expected_snapshot_id",
            "preset",
        ],
    )?;
    let workbook_id = required_text(arguments, "workbook_id", 96)?.to_string();
    let worksheet_id = required_text(arguments, "worksheet_id", 96)?.to_string();
    let range = parse_a1_range(required_text(arguments, "range", 32)?)?;
    let expected_snapshot_id = required_text(
        arguments,
        "expected_snapshot_id",
        MAX_SNAPSHOT_ID_CHARACTERS,
    )?;
    expected_snapshot_id
        .strip_prefix("excel_range_")
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .ok_or_else(|| anyhow!("expected_snapshot_id must come from excel_read_range"))?;
    let preset = required_text(arguments, "preset", 32)?;
    let number_format = number_format_code_for_preset(preset)?;
    Ok(RangeFormatInput {
        workbook_id,
        worksheet_id,
        range,
        expected_snapshot_id: expected_snapshot_id.to_string(),
        preset: preset.to_string(),
        number_format: number_format.to_string(),
    })
}

fn number_format_code_for_preset(preset: &str) -> Result<&'static str> {
    match preset {
        "general" => Ok("General"),
        "integer" => Ok("0"),
        "decimal_2" => Ok("0.00"),
        "percent_2" => Ok("0.00%"),
        "date" => Ok("yyyy-mm-dd"),
        "datetime" => Ok("yyyy-mm-dd hh:mm"),
        "text" => Ok("@"),
        _ => bail!("Excel number format preset is not allowlisted"),
    }
}

pub(super) fn number_format_preset_for_code(number_format: &str) -> Option<&'static str> {
    match number_format {
        "General" => Some("general"),
        "0" => Some("integer"),
        "0.00" => Some("decimal_2"),
        "0.00%" => Some("percent_2"),
        "yyyy-mm-dd" => Some("date"),
        "yyyy-mm-dd hh:mm" => Some("datetime"),
        "@" => Some("text"),
        _ => None,
    }
}

fn parse_write_cell(value: &Value) -> Result<WriteCell> {
    let object = value
        .as_object()
        .context("each Excel write cell must be a typed object")?;
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .context("each Excel write cell requires a kind")?;
    match kind {
        "blank" => {
            ensure_exact_object_fields(object, &["kind"], "Excel blank write cell")?;
            Ok(WriteCell::Blank)
        }
        "value" => {
            ensure_exact_object_fields(object, &["kind", "value"], "Excel value write cell")?;
            let value = object
                .get("value")
                .context("Excel value write cell is missing value")?;
            let value = match value {
                Value::Bool(value) => Value::Bool(*value),
                Value::Number(value)
                    if value
                        .as_f64()
                        .is_some_and(|value| value.is_finite() && value.abs() <= 1.0e15) =>
                {
                    Value::Number(value.clone())
                }
                Value::String(value) => {
                    validate_safe_live_text(value, "write cell value")?;
                    Value::String(value.clone())
                }
                _ => bail!(
                    "Excel value write cell must contain a bounded boolean, number, or string"
                ),
            };
            Ok(WriteCell::Value(value))
        }
        "formula" => {
            ensure_exact_object_fields(object, &["kind", "formula"], "Excel formula write cell")?;
            let formula = object
                .get("formula")
                .and_then(Value::as_str)
                .context("Excel formula write cell is missing formula text")?;
            Ok(WriteCell::Formula(validate_live_formula(formula)?))
        }
        _ => bail!("Excel write cell kind must be blank, value, or formula"),
    }
}

fn ensure_exact_object_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
    label: &str,
) -> Result<()> {
    if object.len() != allowed.len()
        || object
            .keys()
            .any(|key| !allowed.iter().any(|allowed| key == allowed))
    {
        bail!("{label} contains unknown or missing fields");
    }
    Ok(())
}

pub(super) fn validate_live_formula(value: &str) -> Result<String> {
    validate_bounded_text(value, "write formula", MAX_CELL_TEXT_CHARACTERS)?;
    if value.trim() != value || !value.starts_with('=') || value.len() < 2 {
        bail!("Excel write formula must start with one equals sign and have no outer whitespace");
    }
    let expression = &value[1..];
    if formula_contains_external_reference(value) {
        bail!("Excel write formula contains unsupported dynamic, string, or external-link syntax");
    }
    validate_local_formula_expression(expression, |identifier| {
        let plain_identifier = identifier.replace('$', "");
        matches!(plain_identifier.as_str(), "TRUE" | "FALSE")
            || parse_a1_cell(plain_identifier.as_str()).is_ok()
    })
    .context("validate Excel write formula")?;
    Ok(value.to_string())
}

pub(super) fn validate_safe_live_text(value: &str, field: &str) -> Result<()> {
    validate_bounded_text(value, field, MAX_CELL_TEXT_CHARACTERS)?;
    if value.is_empty() {
        bail!("empty Excel text must be written as an explicit blank cell");
    }
    if value
        .trim_start()
        .chars()
        .next()
        .is_some_and(|character| matches!(character, '=' | '+' | '-' | '@' | '\''))
    {
        bail!("Excel text that could be interpreted as a formula is disabled");
    }
    Ok(())
}

fn write_cells_bridge_value(cells: &[WriteCell]) -> Value {
    Value::Array(
        cells
            .iter()
            .map(|cell| match cell {
                WriteCell::Blank => json!({"kind": "blank"}),
                WriteCell::Value(value) => json!({"kind": "value", "value": value}),
                WriteCell::Formula(formula) => {
                    json!({"kind": "formula", "formula": formula})
                }
            })
            .collect(),
    )
}

pub(super) fn write_cell_summary(cells: &[WriteCell]) -> Result<WriteCellSummary> {
    let mut blank_cells = 0usize;
    let mut value_cells = 0usize;
    let mut formula_cells = 0usize;
    let mut text_characters = 0usize;
    for cell in cells {
        match cell {
            WriteCell::Blank => blank_cells += 1,
            WriteCell::Value(Value::String(value)) => {
                value_cells += 1;
                text_characters += value.chars().count();
            }
            WriteCell::Value(_) => value_cells += 1,
            WriteCell::Formula(formula) => {
                formula_cells += 1;
                text_characters += formula.chars().count();
            }
        }
    }
    let encoded = serde_json::to_vec(&write_cells_bridge_value(cells))
        .context("encode Excel write approval summary")?;
    Ok(WriteCellSummary {
        blank_cells,
        value_cells,
        formula_cells,
        text_characters,
        content_sha256: hex::encode(Sha256::digest(encoded)),
    })
}

pub(super) fn range_write_bridge_request(
    target: &RangeReadTarget,
    input: &RangeWriteInput,
    expected_cells: &[Value],
) -> Value {
    json!({
        "schema_version": 1,
        "mutation_kind": "content",
        "runtime_instance": target.runtime_instance,
        "workbook_index": target.workbook_index,
        "workbook_name": target.workbook_name,
        "workbook_identity_source": target.workbook_identity_source,
        "workbook_read_only": target.workbook_read_only,
        "worksheet_index": target.worksheet_index,
        "worksheet_name": target.worksheet_name,
        "worksheet_visibility": target.worksheet_visibility,
        "worksheet_protected": target.worksheet_protected,
        "range_address": input.range.canonical,
        "start_row": input.range.start_row,
        "start_column": input.range.start_column,
        "row_count": input.range.row_count,
        "column_count": input.range.column_count,
        "cell_count": input.range.cell_count,
        "expected_cells": expected_cells,
        "write_cells": write_cells_bridge_value(input.cells.as_slice()),
    })
}

pub(super) fn range_format_bridge_request(
    target: &RangeReadTarget,
    input: &RangeFormatInput,
    expected_cells: &[Value],
) -> Value {
    json!({
        "schema_version": 1,
        "mutation_kind": "number_format",
        "runtime_instance": target.runtime_instance,
        "workbook_index": target.workbook_index,
        "workbook_name": target.workbook_name,
        "workbook_identity_source": target.workbook_identity_source,
        "workbook_read_only": target.workbook_read_only,
        "worksheet_index": target.worksheet_index,
        "worksheet_name": target.worksheet_name,
        "worksheet_visibility": target.worksheet_visibility,
        "worksheet_protected": target.worksheet_protected,
        "range_address": input.range.canonical,
        "start_row": input.range.start_row,
        "start_column": input.range.start_column,
        "row_count": input.range.row_count,
        "column_count": input.range.column_count,
        "cell_count": input.range.cell_count,
        "expected_cells": expected_cells,
        "number_format": input.number_format,
    })
}
