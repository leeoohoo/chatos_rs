// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use serde_json::Value;

use super::format_helpers::csv_formula_injection_risk;
use super::{
    MAX_ARTIFACT_BYTES, MAX_TABLE_CELLS, MAX_TEXT_CELL_CHARS, MAX_TEXT_TABLE_COLUMNS,
    MAX_TEXT_TABLE_ROWS,
};

#[derive(Debug)]
pub(super) struct DelimitedDocument {
    pub(super) rows: Vec<Vec<String>>,
    pub(super) line_ending: Option<&'static str>,
    pub(super) terminal_record_separator: bool,
    pub(super) utf8_bom: bool,
}

pub(super) fn text_table_rows(
    arguments: &Value,
    field: &str,
    require_non_empty: bool,
) -> Result<Vec<Vec<String>>> {
    let rows = arguments
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("{field} must be an array"))?;
    if rows.len() > MAX_TEXT_TABLE_ROWS || (require_non_empty && rows.is_empty()) {
        return Err(anyhow!(
            "{field} must contain between {} and {MAX_TEXT_TABLE_ROWS} rows",
            usize::from(require_non_empty)
        ));
    }
    let mut output = Vec::with_capacity(rows.len());
    let mut cell_count = 0usize;
    for row in rows {
        let cells = row
            .as_array()
            .ok_or_else(|| anyhow!("each {field} row must be an array"))?;
        if cells.is_empty() {
            return Err(anyhow!("each {field} row must contain at least one cell"));
        }
        if cells.len() > MAX_TEXT_TABLE_COLUMNS {
            return Err(anyhow!(
                "{field} row exceeds the {MAX_TEXT_TABLE_COLUMNS} column safety limit"
            ));
        }
        cell_count = cell_count.saturating_add(cells.len());
        if cell_count > MAX_TABLE_CELLS {
            return Err(anyhow!(
                "{field} exceeds the {MAX_TABLE_CELLS} cell safety limit"
            ));
        }
        let mut converted = Vec::with_capacity(cells.len());
        for value in cells {
            let mut cell = match value {
                Value::Null => String::new(),
                Value::Bool(value) => value.to_string(),
                Value::Number(value) => value.to_string(),
                Value::String(value) => value.clone(),
                _ => {
                    return Err(anyhow!(
                        "{field} cells must be null, boolean, number, or string values"
                    ))
                }
            };
            if matches!(value, Value::String(_)) && csv_formula_injection_risk(cell.as_str()) {
                cell.insert(0, '\'');
            }
            if cell.chars().count() > MAX_TEXT_CELL_CHARS {
                return Err(anyhow!(
                    "{field} cell exceeds the {MAX_TEXT_CELL_CHARS} character safety limit"
                ));
            }
            converted.push(cell);
        }
        output.push(converted);
    }
    Ok(output)
}

pub(super) fn parse_delimited(
    text: &str,
    delimiter: char,
    label: &str,
) -> Result<DelimitedDocument> {
    if !matches!(delimiter, ',' | '\t') {
        return Err(anyhow!("unsupported delimited-text separator"));
    }
    let (utf8_bom, text) = text
        .strip_prefix('\u{feff}')
        .map_or((false, text), |stripped| (true, stripped));
    if text.is_empty() {
        return Ok(DelimitedDocument {
            rows: Vec::new(),
            line_ending: None,
            terminal_record_separator: false,
            utf8_bom,
        });
    }
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut cell_chars = 0usize;
    let mut cell_started = false;
    let mut quoted = false;
    let mut closed_quote = false;
    let mut line_ending = None;
    let mut terminal_record_separator = false;
    let mut cell_count = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if quoted {
            if character == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                    cell_chars = cell_chars.saturating_add(1);
                } else {
                    quoted = false;
                    closed_quote = true;
                }
            } else {
                cell.push(character);
                cell_chars = cell_chars.saturating_add(1);
            }
            if cell_chars > MAX_TEXT_CELL_CHARS {
                return Err(anyhow!(
                    "{label} cell exceeds the {MAX_TEXT_CELL_CHARS} character safety limit"
                ));
            }
            continue;
        }
        if closed_quote && character != delimiter && !matches!(character, '\r' | '\n') {
            return Err(anyhow!(
                "{label} quoted field contains characters after its closing quote"
            ));
        }
        match character {
            '"' if !cell_started => {
                quoted = true;
                cell_started = true;
                terminal_record_separator = false;
            }
            '"' => return Err(anyhow!("{label} quote must begin a quoted field")),
            character if character == delimiter => {
                push_delimited_cell(&mut row, &mut cell, &mut cell_count, label)?;
                cell_chars = 0;
                cell_started = false;
                closed_quote = false;
                terminal_record_separator = false;
            }
            '\r' => {
                if chars.next() != Some('\n') {
                    return Err(anyhow!("{label} records must use LF or CRLF line endings"));
                }
                note_delimited_line_ending(&mut line_ending, "\r\n", label)?;
                push_delimited_record(&mut rows, &mut row, &mut cell, &mut cell_count, label)?;
                cell_chars = 0;
                cell_started = false;
                closed_quote = false;
                terminal_record_separator = true;
            }
            '\n' => {
                note_delimited_line_ending(&mut line_ending, "\n", label)?;
                push_delimited_record(&mut rows, &mut row, &mut cell, &mut cell_count, label)?;
                cell_chars = 0;
                cell_started = false;
                closed_quote = false;
                terminal_record_separator = true;
            }
            _ => {
                cell.push(character);
                cell_chars = cell_chars.saturating_add(1);
                cell_started = true;
                terminal_record_separator = false;
                if cell_chars > MAX_TEXT_CELL_CHARS {
                    return Err(anyhow!(
                        "{label} cell exceeds the {MAX_TEXT_CELL_CHARS} character safety limit"
                    ));
                }
            }
        }
    }
    if quoted {
        return Err(anyhow!("{label} contains an unterminated quoted field"));
    }
    if !terminal_record_separator {
        push_delimited_record(&mut rows, &mut row, &mut cell, &mut cell_count, label)?;
    }
    Ok(DelimitedDocument {
        rows,
        line_ending,
        terminal_record_separator,
        utf8_bom,
    })
}

