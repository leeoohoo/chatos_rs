// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

mod contracts;
mod responses;
mod safety_and_platform;

pub(super) fn sample_snapshot() -> Value {
    json!({
        "schema_version": 1,
        "installed": true,
        "running": true,
        "runtime_instance": "4242",
        "application_version": "16.99",
        "workbooks_total": 1,
        "workbooks_truncated": false,
        "workbooks": [{
            "index": 1,
            "name": "Budget.xlsx",
            "identity_source": "/private/secret/Budget.xlsx",
            "saved": false,
            "read_only": false,
            "active": true,
            "sheet_count": 2,
            "sheets_truncated": false,
            "sheets": [
                {"index": 1, "name": "Summary", "visible": "visible", "protected": false, "active": true},
                {"index": 2, "name": "Inputs", "visible": "hidden", "protected": true, "active": false}
            ]
        }]
    })
}

pub(super) fn sample_range_bridge_response(
    target: &RangeReadTarget,
    range: &A1Range,
    first_value: Value,
    second_value: Value,
    second_formula: &str,
) -> Value {
    let first_display = first_value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| first_value.to_string());
    let second_display = second_value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| second_value.to_string());
    json!({
        "schema_version": 1,
        "runtime_instance": target.runtime_instance,
        "workbook_index": target.workbook_index,
        "workbook_name": target.workbook_name,
        "worksheet_index": target.worksheet_index,
        "worksheet_name": target.worksheet_name,
        "range_address": range.canonical,
        "start_row": range.start_row,
        "start_column": range.start_column,
        "row_count": range.row_count,
        "column_count": range.column_count,
        "cell_count": range.cell_count,
        "cells": [
            {
                "row_offset": 0,
                "column_offset": 0,
                "value": first_value,
                "value_truncated": false,
                "displayed_text": first_display,
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
                "value": second_value,
                "value_truncated": false,
                "displayed_text": second_display,
                "displayed_text_truncated": false,
                "has_formula": true,
                "formula": second_formula,
                "formula_truncated": false,
                "formula_hidden": false,
                "formula_external_reference": false,
                "number_format": "0.00",
                "number_format_truncated": false,
                "number_format_unavailable": false,
                "is_error": false
            }
        ]
    })
}
