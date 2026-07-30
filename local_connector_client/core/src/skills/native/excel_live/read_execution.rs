// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use super::platform_bridge::{read_platform_range, read_platform_snapshot, read_platform_status};
use super::range_reference::parse_a1_range;
use super::range_response::{normalize_range_read_response, range_read_response};
use super::range_target::{range_read_bridge_request, resolve_range_read_target};
use super::requires_interactive_approval;
use super::snapshot_identity::{normalize_snapshot, status_response, workbook_list_projection};
use super::validation::{ensure_exact_arguments, required_text};

pub(super) fn execute(operation: &str, arguments: &Value) -> Result<Value> {
    if requires_interactive_approval(operation) {
        bail!(
            "Excel live range mutations require the signed Plugin runtime and interactive approval"
        );
    }
    if operation == "excel_read_range" {
        return execute_range_read(arguments);
    }
    let snapshot = if operation == "excel_live_status" {
        read_platform_status()?
    } else {
        read_platform_snapshot()?
    };
    execute_with_snapshot(operation, arguments, snapshot)
}

fn execute_range_read(arguments: &Value) -> Result<Value> {
    ensure_exact_arguments(arguments, &["workbook_id", "worksheet_id", "range"])?;
    let workbook_id = required_text(arguments, "workbook_id", 96)?;
    let worksheet_id = required_text(arguments, "worksheet_id", 96)?;
    let range = parse_a1_range(required_text(arguments, "range", 32)?)?;
    let before = read_platform_snapshot()?;
    let normalized_before = normalize_snapshot(before.clone())?;
    if !normalized_before
        .get("running")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(excel_not_running_error(&normalized_before));
    }
    let target = resolve_range_read_target(&before, &normalized_before, workbook_id, worksheet_id)?;
    let request = range_read_bridge_request(&target, &range);
    let response = read_platform_range(&request)?;
    let cells = normalize_range_read_response(response, &target, &range)?;

    let after = read_platform_snapshot()?;
    let normalized_after = normalize_snapshot(after.clone())?;
    let refreshed = resolve_range_read_target(
        &after,
        &normalized_after,
        target.workbook_id.as_str(),
        target.worksheet_id.as_str(),
    )?;
    if refreshed != target {
        bail!("Excel workbook or worksheet identity changed during the bounded read; inspect it again");
    }

    range_read_response(&target, &range, cells)
}

pub(super) fn execute_with_snapshot(
    operation: &str,
    arguments: &Value,
    snapshot: Value,
) -> Result<Value> {
    let normalized = normalize_snapshot(snapshot)?;
    match operation {
        "excel_live_status" => {
            ensure_exact_arguments(arguments, &[])?;
            status_response(&normalized)
        }
        "excel_list_open_workbooks" => {
            ensure_exact_arguments(arguments, &[])?;
            if !normalized
                .get("running")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return Err(excel_not_running_error(&normalized));
            }
            let workbooks = normalized
                .get("workbooks")
                .and_then(Value::as_array)
                .context("normalized Excel workbooks are missing")?;
            Ok(json!({
                "platform": normalized.get("platform"),
                "excel_installed": normalized.get("installed"),
                "excel_running": true,
                "read_only": true,
                "safe_no_launch": true,
                "application_version": normalized.get("application_version"),
                "workbook_count": normalized.get("workbooks_total"),
                "workbooks_truncated": normalized.get("workbooks_truncated"),
                "workbooks": workbooks.iter().map(workbook_list_projection).collect::<Vec<_>>(),
            }))
        }
        "excel_inspect_workbook" => {
            ensure_exact_arguments(arguments, &["workbook_id"])?;
            let workbook_id = required_text(arguments, "workbook_id", 96)?;
            if !normalized
                .get("running")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return Err(excel_not_running_error(&normalized));
            }
            let workbook = normalized
                .get("workbooks")
                .and_then(Value::as_array)
                .and_then(|workbooks| {
                    workbooks.iter().find(|workbook| {
                        workbook.get("workbook_id").and_then(Value::as_str) == Some(workbook_id)
                    })
                })
                .ok_or_else(|| {
                    anyhow!(
                        "Excel workbook identity is missing or stale; list open workbooks again"
                    )
                })?;
            Ok(json!({
                "platform": normalized.get("platform"),
                "excel_running": true,
                "read_only": true,
                "safe_no_launch": true,
                "workbook": workbook,
            }))
        }
        _ => Err(anyhow!(
            "Excel Live Control operation is not implemented: {operation}"
        )),
    }
}

pub(super) fn excel_not_running_error(snapshot: &Value) -> anyhow::Error {
    if snapshot.get("installed").and_then(Value::as_bool) == Some(false) {
        anyhow!("Microsoft Excel desktop is not installed")
    } else {
        anyhow!(
            "Microsoft Excel is not running; Excel Live Control never launches it automatically"
        )
    }
}
