// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn publishes_bounded_read_only_no_launch_tools() {
    let macos_range_read = macos_range_read_script();
    let windows_range_read = windows_range_read_script();
    let macos_range_write = macos_range_write_script();
    let windows_range_write = windows_range_write_script();
    let tools = tool_definitions(false);
    assert_eq!(tools.len(), 4);
    let names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "excel_live_status",
            "excel_list_open_workbooks",
            "excel_inspect_workbook",
            "excel_read_range"
        ]
    );
    assert!(!MACOS_SNAPSHOT_SCRIPT.contains(".activate("));
    assert!(!MACOS_SNAPSHOT_SCRIPT.contains(".open("));
    assert!(!MACOS_STATUS_SCRIPT.contains(".activate("));
    assert!(!MACOS_STATUS_SCRIPT.contains(".open("));
    assert!(!WINDOWS_SNAPSHOT_SCRIPT.contains("Workbooks.Open"));
    assert!(!WINDOWS_STATUS_SCRIPT.contains("Workbooks.Open"));
    assert!(!macos_range_read.contains(".activate("));
    assert!(!macos_range_read.contains(".open("));
    assert!(!macos_range_read.contains(".save("));
    assert!(!windows_range_read.contains("Workbooks.Open"));
    assert!(!windows_range_read.contains(".Activate("));
    assert!(!windows_range_read.contains(".Save("));
    assert!(macos_range_read.contains("fileHandleWithStandardInput"));
    assert!(windows_range_read.contains("[Console]::In.ReadToEnd()"));
    assert!(WINDOWS_SNAPSHOT_SCRIPT.contains("GetActiveObject"));
    assert!(windows_range_read.contains("GetActiveObject"));

    let approved_tools = tool_definitions(true);
    assert_eq!(approved_tools.len(), 6);
    assert_eq!(
        approved_tools
            .get(4)
            .and_then(|tool| tool.get("name"))
            .and_then(Value::as_str),
        Some("excel_write_range")
    );
    assert_eq!(
        approved_tools
            .last()
            .and_then(|tool| tool.get("name"))
            .and_then(Value::as_str),
        Some("excel_set_number_format")
    );
    assert!(requires_interactive_approval("excel_write_range"));
    assert!(requires_interactive_approval("excel_set_number_format"));
    assert!(!requires_interactive_approval("excel_read_range"));
    assert!(execute("excel_write_range", &json!({}))
        .expect_err("direct Excel write execution must fail before platform access")
        .to_string()
        .contains("interactive approval"));
    assert!(execute("excel_set_number_format", &json!({}))
        .expect_err("direct Excel format execution must fail before platform access")
        .to_string()
        .contains("interactive approval"));
    assert!(!macos_range_write.contains(".activate("));
    assert!(!macos_range_write.contains(".open("));
    assert!(!macos_range_write.contains(".save("));
    assert!(!macos_range_write.contains(".select("));
    assert!(!macos_range_write.contains(".calculate("));
    assert!(!windows_range_write.contains("Workbooks.Open"));
    assert!(!windows_range_write.contains(".Activate("));
    assert!(!windows_range_write.contains(".Save("));
    assert!(!windows_range_write.contains(".Select("));
    assert!(!windows_range_write.contains(".Calculate("));
    assert!(macos_range_write.contains("fileHandleWithStandardInput"));
    assert!(windows_range_write.contains("[Console]::In.ReadToEnd()"));
    assert!(windows_range_write.contains("GetActiveObject"));
}

#[test]
fn workbook_identity_hides_paths_and_binds_the_running_instance() {
    let normalized = normalize_snapshot(sample_snapshot()).expect("normalized snapshot");
    let serialized = serde_json::to_string(&normalized).expect("serialize normalized snapshot");
    assert!(!serialized.contains("/private/secret"));
    let workbook = normalized
        .pointer("/workbooks/0")
        .expect("normalized workbook");
    let workbook_id = workbook
        .get("workbook_id")
        .and_then(Value::as_str)
        .expect("workbook identity");
    assert!(workbook_id.starts_with("excel_wb_"));

    let mut restarted = sample_snapshot();
    restarted["runtime_instance"] = json!("4243");
    let restarted_id = normalize_snapshot(restarted)
        .expect("restarted snapshot")
        .pointer("/workbooks/0/workbook_id")
        .and_then(Value::as_str)
        .expect("restarted workbook identity")
        .to_string();
    assert_ne!(workbook_id, restarted_id);
}

