// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn write_safety_rejects_read_only_hidden_protected_and_ambiguous_snapshots() {
    let raw = sample_snapshot();
    let normalized = normalize_snapshot(raw.clone()).expect("normalized snapshot");
    let workbook_id = normalized
        .pointer("/workbooks/0/workbook_id")
        .and_then(Value::as_str)
        .expect("workbook ID");
    let visible_id = normalized
        .pointer("/workbooks/0/sheets/0/worksheet_id")
        .and_then(Value::as_str)
        .expect("visible worksheet ID");
    let protected_id = normalized
        .pointer("/workbooks/0/sheets/1/worksheet_id")
        .and_then(Value::as_str)
        .expect("protected worksheet ID");
    let visible = resolve_range_read_target(&raw, &normalized, workbook_id, visible_id)
        .expect("visible target");
    let protected = resolve_range_read_target(&raw, &normalized, workbook_id, protected_id)
        .expect("protected target");
    assert!(ensure_write_target_is_mutable(&visible).is_ok());
    assert!(ensure_write_target_is_mutable(&protected).is_err());

    let mut read_only_raw = sample_snapshot();
    read_only_raw["workbooks"][0]["read_only"] = json!(true);
    let read_only_normalized =
        normalize_snapshot(read_only_raw.clone()).expect("read-only normalized snapshot");
    let read_only_target = resolve_range_read_target(
        &read_only_raw,
        &read_only_normalized,
        read_only_normalized
            .pointer("/workbooks/0/workbook_id")
            .and_then(Value::as_str)
            .expect("read-only workbook ID"),
        read_only_normalized
            .pointer("/workbooks/0/sheets/0/worksheet_id")
            .and_then(Value::as_str)
            .expect("read-only worksheet ID"),
    )
    .expect("read-only target");
    assert!(ensure_write_target_is_mutable(&read_only_target).is_err());

    let mut hidden_raw = sample_snapshot();
    hidden_raw["workbooks"][0]["sheets"][1]["protected"] = json!(false);
    let hidden_normalized =
        normalize_snapshot(hidden_raw.clone()).expect("hidden normalized snapshot");
    let hidden_target = resolve_range_read_target(
        &hidden_raw,
        &hidden_normalized,
        hidden_normalized
            .pointer("/workbooks/0/workbook_id")
            .and_then(Value::as_str)
            .expect("hidden workbook ID"),
        hidden_normalized
            .pointer("/workbooks/0/sheets/1/worksheet_id")
            .and_then(Value::as_str)
            .expect("hidden worksheet ID"),
    )
    .expect("hidden target");
    assert!(ensure_write_target_is_mutable(&hidden_target).is_err());

    let range = parse_a1_range("A1:B1").expect("range");
    let mut response =
        sample_range_bridge_response(&visible, &range, json!(42.5), json!(85.0), "=A1*2");
    response["cells"][0]["displayed_text_truncated"] = json!(true);
    let cells = normalize_range_read_response(response, &visible, &range)
        .expect("normalized ambiguous cells");
    assert!(ensure_snapshot_cells_are_write_safe(cells.as_slice()).is_err());

    let mut unsafe_format =
        sample_range_bridge_response(&visible, &range, json!(42.5), json!(85.0), "=A1*2");
    unsafe_format["cells"][0]["number_format_truncated"] = json!(true);
    let cells = normalize_range_read_response(unsafe_format, &visible, &range)
        .expect("normalized unsafe format cells");
    assert!(ensure_snapshot_cells_are_format_safe(cells.as_slice()).is_err());
}

#[test]
fn range_response_rejects_exposed_external_or_hidden_formulas() {
    assert!(formula_contains_external_reference(
        "='C:\\secret\\[Budget.xlsx]Sheet1'!A1"
    ));
    assert!(formula_contains_external_reference(
        "='https://example.test/[Budget.xlsx]Sheet1'!A1"
    ));
    assert!(formula_contains_external_reference("='[Book1]Sheet1'!A1"));
    assert!(!formula_contains_external_reference("=SUM(Table1[Amount])"));

    let raw = sample_snapshot();
    let normalized = normalize_snapshot(raw.clone()).expect("normalized snapshot");
    let workbook_id = normalized
        .pointer("/workbooks/0/workbook_id")
        .and_then(Value::as_str)
        .expect("workbook ID");
    let worksheet_id = normalized
        .pointer("/workbooks/0/sheets/0/worksheet_id")
        .and_then(Value::as_str)
        .expect("worksheet ID");
    let target = resolve_range_read_target(&raw, &normalized, workbook_id, worksheet_id)
        .expect("exact range target");
    let range = parse_a1_range("A1").expect("range");
    let exposed = json!({
        "schema_version": 1,
        "runtime_instance": target.runtime_instance,
        "workbook_index": target.workbook_index,
        "workbook_name": target.workbook_name,
        "worksheet_index": target.worksheet_index,
        "worksheet_name": target.worksheet_name,
        "range_address": "A1",
        "start_row": 1,
        "start_column": 1,
        "row_count": 1,
        "column_count": 1,
        "cell_count": 1,
        "cells": [{
            "row_offset": 0,
            "column_offset": 0,
            "value": 1,
            "value_truncated": false,
            "displayed_text": "1",
            "displayed_text_truncated": false,
            "has_formula": true,
            "formula": "='C:\\secret\\[Budget.xlsx]Sheet1'!A1",
            "formula_truncated": false,
            "formula_hidden": false,
            "formula_external_reference": false,
            "number_format": "General",
            "number_format_truncated": false,
            "number_format_unavailable": false,
            "is_error": false
        }]
    });
    assert!(normalize_range_read_response(exposed, &target, &range).is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn embedded_excel_jxa_bridges_compile_without_launching_excel() {
    let temp = tempfile::tempdir().expect("temporary Excel script directory");
    for (index, script) in [
        MACOS_STATUS_SCRIPT.to_string(),
        MACOS_SNAPSHOT_SCRIPT.to_string(),
        macos_range_read_script(),
        macos_range_write_script(),
    ]
    .into_iter()
    .enumerate()
    {
        let output_path = temp.path().join(format!("excel-bridge-{index}.scpt"));
        let output = Command::new("/usr/bin/osacompile")
            .args(["-l", "JavaScript", "-e", script.as_str(), "-o"])
            .arg(output_path.as_os_str())
            .output()
            .expect("compile embedded Excel JXA bridge");
        assert!(
            output.status.success(),
            "Excel JXA compilation failed: {}",
            String::from_utf8_lossy(output.stderr.as_slice())
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires the local macOS Excel installation; the probe never launches Excel"]
fn macos_status_probe_uses_the_real_no_launch_bridge() {
    let status = execute("excel_live_status", &json!({})).expect("live Excel status");
    assert_eq!(
        status.get("safe_no_launch").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        status.get("cell_content_access").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        status.get("write_access").and_then(Value::as_bool),
        Some(true)
    );
    if status.get("excel_installed").and_then(Value::as_bool) == Some(true)
        && status.get("excel_running").and_then(Value::as_bool) == Some(false)
    {
        let error = execute("excel_list_open_workbooks", &json!({}))
            .expect_err("stopped Excel cannot list workbooks");
        assert!(error.to_string().contains("never launches"));
    }
}
