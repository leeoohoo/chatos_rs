// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{bail, Context, Result};

use super::{MAX_EXCEL_COLUMNS, MAX_EXCEL_ROWS, MAX_RANGE_CELLS};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct A1Range {
    pub(super) canonical: String,
    pub(super) start_row: usize,
    pub(super) start_column: usize,
    pub(super) end_row: usize,
    pub(super) end_column: usize,
    pub(super) row_count: usize,
    pub(super) column_count: usize,
    pub(super) cell_count: usize,
}

pub(super) fn parse_a1_range(value: &str) -> Result<A1Range> {
    if value.chars().count() > 32 || value.chars().any(char::is_whitespace) {
        bail!("Excel range must be a bounded canonical A1 reference");
    }
    let mut parts = value.split(':');
    let first = parts
        .next()
        .context("Excel range is missing its first A1 component")?;
    let second = parts.next();
    if parts.next().is_some() {
        bail!("Excel range must be one contiguous canonical A1 reference");
    }
    let (start_row, start_column) = parse_a1_cell(first)?;
    let (end_row, end_column) = match second {
        Some(second) => parse_a1_cell(second)?,
        None => (start_row, start_column),
    };
    if end_row < start_row || end_column < start_column {
        bail!("Excel range must run from its upper-left cell to its lower-right cell");
    }
    let row_count = end_row - start_row + 1;
    let column_count = end_column - start_column + 1;
    let cell_count = row_count
        .checked_mul(column_count)
        .context("Excel range cell count overflow")?;
    if cell_count > MAX_RANGE_CELLS {
        bail!("Excel range exceeds the 256-cell read limit");
    }
    let canonical = if second.is_some() {
        format!(
            "{}{}:{}{}",
            excel_column_name(start_column),
            start_row,
            excel_column_name(end_column),
            end_row
        )
    } else {
        format!("{}{}", excel_column_name(start_column), start_row)
    };
    if canonical != value {
        bail!("Excel range must use canonical uppercase A1 notation");
    }
    Ok(A1Range {
        canonical,
        start_row,
        start_column,
        end_row,
        end_column,
        row_count,
        column_count,
        cell_count,
    })
}

pub(super) fn parse_a1_cell(value: &str) -> Result<(usize, usize)> {
    let split = value
        .find(|character: char| character.is_ascii_digit())
        .context("Excel range cell is missing a row number")?;
    let (column, row) = value.split_at(split);
    if column.is_empty()
        || column.len() > 3
        || !column.bytes().all(|byte| byte.is_ascii_uppercase())
        || row.is_empty()
        || row.starts_with('0')
        || !row.bytes().all(|byte| byte.is_ascii_digit())
    {
        bail!("Excel range cell is not canonical A1 notation");
    }
    let column = column.bytes().try_fold(0usize, |value, byte| {
        value
            .checked_mul(26)
            .and_then(|value| value.checked_add((byte - b'A' + 1) as usize))
            .context("Excel range column overflow")
    })?;
    let row = row
        .parse::<usize>()
        .context("Excel range row is not a valid integer")?;
    if column == 0 || column > MAX_EXCEL_COLUMNS || row == 0 || row > MAX_EXCEL_ROWS {
        bail!("Excel range cell is outside the worksheet grid");
    }
    Ok((row, column))
}

pub(super) fn excel_column_name(mut column: usize) -> String {
    let mut bytes = Vec::new();
    while column > 0 {
        column -= 1;
        bytes.push(b'A' + (column % 26) as u8);
        column /= 26;
    }
    bytes.reverse();
    bytes.into_iter().map(char::from).collect()
}

pub(super) fn formula_contains_external_reference(formula: &str) -> bool {
    let lower = formula.to_ascii_lowercase();
    if lower.contains("://") || formula.contains("\\\\") {
        return true;
    }
    if let Some(open) = lower.find('[') {
        if let Some(close) = lower[open + 1..].find(']') {
            if lower[open + close + 2..].contains('!') {
                return true;
            }
        }
    }
    formula
        .as_bytes()
        .windows(3)
        .any(|window| window[0].is_ascii_alphabetic() && window[1] == b':' && window[2] == b'\\')
}
