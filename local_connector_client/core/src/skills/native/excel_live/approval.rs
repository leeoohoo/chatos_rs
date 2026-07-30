// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::AtomicBool;

use anyhow::{bail, Result};
use serde_json::Value;

use super::mutation_input::{
    parse_range_format_input, parse_range_write_input, write_cell_summary,
};
use super::{execute_range_format_write, execute_range_write, requires_interactive_approval};

pub(super) fn approval_command(
    operation: &str,
    arguments: &Value,
) -> Result<(String, Vec<String>)> {
    let args = match operation {
        "excel_write_range" => {
            let input = parse_range_write_input(arguments)?;
            let summary = write_cell_summary(input.cells.as_slice())?;
            vec![
                "write_range".to_string(),
                format!("workbook_id={}", input.workbook_id),
                format!("worksheet_id={}", input.worksheet_id),
                format!("range={}", input.range.canonical),
                format!("expected_snapshot_id={}", input.expected_snapshot_id),
                format!("cell_count={}", input.range.cell_count),
                format!("blank_cells={}", summary.blank_cells),
                format!("value_cells={}", summary.value_cells),
                format!("formula_cells={}", summary.formula_cells),
                format!("text_characters={}", summary.text_characters),
                format!("content_sha256={}", summary.content_sha256),
            ]
        }
        "excel_set_number_format" => {
            let input = parse_range_format_input(arguments)?;
            vec![
                "set_number_format".to_string(),
                format!("workbook_id={}", input.workbook_id),
                format!("worksheet_id={}", input.worksheet_id),
                format!("range={}", input.range.canonical),
                format!("expected_snapshot_id={}", input.expected_snapshot_id),
                format!("cell_count={}", input.range.cell_count),
                format!("preset={}", input.preset),
            ]
        }
        _ => bail!("Excel Live Control operation does not support interactive approval"),
    };
    Ok(("chatos-excel-live".to_string(), args))
}

pub(super) fn execute_approved(
    operation: &str,
    arguments: &Value,
    approved_command_args: Option<&[String]>,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    if !requires_interactive_approval(operation) {
        bail!("Excel Live Control operation does not support approved execution");
    }
    let (_, expected) = approval_command(operation, arguments)?;
    if approved_command_args != Some(expected.as_slice()) {
        bail!("approved Excel write no longer matches the exact reviewed arguments");
    }
    match operation {
        "excel_write_range" => execute_range_write(arguments, action_cancelled),
        "excel_set_number_format" => execute_range_format_write(arguments, action_cancelled),
        _ => unreachable!("approval-gated Excel operation was already checked"),
    }
}
