// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(test)]
use std::process::Command;
use std::sync::atomic::AtomicBool;

use anyhow::Result;
#[cfg(test)]
use serde_json::json;
use serde_json::Value;

mod approval;
mod macos_range_write_script;
mod mutation_execution;
mod mutation_input;
mod mutation_response;
mod mutation_safety;
mod platform_bridge;
mod platform_snapshot_scripts;
mod range_read_scripts;
mod range_reference;
mod range_response;
mod range_snapshot;
mod range_target;
mod read_execution;
mod script_fragments;
mod snapshot_identity;
mod tool_schema;
mod validation;
mod windows_range_write_script;

#[path = "formula_safety.rs"]
pub(super) mod formula_safety;

#[cfg(test)]
use macos_range_write_script::macos_range_write_script;
use mutation_input::number_format_preset_for_code;
#[cfg(test)]
use mutation_input::validate_live_formula;
#[cfg(test)]
use mutation_input::{parse_range_format_input, parse_range_write_input};
#[cfg(test)]
use mutation_response::{
    desired_cells_match, formatted_cells_match, normalize_range_format_response,
    normalize_range_write_response, range_format_response,
};
#[cfg(test)]
use mutation_safety::{
    ensure_snapshot_cells_are_format_safe, ensure_snapshot_cells_are_write_safe,
};
#[cfg(test)]
use platform_snapshot_scripts::{
    MACOS_SNAPSHOT_SCRIPT, MACOS_STATUS_SCRIPT, WINDOWS_SNAPSHOT_SCRIPT, WINDOWS_STATUS_SCRIPT,
};
#[cfg(test)]
use range_read_scripts::{macos_range_read_script, windows_range_read_script};
use range_reference::{
    excel_column_name, formula_contains_external_reference, parse_a1_cell, parse_a1_range, A1Range,
};
#[cfg(test)]
use range_response::{normalize_range_read_response, range_read_response};
#[cfg(test)]
use range_snapshot::range_snapshot_id;
#[cfg(test)]
use range_target::{ensure_write_target_is_mutable, resolve_range_read_target};
#[cfg(test)]
use snapshot_identity::normalize_snapshot;
use validation::{
    ensure_exact_arguments, optional_bounded_text, required_bool, required_bounded_text,
    required_text, required_usize, validate_bounded_text,
};
#[cfg(test)]
use windows_range_write_script::windows_range_write_script;

const MAX_OPEN_WORKBOOKS: usize = 32;
const MAX_WORKSHEETS_PER_WORKBOOK: usize = 64;
const MAX_WORKBOOK_NAME_CHARACTERS: usize = 512;
const MAX_WORKSHEET_NAME_CHARACTERS: usize = 64;
const MAX_IDENTITY_SOURCE_CHARACTERS: usize = 4096;
const MAX_RANGE_CELLS: usize = 256;
const MAX_CELL_TEXT_CHARACTERS: usize = 128;
const MAX_NUMBER_FORMAT_CHARACTERS: usize = 128;
const MAX_SNAPSHOT_ID_CHARACTERS: usize = 96;
const MAX_EXCEL_ROWS: usize = 1_048_576;
const MAX_EXCEL_COLUMNS: usize = 16_384;

#[derive(Clone, Debug, Eq, PartialEq)]
struct RangeReadTarget {
    runtime_instance: String,
    workbook_id: String,
    workbook_index: usize,
    workbook_name: String,
    workbook_identity_source: String,
    workbook_read_only: bool,
    worksheet_id: String,
    worksheet_index: usize,
    worksheet_name: String,
    worksheet_visibility: String,
    worksheet_protected: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum WriteCell {
    Blank,
    Value(Value),
    Formula(String),
}

#[derive(Clone, Debug, PartialEq)]
struct RangeWriteInput {
    workbook_id: String,
    worksheet_id: String,
    range: A1Range,
    expected_snapshot_id: String,
    cells: Vec<WriteCell>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RangeFormatInput {
    workbook_id: String,
    worksheet_id: String,
    range: A1Range,
    expected_snapshot_id: String,
    preset: String,
    number_format: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WriteCellSummary {
    blank_cells: usize,
    value_cells: usize,
    formula_cells: usize,
    text_characters: usize,
    content_sha256: String,
}

pub(super) fn tool_definitions(include_write: bool) -> Vec<Value> {
    tool_schema::tool_definitions(include_write)
}

pub(super) fn requires_interactive_approval(operation: &str) -> bool {
    tool_schema::requires_interactive_approval(operation)
}

pub(super) fn dependency_error() -> Option<String> {
    platform_bridge::dependency_error()
}

pub(super) fn approval_command(
    operation: &str,
    arguments: &Value,
) -> Result<(String, Vec<String>)> {
    approval::approval_command(operation, arguments)
}

pub(super) fn execute_approved(
    operation: &str,
    arguments: &Value,
    approved_command_args: Option<&[String]>,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    approval::execute_approved(
        operation,
        arguments,
        approved_command_args,
        action_cancelled,
    )
}

pub(super) fn execute(operation: &str, arguments: &Value) -> Result<Value> {
    read_execution::execute(operation, arguments)
}

fn execute_range_write(arguments: &Value, action_cancelled: Option<&AtomicBool>) -> Result<Value> {
    mutation_execution::execute_range_write(arguments, action_cancelled)
}

fn execute_range_format_write(
    arguments: &Value,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    mutation_execution::execute_range_format_write(arguments, action_cancelled)
}

#[cfg(test)]
fn execute_with_snapshot(operation: &str, arguments: &Value, snapshot: Value) -> Result<Value> {
    read_execution::execute_with_snapshot(operation, arguments, snapshot)
}

#[cfg(test)]
mod tests;