fn note_delimited_line_ending(
    current: &mut Option<&'static str>,
    next: &'static str,
    label: &str,
) -> Result<()> {
    if current.is_some_and(|value| value != next) {
        return Err(anyhow!("{label} contains mixed record line endings"));
    }
    *current = Some(next);
    Ok(())
}

fn push_delimited_cell(
    row: &mut Vec<String>,
    cell: &mut String,
    cell_count: &mut usize,
    label: &str,
) -> Result<()> {
    if row.len() >= MAX_TEXT_TABLE_COLUMNS {
        return Err(anyhow!(
            "{label} row exceeds the {MAX_TEXT_TABLE_COLUMNS} column safety limit"
        ));
    }
    *cell_count = cell_count.saturating_add(1);
    if *cell_count > MAX_TABLE_CELLS {
        return Err(anyhow!(
            "{label} exceeds the {MAX_TABLE_CELLS} cell safety limit"
        ));
    }
    row.push(std::mem::take(cell));
    Ok(())
}

fn push_delimited_record(
    rows: &mut Vec<Vec<String>>,
    row: &mut Vec<String>,
    cell: &mut String,
    cell_count: &mut usize,
    label: &str,
) -> Result<()> {
    if rows.len() >= MAX_TEXT_TABLE_ROWS {
        return Err(anyhow!(
            "{label} exceeds the {MAX_TEXT_TABLE_ROWS} row safety limit"
        ));
    }
    push_delimited_cell(row, cell, cell_count, label)?;
    rows.push(std::mem::take(row));
    Ok(())
}

pub(super) fn serialize_delimited(
    rows: &[Vec<String>],
    delimiter: char,
    line_ending: &str,
    terminal_record_separator: bool,
    utf8_bom: bool,
    label: &str,
) -> Result<String> {
    if !matches!(delimiter, ',' | '\t') {
        return Err(anyhow!("unsupported delimited-text separator"));
    }
    if !matches!(line_ending, "\r\n" | "\n") {
        return Err(anyhow!("{label} line ending must be LF or CRLF"));
    }
    let mut output = String::new();
    if utf8_bom {
        output.push('\u{feff}');
    }
    for (row_index, row) in rows.iter().enumerate() {
        if row_index > 0 {
            output.push_str(line_ending);
        }
        for (column_index, cell) in row.iter().enumerate() {
            if column_index > 0 {
                output.push(delimiter);
            }
            if cell.contains([delimiter, '"', '\r', '\n']) {
                output.push('"');
                output.push_str(cell.replace('"', "\"\"").as_str());
                output.push('"');
            } else {
                output.push_str(cell.as_str());
            }
            if output.len() as u64 > MAX_ARTIFACT_BYTES {
                return Err(anyhow!("{label} exceeds the 100 MiB safety limit"));
            }
        }
    }
    if terminal_record_separator && !rows.is_empty() {
        output.push_str(line_ending);
    }
    if output.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("{label} exceeds the 100 MiB safety limit"));
    }
    Ok(output)
}

pub(super) fn parse_text_cell_reference(value: &str, label: &str) -> Result<(usize, usize)> {
    let split = value
        .bytes()
        .position(|byte| byte.is_ascii_digit())
        .ok_or_else(|| anyhow!("{label} cell reference must use A1 notation"))?;
    let (column, row) = value.split_at(split);
    if column.is_empty()
        || column.len() > 3
        || !column.bytes().all(|byte| byte.is_ascii_alphabetic())
        || row.is_empty()
        || !row.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(anyhow!("{label} cell reference must use A1 notation"));
    }
    let mut column_number = 0usize;
    for byte in column.bytes() {
        column_number = column_number
            .saturating_mul(26)
            .saturating_add((byte.to_ascii_uppercase() - b'A' + 1) as usize);
    }
    let row_number = row
        .parse::<usize>()
        .with_context(|| format!("{label} cell row is outside the supported range"))?;
    if column_number == 0
        || column_number > MAX_TEXT_TABLE_COLUMNS
        || row_number == 0
        || row_number > MAX_TEXT_TABLE_ROWS
    {
        return Err(anyhow!(
            "{label} cell reference is outside the supported range"
        ));
    }
    Ok((column_number, row_number))
}
