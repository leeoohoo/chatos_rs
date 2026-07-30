// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{Map, Value};

use super::mutation_input::{validate_live_formula, validate_safe_live_text};

#[derive(Clone, Copy)]
enum SnapshotMutation {
    Content,
    NumberFormat,
}

fn exact_restorable_cell(
    cell: &Value,
    mutation: SnapshotMutation,
) -> Result<(&Map<String, Value>, &str)> {
    let object = cell
        .as_object()
        .context("normalized Excel range cell must be an object")?;
    let address = object
        .get("address")
        .and_then(Value::as_str)
        .context("normalized Excel range cell address is missing")?;
    let operation = match mutation {
        SnapshotMutation::Content => "replaced",
        SnapshotMutation::NumberFormat => "formatted",
    };
    if object
        .get("value_truncated")
        .and_then(Value::as_bool)
        .unwrap_or(true)
        || object
            .get("displayed_text_truncated")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        || object
            .get("formula_truncated")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        || object
            .get("number_format_truncated")
            .and_then(Value::as_bool)
            .unwrap_or(true)
    {
        bail!("Excel cell {address} has truncated state and cannot be safely {operation}");
    }
    if object
        .get("formula_hidden")
        .and_then(Value::as_bool)
        .unwrap_or(true)
        || object
            .get("formula_external_reference")
            .and_then(Value::as_bool)
            .unwrap_or(true)
    {
        bail!("Excel cell {address} has a hidden or external formula and cannot be safely {operation}");
    }
    if object
        .get("number_format_unavailable")
        .and_then(Value::as_bool)
        .unwrap_or(true)
        || object
            .get("number_format")
            .and_then(Value::as_str)
            .is_none()
    {
        match mutation {
            SnapshotMutation::Content => bail!(
                "Excel cell {address} number format cannot be verified before content replacement"
            ),
            SnapshotMutation::NumberFormat => {
                bail!("Excel cell {address} number format cannot be read and restored exactly")
            }
        }
    }
    Ok((object, address))
}

pub(super) fn ensure_snapshot_cells_are_format_safe(cells: &[Value]) -> Result<()> {
    for cell in cells {
        exact_restorable_cell(cell, SnapshotMutation::NumberFormat)?;
    }
    Ok(())
}

pub(super) fn ensure_snapshot_cells_are_write_safe(cells: &[Value]) -> Result<()> {
    for cell in cells {
        let (object, address) = exact_restorable_cell(cell, SnapshotMutation::Content)?;
        let status = object
            .get("status")
            .and_then(Value::as_str)
            .context("normalized Excel range cell status is missing")?;
        match status {
            "blank" => {}
            "value" => {
                let value = object
                    .get("value")
                    .context("normalized Excel range cell value is missing")?;
                if value.is_null() {
                    bail!("Excel cell {address} has an unsupported non-scalar value");
                }
                if let Some(value) = value.as_str() {
                    validate_safe_live_text(value, "existing Excel cell text")?;
                }
            }
            "formula" | "error" => {
                let formula = object
                    .get("formula")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("Excel cell {address} formula cannot be restored"))?;
                validate_live_formula(formula).with_context(|| {
                    format!("Excel cell {address} uses a formula outside the rollback allowlist")
                })?;
            }
            _ => bail!("Excel cell {address} has an unsupported state"),
        }
    }
    Ok(())
}
