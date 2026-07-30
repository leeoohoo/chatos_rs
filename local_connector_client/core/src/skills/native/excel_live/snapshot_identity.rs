// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{
    optional_bounded_text, required_bool, required_bounded_text, required_usize,
    MAX_IDENTITY_SOURCE_CHARACTERS, MAX_OPEN_WORKBOOKS, MAX_RANGE_CELLS,
    MAX_WORKBOOK_NAME_CHARACTERS, MAX_WORKSHEETS_PER_WORKBOOK, MAX_WORKSHEET_NAME_CHARACTERS,
};

pub(super) fn normalize_snapshot(snapshot: Value) -> Result<Value> {
    let object = snapshot
        .as_object()
        .context("Excel automation response must be an object")?;
    if object.get("schema_version").and_then(Value::as_u64) != Some(1) {
        bail!("Excel automation response has an unsupported schema version");
    }
    let installed = required_bool(object, "installed")?;
    let running = required_bool(object, "running")?;
    if running && !installed {
        bail!("Excel automation response cannot be running when Excel is not installed");
    }
    let runtime_instance =
        optional_bounded_text(object.get("runtime_instance"), "runtime_instance", 128)?;
    if running && runtime_instance.is_none() {
        bail!("Excel automation response is missing the running instance identity");
    }
    let application_version = optional_bounded_text(
        object.get("application_version"),
        "application_version",
        128,
    )?;
    let workbooks_total = required_usize(object, "workbooks_total", 10_000)?;
    let workbooks_truncated = required_bool(object, "workbooks_truncated")?;
    let workbook_metadata_omitted = object
        .get("workbook_metadata_omitted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let workbooks = object
        .get("workbooks")
        .and_then(Value::as_array)
        .context("Excel automation workbooks must be an array")?;
    if workbooks.len() > MAX_OPEN_WORKBOOKS {
        bail!("Excel automation response exceeds the open workbook limit");
    }
    if !running && (!workbooks.is_empty() || workbooks_total != 0) {
        bail!("stopped Excel automation response cannot contain workbooks");
    }
    if workbook_metadata_omitted && !workbooks.is_empty() {
        bail!("Excel status response cannot include omitted workbook metadata");
    }
    if workbooks_truncated != (workbooks_total > MAX_OPEN_WORKBOOKS) {
        bail!("Excel automation workbook truncation metadata is inconsistent");
    }
    if !workbook_metadata_omitted && workbooks.len() != workbooks_total.min(MAX_OPEN_WORKBOOKS) {
        bail!("Excel automation workbook count does not match its bounded metadata");
    }

    let platform = std::env::consts::OS;
    let mut normalized_workbooks = Vec::with_capacity(workbooks.len());
    let mut workbook_ids = BTreeSet::new();
    let mut active_workbooks = 0usize;
    for (position, workbook) in workbooks.iter().enumerate() {
        let workbook = normalize_workbook(workbook, runtime_instance.as_deref().unwrap_or(""))?;
        if workbook.get("index").and_then(Value::as_u64) != Some((position + 1) as u64) {
            bail!("Excel automation workbook indices are not exact and sequential");
        }
        let workbook_id = workbook
            .get("workbook_id")
            .and_then(Value::as_str)
            .context("normalized Excel workbook identity is missing")?;
        if !workbook_ids.insert(workbook_id.to_string()) {
            bail!("Excel automation response contains duplicate workbook identities");
        }
        if workbook.get("active").and_then(Value::as_bool) == Some(true) {
            active_workbooks += 1;
        }
        normalized_workbooks.push(workbook);
    }
    if active_workbooks > 1 {
        bail!("Excel automation response contains more than one active workbook");
    }

    Ok(json!({
        "platform": platform,
        "installed": installed,
        "running": running,
        "application_version": application_version,
        "workbooks_total": workbooks_total,
        "workbooks_truncated": workbooks_truncated,
        "workbooks": normalized_workbooks,
    }))
}

fn normalize_workbook(workbook: &Value, runtime_instance: &str) -> Result<Value> {
    let object = workbook
        .as_object()
        .context("Excel workbook metadata must be an object")?;
    let index = required_usize(object, "index", MAX_OPEN_WORKBOOKS)?;
    if index == 0 {
        bail!("Excel workbook index must be one-based");
    }
    let name = required_bounded_text(object, "name", MAX_WORKBOOK_NAME_CHARACTERS)?;
    let identity_source =
        required_bounded_text(object, "identity_source", MAX_IDENTITY_SOURCE_CHARACTERS)?;
    let saved = required_bool(object, "saved")?;
    let read_only = required_bool(object, "read_only")?;
    let active = required_bool(object, "active")?;
    let sheet_count = required_usize(object, "sheet_count", 100_000)?;
    let sheets_truncated = required_bool(object, "sheets_truncated")?;
    let sheets = object
        .get("sheets")
        .and_then(Value::as_array)
        .context("Excel workbook sheets must be an array")?;
    if sheets.len() > MAX_WORKSHEETS_PER_WORKBOOK {
        bail!("Excel workbook exceeds the worksheet metadata limit");
    }
    if sheets_truncated != (sheet_count > MAX_WORKSHEETS_PER_WORKBOOK) {
        bail!("Excel worksheet truncation metadata is inconsistent");
    }
    if sheets.len() != sheet_count.min(MAX_WORKSHEETS_PER_WORKBOOK) {
        bail!("Excel worksheet count does not match its bounded metadata");
    }
    let workbook_id = workbook_identity(runtime_instance, index, name, identity_source);
    let mut normalized_sheets = Vec::with_capacity(sheets.len());
    let mut sheet_names = BTreeSet::new();
    let mut active_sheets = 0usize;
    for (position, sheet) in sheets.iter().enumerate() {
        let sheet = normalize_sheet(sheet, workbook_id.as_str())?;
        if sheet.get("index").and_then(Value::as_u64) != Some((position + 1) as u64) {
            bail!("Excel worksheet indices are not exact and sequential");
        }
        let sheet_name = sheet
            .get("name")
            .and_then(Value::as_str)
            .context("normalized Excel worksheet name is missing")?;
        if !sheet_names.insert(sheet_name.to_lowercase()) {
            bail!("Excel workbook contains duplicate worksheet names");
        }
        if sheet.get("active").and_then(Value::as_bool) == Some(true) {
            active_sheets += 1;
        }
        normalized_sheets.push(sheet);
    }
    if active_sheets > 1 || (!active && active_sheets != 0) {
        bail!("Excel workbook active worksheet metadata is inconsistent");
    }
    Ok(json!({
        "workbook_id": workbook_id,
        "name": name,
        "index": index,
        "saved": saved,
        "read_only": read_only,
        "active": active,
        "sheet_count": sheet_count,
        "sheets_truncated": sheets_truncated,
        "sheets": normalized_sheets,
    }))
}

fn normalize_sheet(sheet: &Value, workbook_id: &str) -> Result<Value> {
    let object = sheet
        .as_object()
        .context("Excel worksheet metadata must be an object")?;
    let index = required_usize(object, "index", MAX_WORKSHEETS_PER_WORKBOOK)?;
    if index == 0 {
        bail!("Excel worksheet index must be one-based");
    }
    let name = required_bounded_text(object, "name", MAX_WORKSHEET_NAME_CHARACTERS)?;
    let visible = required_bounded_text(object, "visible", 32)?;
    if !matches!(visible, "visible" | "hidden" | "very_hidden" | "unknown") {
        bail!("Excel worksheet visibility is unsupported");
    }
    let worksheet_id = worksheet_identity(workbook_id, index, name);
    Ok(json!({
        "worksheet_id": worksheet_id,
        "index": index,
        "name": name,
        "visible": visible,
        "protected": required_bool(object, "protected")?,
        "active": required_bool(object, "active")?,
    }))
}

pub(super) fn status_response(snapshot: &Value) -> Result<Value> {
    let object = snapshot
        .as_object()
        .context("normalized Excel snapshot must be an object")?;
    let installed = required_bool(object, "installed")?;
    let running = required_bool(object, "running")?;
    let status = if !installed {
        "excel_not_installed"
    } else if !running {
        "excel_not_running"
    } else {
        "ready"
    };
    Ok(json!({
        "platform": snapshot.get("platform"),
        "status": status,
        "excel_installed": installed,
        "excel_running": running,
        "application_version": snapshot.get("application_version"),
        "open_workbook_count": snapshot.get("workbooks_total"),
        "workbooks_truncated": snapshot.get("workbooks_truncated"),
        "read_only": false,
        "discovery_read_only": true,
        "safe_no_launch": true,
        "cell_content_access": true,
        "max_range_cells": MAX_RANGE_CELLS,
        "write_access": true,
        "write_requires_interactive_approval": true,
        "number_format_write_access": true,
        "number_format_presets": ["general", "integer", "decimal_2", "percent_2", "date", "datetime", "text"],
        "write_saves_workbook": false,
    }))
}

pub(super) fn workbook_list_projection(workbook: &Value) -> Value {
    json!({
        "workbook_id": workbook.get("workbook_id"),
        "name": workbook.get("name"),
        "index": workbook.get("index"),
        "saved": workbook.get("saved"),
        "read_only": workbook.get("read_only"),
        "active": workbook.get("active"),
        "sheet_count": workbook.get("sheet_count"),
        "sheets_truncated": workbook.get("sheets_truncated"),
    })
}

pub(super) fn workbook_identity(
    runtime_instance: &str,
    index: usize,
    name: &str,
    identity_source: &str,
) -> String {
    let mut hasher = Sha256::new();
    let index = index.to_string();
    for value in [
        "chatos-excel-workbook-v1",
        std::env::consts::OS,
        runtime_instance,
        index.as_str(),
        name,
        identity_source,
    ] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    format!("excel_wb_{}", hex::encode(hasher.finalize()))
}

pub(super) fn worksheet_identity(workbook_id: &str, index: usize, name: &str) -> String {
    let mut hasher = Sha256::new();
    let index = index.to_string();
    for value in [
        "chatos-excel-worksheet-v1",
        std::env::consts::OS,
        workbook_id,
        index.as_str(),
        name,
    ] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    format!("excel_ws_{}", hex::encode(hasher.finalize()))
}
