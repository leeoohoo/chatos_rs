// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use anyhow::{anyhow, bail, Result};
use serde_json::Value;

use super::mutation_input::{
    parse_range_format_input, parse_range_write_input, range_format_bridge_request,
    range_write_bridge_request,
};
use super::mutation_response::{
    desired_cells_match, formatted_cells_match, normalize_range_format_response,
    normalize_range_write_response, range_format_response, range_write_response,
    same_number_formats,
};
use super::mutation_safety::{
    ensure_snapshot_cells_are_format_safe, ensure_snapshot_cells_are_write_safe,
};
use super::platform_bridge::{read_platform_range, read_platform_snapshot, write_platform_range};
use super::range_response::normalize_range_read_response;
use super::range_snapshot::range_snapshot_id;
use super::range_target::{
    ensure_write_target_is_mutable, range_read_bridge_request, resolve_range_read_target,
};
use super::read_execution::excel_not_running_error;
use super::snapshot_identity::normalize_snapshot;

static EXCEL_WRITE_LOCK: Mutex<()> = Mutex::new(());

pub(super) fn execute_range_write(
    arguments: &Value,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        bail!("approved Excel write was cancelled before execution");
    }
    let _write_guard = EXCEL_WRITE_LOCK
        .lock()
        .map_err(|_| anyhow!("Excel live write lock is unavailable"))?;
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        bail!("approved Excel write was cancelled while waiting for another write");
    }
    let input = parse_range_write_input(arguments)?;
    let before = read_platform_snapshot()?;
    let normalized_before = normalize_snapshot(before.clone())?;
    if !normalized_before
        .get("running")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(excel_not_running_error(&normalized_before));
    }
    let target = resolve_range_read_target(
        &before,
        &normalized_before,
        input.workbook_id.as_str(),
        input.worksheet_id.as_str(),
    )?;
    ensure_write_target_is_mutable(&target)?;

    let current = read_platform_range(&range_read_bridge_request(&target, &input.range))?;
    let current_cells = normalize_range_read_response(current, &target, &input.range)?;
    let current_snapshot_id = range_snapshot_id(&target, &input.range, current_cells.as_slice())?;
    if current_snapshot_id != input.expected_snapshot_id {
        bail!("Excel range changed after it was read; read the exact range again before writing");
    }
    ensure_snapshot_cells_are_write_safe(current_cells.as_slice())?;
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        bail!("approved Excel write was cancelled before mutation");
    }

    let request = range_write_bridge_request(&target, &input, current_cells.as_slice());
    let response = write_platform_range(&request)?;
    normalize_range_write_response(response, &target, &input, current_cells.as_slice())?;

    let after = read_platform_snapshot()?;
    let normalized_after = normalize_snapshot(after.clone())?;
    let refreshed = resolve_range_read_target(
        &after,
        &normalized_after,
        target.workbook_id.as_str(),
        target.worksheet_id.as_str(),
    )?;
    if refreshed != target {
        bail!("Excel workbook or worksheet identity changed after the verified write; inspect it again");
    }

    let final_response = read_platform_range(&range_read_bridge_request(&target, &input.range))?;
    let final_cells = normalize_range_read_response(final_response, &target, &input.range)?;
    if !desired_cells_match(input.cells.as_slice(), final_cells.as_slice())?
        || !same_number_formats(current_cells.as_slice(), final_cells.as_slice())?
    {
        bail!("Excel range changed after the bridge verified the write; inspect the range before any retry");
    }
    range_write_response(
        &target,
        &input.range,
        final_cells,
        action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)),
    )
}

pub(super) fn execute_range_format_write(
    arguments: &Value,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        bail!("approved Excel number format write was cancelled before execution");
    }
    let _write_guard = EXCEL_WRITE_LOCK
        .lock()
        .map_err(|_| anyhow!("Excel live write lock is unavailable"))?;
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        bail!("approved Excel number format write was cancelled while waiting for another write");
    }
    let input = parse_range_format_input(arguments)?;
    let before = read_platform_snapshot()?;
    let normalized_before = normalize_snapshot(before.clone())?;
    if !normalized_before
        .get("running")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(excel_not_running_error(&normalized_before));
    }
    let target = resolve_range_read_target(
        &before,
        &normalized_before,
        input.workbook_id.as_str(),
        input.worksheet_id.as_str(),
    )?;
    ensure_write_target_is_mutable(&target)?;

    let current = read_platform_range(&range_read_bridge_request(&target, &input.range))?;
    let current_cells = normalize_range_read_response(current, &target, &input.range)?;
    let current_snapshot_id = range_snapshot_id(&target, &input.range, current_cells.as_slice())?;
    if current_snapshot_id != input.expected_snapshot_id {
        bail!(
            "Excel range changed after it was read; read the exact range again before formatting"
        );
    }
    ensure_snapshot_cells_are_format_safe(current_cells.as_slice())?;
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        bail!("approved Excel number format write was cancelled before mutation");
    }

    let request = range_format_bridge_request(&target, &input, current_cells.as_slice());
    let response = write_platform_range(&request)?;
    normalize_range_format_response(response, &target, &input, current_cells.as_slice())?;

    let after = read_platform_snapshot()?;
    let normalized_after = normalize_snapshot(after.clone())?;
    let refreshed = resolve_range_read_target(
        &after,
        &normalized_after,
        target.workbook_id.as_str(),
        target.worksheet_id.as_str(),
    )?;
    if refreshed != target {
        bail!("Excel workbook or worksheet identity changed after the verified number format write; inspect it again");
    }

    let final_response = read_platform_range(&range_read_bridge_request(&target, &input.range))?;
    let final_cells = normalize_range_read_response(final_response, &target, &input.range)?;
    if !formatted_cells_match(
        current_cells.as_slice(),
        final_cells.as_slice(),
        input.number_format.as_str(),
    )? {
        bail!("Excel range changed after the bridge verified the number format; inspect the range before any retry");
    }
    range_format_response(
        &target,
        &input,
        final_cells,
        action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)),
    )
}
