// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Map, Value};

fn mutation_target_properties() -> Map<String, Value> {
    [
        (
            "workbook_id".to_string(),
            json!({
                "type": "string",
                "minLength": 1,
                "maxLength": 96,
                "description": "Exact opaque workbook identity from the current Excel discovery snapshot."
            }),
        ),
        (
            "worksheet_id".to_string(),
            json!({
                "type": "string",
                "minLength": 1,
                "maxLength": 96,
                "description": "Exact opaque worksheet identity from the current workbook inspection."
            }),
        ),
        (
            "range".to_string(),
            json!({
                "type": "string",
                "pattern": "^[A-Z]{1,3}[1-9][0-9]*(?::[A-Z]{1,3}[1-9][0-9]*)?$",
                "maxLength": 32,
                "description": "The exact canonical uppercase A1 range used for the fresh read snapshot."
            }),
        ),
        (
            "expected_snapshot_id".to_string(),
            json!({
                "type": "string",
                "pattern": "^excel_range_[0-9a-f]{64}$",
                "maxLength": 96,
                "description": "Exact range_snapshot_id returned by a fresh excel_read_range for the same workbook, worksheet, and range."
            }),
        ),
    ]
    .into_iter()
    .collect()
}

pub(super) fn tool_definitions(include_write: bool) -> Vec<Value> {
    let mut tools = vec![
        json!({
            "name": "excel_live_status",
            "description": "Inspect whether Microsoft Excel is installed and already running, without launching it or reading workbook names or cell contents.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "excel_list_open_workbooks",
            "description": "List bounded metadata for workbooks already open in the current Microsoft Excel instance. Returns opaque workbook identities and never launches Excel or reads cell contents.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "excel_inspect_workbook",
            "description": "Inspect worksheet names, visibility, protection, and active state for one exact opaque workbook identity returned by excel_list_open_workbooks. Does not read cells or mutate Excel.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workbook_id": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 96,
                        "description": "Exact opaque workbook identity returned by excel_list_open_workbooks."
                    }
                },
                "required": ["workbook_id"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "excel_read_range",
            "description": "Read up to 256 cells from one exact worksheet and canonical A1 range in an already-open Microsoft Excel workbook. Returns bounded scalar values, displayed text, non-hidden non-external formulas, and a safe number-format classification without activating, recalculating, or mutating Excel.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workbook_id": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 96,
                        "description": "Exact opaque workbook identity returned by excel_list_open_workbooks."
                    },
                    "worksheet_id": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 96,
                        "description": "Exact opaque worksheet identity returned by excel_inspect_workbook."
                    },
                    "range": {
                        "type": "string",
                        "pattern": "^[A-Z]{1,3}[1-9][0-9]*(?::[A-Z]{1,3}[1-9][0-9]*)?$",
                        "maxLength": 32,
                        "description": "Canonical uppercase A1 range without a sheet name, dollar signs, unions, or whole-row/whole-column references."
                    }
                },
                "required": ["workbook_id", "worksheet_id", "range"],
                "additionalProperties": false
            }
        }),
    ];
    if include_write {
        let mut write_properties = mutation_target_properties();
        write_properties.insert(
            "cells".to_string(),
            json!({
                "type": "array",
                "minItems": 1,
                "maxItems": 256,
                "description": "Exact rectangular row matrix matching the target range geometry.",
                "items": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 256,
                    "items": {
                        "oneOf": [
                            {
                                "type": "object",
                                "properties": {"kind": {"const": "blank"}},
                                "required": ["kind"],
                                "additionalProperties": false
                            },
                            {
                                "type": "object",
                                "properties": {
                                    "kind": {"const": "value"},
                                    "value": {"type": ["boolean", "number", "string"], "maxLength": 128}
                                },
                                "required": ["kind", "value"],
                                "additionalProperties": false
                            },
                            {
                                "type": "object",
                                "properties": {
                                    "kind": {"const": "formula"},
                                    "formula": {"type": "string", "minLength": 2, "maxLength": 128, "pattern": "^="}
                                },
                                "required": ["kind", "formula"],
                                "additionalProperties": false
                            }
                        ]
                    }
                }
            }),
        );
        tools.push(json!({
            "name": "excel_write_range",
            "description": "After mandatory interactive approval, replace the contents of up to 256 exact visible, unprotected cells in an already-open writable Excel workbook. Requires the exact optimistic snapshot ID from a fresh excel_read_range result; writes only typed blanks, scalar constants, or strictly allowlisted local formulas, verifies content and number-format preservation, and attempts verified rollback on partial failure. Does not save, export, activate, select, format, or explicitly recalculate Excel.",
            "inputSchema": {
                "type": "object",
                "properties": write_properties,
                "required": ["workbook_id", "worksheet_id", "range", "expected_snapshot_id", "cells"],
                "additionalProperties": false
            }
        }));
        let mut format_properties = mutation_target_properties();
        format_properties.insert(
            "preset".to_string(),
            json!({
                "type": "string",
                "enum": ["general", "integer", "decimal_2", "percent_2", "date", "datetime", "text"],
                "description": "Fixed locale-independent number-format preset; arbitrary custom format strings are not accepted."
            }),
        );
        tools.push(json!({
            "name": "excel_set_number_format",
            "description": "After mandatory interactive approval, apply one fixed allowlisted number-format preset to up to 256 exact visible, unprotected cells in an already-open writable Excel workbook. Requires the exact snapshot ID from a fresh excel_read_range, preserves cell contents and formulas, verifies the result, and attempts exact format rollback on partial failure. Does not save, export, activate, select, or explicitly recalculate Excel.",
            "inputSchema": {
                "type": "object",
                "properties": format_properties,
                "required": ["workbook_id", "worksheet_id", "range", "expected_snapshot_id", "preset"],
                "additionalProperties": false
            }
        }));
    }
    tools
}

pub(super) fn requires_interactive_approval(operation: &str) -> bool {
    matches!(operation, "excel_write_range" | "excel_set_number_format")
}
