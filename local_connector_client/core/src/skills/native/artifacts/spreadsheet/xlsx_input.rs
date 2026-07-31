// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::skills::native::excel_live::formula_safety::validate_local_formula_expression;

use super::super::{optional_text, MAX_TABLE_CELLS};
use super::xlsx_model::{CellInput, CellValue, NumberFormat, PrimitiveCellValue, WorksheetInput};
use super::{
    parse_cell_reference, parse_column_reference, MAX_CELL_TEXT_CHARS, MAX_COLUMN_WIDTH,
    MAX_FORMULA_BYTES, MAX_XLSX_COLUMNS, MAX_XLSX_ROWS, MAX_XLSX_SHEETS,
};

pub(super) fn parse_worksheets(arguments: &Value) -> Result<Vec<WorksheetInput>> {
    let worksheets = if let Some(items) = arguments.get("worksheets") {
        if arguments.get("rows").is_some() || arguments.get("sheet_name").is_some() {
            return Err(anyhow!(
                "create_xlsx accepts either worksheets or legacy rows/sheet_name, not both"
            ));
        }
        let items = items
            .as_array()
            .ok_or_else(|| anyhow!("worksheets must be an array"))?;
        if items.is_empty() || items.len() > MAX_XLSX_SHEETS {
            return Err(anyhow!(
                "worksheets must contain between 1 and {MAX_XLSX_SHEETS} items"
            ));
        }
        let mut output = Vec::with_capacity(items.len());
        for item in items {
            let object = item
                .as_object()
                .ok_or_else(|| anyhow!("each worksheet must be an object"))?;
            let name = object
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("worksheet name is required"))?
                .to_string();
            validate_sheet_name(name.as_str())?;
            let rows = parse_cell_rows(
                object
                    .get("rows")
                    .ok_or_else(|| anyhow!("worksheet rows are required"))?,
                "worksheet rows",
            )?;
            let freeze_rows = object
                .get("freeze_rows")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if freeze_rows > 1_000 || freeze_rows > u64::from(MAX_XLSX_ROWS - 1) {
                return Err(anyhow!("freeze_rows must be between 0 and 1000"));
            }
            let column_widths = parse_column_widths(object.get("column_widths"))?;
            output.push(WorksheetInput {
                name,
                rows,
                freeze_rows: freeze_rows as u32,
                column_widths,
            });
        }
        output
    } else {
        let name = optional_text(arguments, "sheet_name").unwrap_or_else(|| "Sheet1".to_string());
        validate_sheet_name(name.as_str())?;
        vec![WorksheetInput {
            name,
            rows: parse_cell_rows(
                arguments
                    .get("rows")
                    .ok_or_else(|| anyhow!("rows is required"))?,
                "rows",
            )?,
            freeze_rows: 0,
            column_widths: BTreeMap::new(),
        }]
    };
    let mut names = HashSet::new();
    for sheet in &worksheets {
        if !names.insert(sheet.name.to_lowercase()) {
            return Err(anyhow!(
                "XLSX worksheet names must be unique case-insensitively"
            ));
        }
    }
    let cells = worksheets
        .iter()
        .flat_map(|sheet| sheet.rows.iter())
        .map(Vec::len)
        .sum::<usize>();
    if cells > MAX_TABLE_CELLS {
        return Err(anyhow!(
            "workbook exceeds the {MAX_TABLE_CELLS} cell safety limit"
        ));
    }
    Ok(worksheets)
}

pub(super) fn parse_cell_rows(value: &Value, label: &str) -> Result<Vec<Vec<CellInput>>> {
    let rows = value
        .as_array()
        .ok_or_else(|| anyhow!("{label} must be an array"))?;
    if rows.len() > MAX_XLSX_ROWS as usize {
        return Err(anyhow!("{label} exceeds the XLSX row limit"));
    }
    let mut cells = 0usize;
    let mut output = Vec::with_capacity(rows.len());
    for row in rows {
        let row = row
            .as_array()
            .ok_or_else(|| anyhow!("each {label} row must be an array"))?;
        if row.len() > MAX_XLSX_COLUMNS as usize {
            return Err(anyhow!("{label} row exceeds the XLSX column limit"));
        }
        cells = cells.saturating_add(row.len());
        if cells > MAX_TABLE_CELLS {
            return Err(anyhow!(
                "{label} exceeds the {MAX_TABLE_CELLS} cell safety limit"
            ));
        }
        output.push(
            row.iter()
                .map(parse_cell_input)
                .collect::<Result<Vec<_>>>()?,
        );
    }
    Ok(output)
}

