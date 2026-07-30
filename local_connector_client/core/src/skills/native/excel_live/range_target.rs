// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use super::snapshot_identity::{workbook_identity, worksheet_identity};
use super::{
    required_bool, required_bounded_text, required_usize, A1Range, RangeReadTarget,
    MAX_IDENTITY_SOURCE_CHARACTERS, MAX_OPEN_WORKBOOKS, MAX_WORKBOOK_NAME_CHARACTERS,
    MAX_WORKSHEETS_PER_WORKBOOK, MAX_WORKSHEET_NAME_CHARACTERS,
};

pub(super) fn ensure_write_target_is_mutable(target: &RangeReadTarget) -> Result<()> {
    if target.workbook_read_only {
        bail!("Excel workbook is read-only; live range writes are disabled");
    }
    if target.worksheet_protected {
        bail!("Excel worksheet is protected; live range writes are disabled");
    }
    if target.worksheet_visibility != "visible" {
        bail!("Excel live range writes require an exact visible worksheet");
    }
    Ok(())
}

pub(super) fn resolve_range_read_target(
    raw_snapshot: &Value,
    normalized: &Value,
    workbook_id: &str,
    worksheet_id: &str,
) -> Result<RangeReadTarget> {
    let raw = raw_snapshot
        .as_object()
        .context("Excel automation response must be an object")?;
    let runtime_instance = required_bounded_text(raw, "runtime_instance", 128)?.to_string();
    let workbook = normalized
        .get("workbooks")
        .and_then(Value::as_array)
        .and_then(|workbooks| {
            workbooks.iter().find(|workbook| {
                workbook.get("workbook_id").and_then(Value::as_str) == Some(workbook_id)
            })
        })
        .ok_or_else(|| {
            anyhow!("Excel workbook identity is missing or stale; list open workbooks again")
        })?;
    let workbook_object = workbook
        .as_object()
        .context("normalized Excel workbook must be an object")?;
    let workbook_index = required_usize(workbook_object, "index", MAX_OPEN_WORKBOOKS)?;
    if workbook_index == 0 {
        bail!("normalized Excel workbook index must be one-based");
    }
    let workbook_name =
        required_bounded_text(workbook_object, "name", MAX_WORKBOOK_NAME_CHARACTERS)?.to_string();
    let raw_workbook = raw
        .get("workbooks")
        .and_then(Value::as_array)
        .and_then(|workbooks| workbooks.get(workbook_index - 1))
        .and_then(Value::as_object)
        .context("Excel private workbook identity is missing")?;
    if required_usize(raw_workbook, "index", MAX_OPEN_WORKBOOKS)? != workbook_index
        || required_bounded_text(raw_workbook, "name", MAX_WORKBOOK_NAME_CHARACTERS)?
            != workbook_name.as_str()
    {
        bail!("Excel private workbook identity does not match normalized metadata");
    }
    let workbook_identity_source = required_bounded_text(
        raw_workbook,
        "identity_source",
        MAX_IDENTITY_SOURCE_CHARACTERS,
    )?
    .to_string();
    if workbook_identity(
        runtime_instance.as_str(),
        workbook_index,
        workbook_name.as_str(),
        workbook_identity_source.as_str(),
    ) != workbook_id
    {
        bail!("Excel private workbook identity is stale");
    }

    let worksheet = workbook_object
        .get("sheets")
        .and_then(Value::as_array)
        .and_then(|worksheets| {
            worksheets.iter().find(|worksheet| {
                worksheet.get("worksheet_id").and_then(Value::as_str) == Some(worksheet_id)
            })
        })
        .ok_or_else(|| {
            anyhow!("Excel worksheet identity is missing or stale; inspect the workbook again")
        })?;
    let worksheet_object = worksheet
        .as_object()
        .context("normalized Excel worksheet must be an object")?;
    let worksheet_index = required_usize(worksheet_object, "index", MAX_WORKSHEETS_PER_WORKBOOK)?;
    if worksheet_index == 0 {
        bail!("normalized Excel worksheet index must be one-based");
    }
    let worksheet_name =
        required_bounded_text(worksheet_object, "name", MAX_WORKSHEET_NAME_CHARACTERS)?.to_string();
    if worksheet_identity(workbook_id, worksheet_index, worksheet_name.as_str()) != worksheet_id {
        bail!("Excel worksheet identity is stale");
    }

    Ok(RangeReadTarget {
        runtime_instance,
        workbook_id: workbook_id.to_string(),
        workbook_index,
        workbook_name,
        workbook_identity_source,
        workbook_read_only: required_bool(workbook_object, "read_only")?,
        worksheet_id: worksheet_id.to_string(),
        worksheet_index,
        worksheet_name,
        worksheet_visibility: required_bounded_text(worksheet_object, "visible", 32)?.to_string(),
        worksheet_protected: required_bool(worksheet_object, "protected")?,
    })
}

pub(super) fn range_read_bridge_request(target: &RangeReadTarget, range: &A1Range) -> Value {
    json!({
        "schema_version": 1,
        "runtime_instance": target.runtime_instance,
        "workbook_index": target.workbook_index,
        "workbook_name": target.workbook_name,
        "workbook_identity_source": target.workbook_identity_source,
        "worksheet_index": target.worksheet_index,
        "worksheet_name": target.worksheet_name,
        "range_address": range.canonical,
        "start_row": range.start_row,
        "start_column": range.start_column,
        "row_count": range.row_count,
        "column_count": range.column_count,
        "cell_count": range.cell_count,
    })
}
