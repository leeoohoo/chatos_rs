// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn range_response_is_typed_bounded_and_strips_private_identity() {
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
    let range = parse_a1_range("A1:B1").expect("range");
    let response = json!({
        "schema_version": 1,
        "runtime_instance": target.runtime_instance,
        "workbook_index": target.workbook_index,
        "workbook_name": target.workbook_name,
        "worksheet_index": target.worksheet_index,
        "worksheet_name": target.worksheet_name,
        "range_address": "A1:B1",
        "start_row": 1,
        "start_column": 1,
        "row_count": 1,
        "column_count": 2,
        "cell_count": 2,
        "cells": [
            {
                "row_offset": 0,
                "column_offset": 0,
                "value": 42.5,
                "value_truncated": false,
                "displayed_text": "42.50",
                "displayed_text_truncated": false,
                "has_formula": false,
                "formula": null,
                "formula_truncated": false,
                "formula_hidden": false,
                "formula_external_reference": false,
                "number_format": "0.000 \"private-budget\"",
                "number_format_truncated": false,
                "number_format_unavailable": false,
                "is_error": false
            },
            {
                "row_offset": 0,
                "column_offset": 1,
                "value": 85.0,
                "value_truncated": false,
                "displayed_text": "85",
                "displayed_text_truncated": false,
                "has_formula": true,
                "formula": "=A1*2",
                "formula_truncated": false,
                "formula_hidden": false,
                "formula_external_reference": false,
                "number_format": "0.00",
                "number_format_truncated": false,
                "number_format_unavailable": false,
                "is_error": false
            }
        ]
    });
    let cells =
        normalize_range_read_response(response, &target, &range).expect("normalized range cells");
    let snapshot_id =
        range_snapshot_id(&target, &range, cells.as_slice()).expect("range snapshot ID");
    let mut reformatted = cells.clone();
    reformatted[0]["number_format"] = json!("0");
    reformatted[0]["number_format_preset"] = json!("integer");
    reformatted[0]["number_format_custom"] = json!(false);
    assert_ne!(
        snapshot_id,
        range_snapshot_id(&target, &range, reformatted.as_slice())
            .expect("reformatted range snapshot ID")
    );
    let projected = range_read_response(&target, &range, cells).expect("public range response");
    assert_eq!(
        projected
            .pointer("/cells/0/1/formula")
            .and_then(Value::as_str),
        Some("=A1*2")
    );
    assert_eq!(
        projected
            .pointer("/cells/0/1/address")
            .and_then(Value::as_str),
        Some("B1")
    );
    assert_eq!(
        projected
            .pointer("/cells/0/1/number_format_preset")
            .and_then(Value::as_str),
        Some("decimal_2")
    );
    assert_eq!(
        projected
            .pointer("/cells/0/0/number_format_custom")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert!(projected.pointer("/cells/0/1/number_format").is_none());
    let serialized = serde_json::to_string(&projected).expect("serialized tool response");
    assert!(!serialized.contains("/private/secret"));
    assert!(!serialized.contains("identity_source"));
    assert!(!serialized.contains("private-budget"));
    assert!(projected
        .get("range_snapshot_id")
        .and_then(Value::as_str)
        .is_some_and(|value| value.starts_with("excel_range_") && value.len() == 76));
}

#[test]
fn write_result_requires_exact_snapshot_safe_cells_and_verified_values() {
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
    ensure_write_target_is_mutable(&target).expect("mutable exact target");
    let range = parse_a1_range("A1:B1").expect("range");
    let current = normalize_range_read_response(
        sample_range_bridge_response(&target, &range, json!(42.5), json!(85.0), "=A1*2"),
        &target,
        &range,
    )
    .expect("current cells");
    ensure_snapshot_cells_are_write_safe(current.as_slice()).expect("safe rollback snapshot");
    let snapshot_id =
        range_snapshot_id(&target, &range, current.as_slice()).expect("range snapshot ID");
    let input = RangeWriteInput {
        workbook_id: workbook_id.to_string(),
        worksheet_id: worksheet_id.to_string(),
        range: range.clone(),
        expected_snapshot_id: snapshot_id,
        cells: vec![
            WriteCell::Value(json!(43.0)),
            WriteCell::Formula("=A1*3".to_string()),
        ],
    };

    let mut written =
        sample_range_bridge_response(&target, &range, json!(43.0), json!(129.0), "=A1*3");
    written["write_status"] = json!("written");
    let normalized_written =
        normalize_range_write_response(written, &target, &input, current.as_slice())
            .expect("verified write result");
    assert!(
        desired_cells_match(input.cells.as_slice(), normalized_written.as_slice())
            .expect("desired write comparison")
    );

    let mut rolled_back =
        sample_range_bridge_response(&target, &range, json!(42.5), json!(85.0), "=A1*2");
    rolled_back["write_status"] = json!("rolled_back");
    assert!(
        normalize_range_write_response(rolled_back, &target, &input, current.as_slice(),)
            .expect_err("rolled-back write must not report success")
            .to_string()
            .contains("restored and verified")
    );
}

#[test]
fn number_format_result_preserves_contents_and_verifies_exact_preset() {
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
    let range = parse_a1_range("A1:B1").expect("range");
    let current = normalize_range_read_response(
        sample_range_bridge_response(&target, &range, json!(42.5), json!(85.0), "=A1*2"),
        &target,
        &range,
    )
    .expect("current cells");
    ensure_snapshot_cells_are_format_safe(current.as_slice()).expect("safe format snapshot");
    let input = RangeFormatInput {
        workbook_id: workbook_id.to_string(),
        worksheet_id: worksheet_id.to_string(),
        range: range.clone(),
        expected_snapshot_id: range_snapshot_id(&target, &range, current.as_slice())
            .expect("range snapshot ID"),
        preset: "percent_2".to_string(),
        number_format: "0.00%".to_string(),
    };
    let mut formatted =
        sample_range_bridge_response(&target, &range, json!(42.5), json!(85.0), "=A1*2");
    formatted["write_status"] = json!("formatted");
    formatted["cells"][0]["displayed_text"] = json!("4250.00%");
    formatted["cells"][1]["displayed_text"] = json!("8500.00%");
    formatted["cells"][0]["number_format"] = json!("0.00%");
    formatted["cells"][1]["number_format"] = json!("0.00%");
    let normalized_formatted =
        normalize_range_format_response(formatted, &target, &input, current.as_slice())
            .expect("verified number format result");
    assert!(
        formatted_cells_match(current.as_slice(), normalized_formatted.as_slice(), "0.00%")
            .expect("formatted comparison")
    );

    let result = range_format_response(&target, &input, normalized_formatted, false)
        .expect("public number format response");
    assert_eq!(
        result.get("number_format_preset").and_then(Value::as_str),
        Some("percent_2")
    );
    assert_eq!(
        result
            .pointer("/cells/0/0/number_format_preset")
            .and_then(Value::as_str),
        Some("percent_2")
    );
    assert!(result.pointer("/cells/0/0/number_format").is_none());
}