pub(super) fn parse_cell_input(value: &Value) -> Result<CellInput> {
    if let Some(object) = value.as_object() {
        if object.keys().any(|key| {
            !matches!(
                key.as_str(),
                "value" | "formula" | "cached_value" | "number_format"
            )
        }) {
            return Err(anyhow!("XLSX cell object contains an unsupported field"));
        }
        let number_format = object
            .get("number_format")
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| anyhow!("number_format must be a string"))
                    .and_then(NumberFormat::parse)
            })
            .transpose()?
            .flatten();
        let cell_value = if let Some(formula) = object.get("formula") {
            if object.contains_key("value") {
                return Err(anyhow!("formula cells cannot also contain value"));
            }
            let expression = validate_formula(
                formula
                    .as_str()
                    .ok_or_else(|| anyhow!("formula must be a string"))?,
            )?;
            let cached_value = object
                .get("cached_value")
                .map(parse_primitive_cell_value)
                .transpose()?;
            CellValue::Formula {
                expression,
                cached_value,
            }
        } else {
            if object.contains_key("cached_value") {
                return Err(anyhow!("cached_value is only valid for formula cells"));
            }
            CellValue::Primitive(parse_primitive_cell_value(
                object
                    .get("value")
                    .ok_or_else(|| anyhow!("XLSX cell object requires value or formula"))?,
            )?)
        };
        if number_format.is_some() {
            let incompatible = matches!(
                &cell_value,
                CellValue::Primitive(PrimitiveCellValue::Text(_) | PrimitiveCellValue::Bool(_))
                    | CellValue::Formula {
                        cached_value: Some(
                            PrimitiveCellValue::Text(_) | PrimitiveCellValue::Bool(_)
                        ),
                        ..
                    }
            );
            if incompatible {
                return Err(anyhow!(
                    "XLSX number_format requires a numeric value, blank value, or formula with a numeric cached_value"
                ));
            }
        }
        Ok(CellInput {
            value: cell_value,
            number_format,
        })
    } else {
        Ok(CellInput {
            value: CellValue::Primitive(parse_primitive_cell_value(value)?),
            number_format: None,
        })
    }
}

fn parse_primitive_cell_value(value: &Value) -> Result<PrimitiveCellValue> {
    match value {
        Value::Null => Ok(PrimitiveCellValue::Blank),
        Value::Bool(value) => Ok(PrimitiveCellValue::Bool(*value)),
        Value::Number(value) => Ok(PrimitiveCellValue::Number(value.to_string())),
        Value::String(value) => {
            if value.chars().count() > MAX_CELL_TEXT_CHARS {
                return Err(anyhow!(
                    "XLSX cell text exceeds the {MAX_CELL_TEXT_CHARS} character limit"
                ));
            }
            Ok(PrimitiveCellValue::Text(value.clone()))
        }
        _ => Err(anyhow!(
            "XLSX cell values must be null, boolean, number, string, or a supported cell object"
        )),
    }
}

fn parse_column_widths(value: Option<&Value>) -> Result<BTreeMap<u16, f64>> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let entries = value
        .as_array()
        .ok_or_else(|| anyhow!("column_widths must be an array"))?;
    if entries.len() > 256 {
        return Err(anyhow!("column_widths exceeds the 256 item limit"));
    }
    let mut widths = BTreeMap::new();
    for entry in entries {
        let object = entry
            .as_object()
            .ok_or_else(|| anyhow!("each column_widths item must be an object"))?;
        let column = parse_column_reference(
            object
                .get("column")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("column_widths column is required"))?,
        )?;
        let width = object
            .get("width")
            .and_then(Value::as_f64)
            .ok_or_else(|| anyhow!("column_widths width must be a number"))?;
        if !width.is_finite() || !(0.1..=MAX_COLUMN_WIDTH).contains(&width) {
            return Err(anyhow!(
                "column_widths width must be between 0.1 and {MAX_COLUMN_WIDTH}"
            ));
        }
        if widths.insert(column, width).is_some() {
            return Err(anyhow!("column_widths contains a duplicate column"));
        }
    }
    Ok(widths)
}

pub(super) fn validate_sheet_name(value: &str) -> Result<()> {
    let chars = value.chars().count();
    if chars == 0 || chars > 31 || value.trim().is_empty() {
        return Err(anyhow!(
            "worksheet name must contain between 1 and 31 characters"
        ));
    }
    if value.starts_with('\'')
        || value.ends_with('\'')
        || value.chars().any(|character| {
            character.is_control() || matches!(character, ':' | '\\' | '/' | '?' | '*' | '[' | ']')
        })
    {
        return Err(anyhow!("worksheet name contains an unsupported character"));
    }
    Ok(())
}

pub(super) fn validate_formula(value: &str) -> Result<String> {
    let expression = value
        .trim()
        .strip_prefix('=')
        .unwrap_or(value.trim())
        .trim();
    if expression.is_empty() || expression.len() > MAX_FORMULA_BYTES {
        return Err(anyhow!(
            "formula must contain between 1 and {MAX_FORMULA_BYTES} bytes"
        ));
    }
    validate_local_formula_expression(expression, |identifier| {
        let plain_identifier = identifier.replace('$', "");
        matches!(
            plain_identifier.to_ascii_uppercase().as_str(),
            "TRUE" | "FALSE"
        ) || parse_cell_reference(plain_identifier.as_str()).is_ok()
    })?;
    Ok(expression.to_string())
}