#[test]
fn exact_workbook_inspection_rejects_stale_identity() {
    let normalized = normalize_snapshot(sample_snapshot()).expect("normalized snapshot");
    let workbook_id = normalized
        .pointer("/workbooks/0/workbook_id")
        .and_then(Value::as_str)
        .expect("workbook identity")
        .to_string();
    let inspected = execute_with_snapshot(
        "excel_inspect_workbook",
        &json!({"workbook_id": workbook_id}),
        sample_snapshot(),
    )
    .expect("inspect workbook");
    assert_eq!(
        inspected.pointer("/workbook/name").and_then(Value::as_str),
        Some("Budget.xlsx")
    );
    assert_eq!(
        inspected
            .pointer("/workbook/sheets/1/protected")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert!(inspected
        .pointer("/workbook/sheets/0/worksheet_id")
        .and_then(Value::as_str)
        .is_some_and(|value| value.starts_with("excel_ws_")));

    let error = execute_with_snapshot(
        "excel_inspect_workbook",
        &json!({"workbook_id": "excel_wb_stale"}),
        sample_snapshot(),
    )
    .expect_err("stale identity must fail");
    assert!(error.to_string().contains("missing or stale"));
}

#[test]
fn stopped_excel_status_is_safe_and_other_operations_fail_closed() {
    let stopped = json!({
        "schema_version": 1,
        "installed": true,
        "running": false,
        "runtime_instance": null,
        "application_version": null,
        "workbooks_total": 0,
        "workbooks_truncated": false,
        "workbooks": []
    });
    let status = execute_with_snapshot("excel_live_status", &json!({}), stopped.clone())
        .expect("stopped Excel status");
    assert_eq!(
        status.get("status").and_then(Value::as_str),
        Some("excel_not_running")
    );
    assert_eq!(
        status.get("safe_no_launch").and_then(Value::as_bool),
        Some(true)
    );
    assert!(
        execute_with_snapshot("excel_list_open_workbooks", &json!({}), stopped)
            .expect_err("list requires running Excel")
            .to_string()
            .contains("never launches")
    );
}

#[test]
fn status_can_report_counts_without_collecting_workbook_names() {
    let status_only = json!({
        "schema_version": 1,
        "installed": true,
        "running": true,
        "runtime_instance": "4242",
        "application_version": "16.99",
        "workbooks_total": 3,
        "workbooks_truncated": false,
        "workbook_metadata_omitted": true,
        "workbooks": []
    });
    let status = execute_with_snapshot("excel_live_status", &json!({}), status_only)
        .expect("status-only Excel snapshot");
    assert_eq!(
        status.get("open_workbook_count").and_then(Value::as_u64),
        Some(3)
    );
    assert!(!serde_json::to_string(&status)
        .expect("status JSON")
        .contains("Budget.xlsx"));
}

#[test]
fn malformed_or_ambiguous_snapshots_fail_closed() {
    let mut duplicate_active = sample_snapshot();
    let second = duplicate_active["workbooks"][0].clone();
    duplicate_active["workbooks_total"] = json!(2);
    duplicate_active["workbooks"]
        .as_array_mut()
        .expect("workbooks")
        .push(second);
    assert!(normalize_snapshot(duplicate_active).is_err());

    let mut leaked_control = sample_snapshot();
    leaked_control["workbooks"][0]["name"] = json!("Budget\n.xlsx");
    assert!(normalize_snapshot(leaked_control).is_err());

    let mut missing_sheet = sample_snapshot();
    missing_sheet["workbooks"][0]["sheets"]
        .as_array_mut()
        .expect("sheets")
        .pop();
    assert!(normalize_snapshot(missing_sheet).is_err());
}

#[test]
fn canonical_a1_ranges_are_bounded_to_the_excel_grid_and_256_cells() {
    let range = parse_a1_range("A1:P16").expect("256-cell range");
    assert_eq!(range.cell_count, 256);
    assert_eq!(range.start_column, 1);
    assert_eq!(range.end_column, 16);
    assert_eq!(excel_column_name(MAX_EXCEL_COLUMNS), "XFD");

    for invalid in [
        "a1",
        "$A$1",
        "Sheet1!A1",
        "A0",
        "XFE1",
        "A1048577",
        "B2:A1",
        "A1:A257",
        "A1,B2",
        "A1:B2:C3",
    ] {
        assert!(parse_a1_range(invalid).is_err(), "must reject {invalid}");
    }
}

#[test]
fn write_inputs_are_exact_typed_bounded_and_formula_allowlisted() {
    let snapshot_id = format!("excel_range_{}", "a".repeat(64));
    let arguments = json!({
        "workbook_id": "excel_wb_current",
        "worksheet_id": "excel_ws_current",
        "range": "A1:B2",
        "expected_snapshot_id": snapshot_id,
        "cells": [
            [{"kind":"blank"},{"kind":"value","value":42.5}],
            [{"kind":"value","value":"Quarter 1"},{"kind":"formula","formula":"=SUM(A1:A2)"}]
        ]
    });
    let parsed = parse_range_write_input(&arguments).expect("safe write input");
    assert_eq!(parsed.range.cell_count, 4);
    assert_eq!(parsed.cells.len(), 4);
    assert_eq!(
        validate_live_formula("=ROUND(SUM(A1:A2),2)").expect("safe formula"),
        "=ROUND(SUM(A1:A2),2)"
    );
    for formula in [
        "=WEBSERVICE(A1)",
        "=RTD(A1)",
        "='[Book.xlsx]Sheet1'!A1",
        "=HYPERLINK(A1)",
        "=SUM(Table1[Amount])",
        "=\"secret\"",
    ] {
        assert!(validate_live_formula(formula).is_err(), "reject {formula}");
    }

    let mut dangerous_text = arguments.clone();
    dangerous_text["cells"][1][0] = json!({"kind":"value","value":"=CMD()"});
    assert!(parse_range_write_input(&dangerous_text).is_err());

    let mut wrong_shape = arguments.clone();
    wrong_shape["cells"][1] = json!([{"kind":"blank"}]);
    assert!(parse_range_write_input(&wrong_shape).is_err());

    let mut bad_snapshot = arguments.clone();
    bad_snapshot["expected_snapshot_id"] = json!("excel_range_stale");
    assert!(parse_range_write_input(&bad_snapshot).is_err());
}

#[test]
fn write_approval_arguments_bind_content_without_exposing_cell_text() {
    let arguments = json!({
        "workbook_id": "excel_wb_current",
        "worksheet_id": "excel_ws_current",
        "range": "A1:B1",
        "expected_snapshot_id": format!("excel_range_{}", "b".repeat(64)),
        "cells": [[
            {"kind":"value","value":"private budget note"},
            {"kind":"formula","formula":"=A1"}
        ]]
    });
    let (command, args) =
        approval_command("excel_write_range", &arguments).expect("Excel write approval command");
    assert_eq!(command, "chatos-excel-live");
    let serialized = args.join("\n");
    assert!(serialized.contains("range=A1:B1"));
    assert!(serialized.contains("cell_count=2"));
    assert!(serialized.contains("content_sha256="));
    assert!(!serialized.contains("private budget note"));
    assert!(!serialized.contains("formula==A1"));
}

#[test]
fn number_format_inputs_and_approval_are_exact_and_allowlisted() {
    let arguments = json!({
        "workbook_id": "excel_wb_current",
        "worksheet_id": "excel_ws_current",
        "range": "A1:B2",
        "expected_snapshot_id": format!("excel_range_{}", "c".repeat(64)),
        "preset": "percent_2"
    });
    let parsed = parse_range_format_input(&arguments).expect("safe number format input");
    assert_eq!(parsed.range.cell_count, 4);
    assert_eq!(parsed.number_format, "0.00%");
    assert_eq!(number_format_preset_for_code("0.00%"), Some("percent_2"));
    let (command, args) = approval_command("excel_set_number_format", &arguments)
        .expect("Excel number format approval command");
    assert_eq!(command, "chatos-excel-live");
    assert!(args.iter().any(|value| value == "set_number_format"));
    assert!(args.iter().any(|value| value == "preset=percent_2"));
    assert!(args.iter().any(|value| value == "cell_count=4"));

    let mut arbitrary = arguments.clone();
    arbitrary["preset"] = json!("$#,##0.00");
    assert!(parse_range_format_input(&arbitrary).is_err());
    let mut extra = arguments.clone();
    extra["custom_format"] = json!("secret");
    assert!(parse_range_format_input(&extra).is_err());
}

#[test]
fn range_target_uses_exact_opaque_workbook_and_worksheet_identities() {
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
    assert_eq!(target.workbook_name, "Budget.xlsx");
    assert_eq!(target.worksheet_name, "Summary");
    assert_eq!(
        target.workbook_identity_source,
        "/private/secret/Budget.xlsx"
    );

    assert!(resolve_range_read_target(&raw, &normalized, "excel_wb_stale", worksheet_id).is_err());
    assert!(resolve_range_read_target(&raw, &normalized, workbook_id, "excel_ws_stale").is_err());
}
