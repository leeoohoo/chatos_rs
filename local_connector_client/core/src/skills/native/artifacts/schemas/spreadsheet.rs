// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use super::{
    path_only_schema, spreadsheet_rows_schema, text_table_output_schema, text_table_rows_schema,
    tool,
};

pub(super) fn tool_definitions() -> Vec<Value> {
    vec![
        inspect_spreadsheet_tool(),
        render_spreadsheet_pages_tool(),
        create_xlsx_tool(),
        update_xlsx_range_tool(),
        create_csv_tool(),
        update_csv_range_tool(),
        create_tsv_tool(),
        update_tsv_range_tool(),
    ]
}

fn inspect_spreadsheet_tool() -> Value {
    tool(
        "inspect_spreadsheet",
        "Inspect a local CSV, TSV, or XLSX workbook and report its bounded basic structure. TSV inspection also returns an exact SHA-256 for optimistic-lock edits.",
        path_only_schema(),
    )
}

fn render_spreadsheet_pages_tool() -> Value {
    tool(
        "render_spreadsheet_pages",
        "Validate one regular non-symlink workspace XLSX, reject active or externally connected content, convert it with the packaged manifest-verified LibreOffice runtime, and attach a bounded combined-PDF page range as transient PNG model input for visual QA. Rendering does not itself claim that visual review passed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .xlsx path."},
                "first_page":{"type":"integer","minimum":1,"maximum":500,"default":1,"description":"First page in LibreOffice's combined PDF output across worksheets."},
                "last_page":{"type":"integer","minimum":1,"maximum":500,"description":"Inclusive final combined-PDF page. At most 8 pages may be attached per call; omit to render up to 8 pages from first_page."},
                "dpi":{"type":"integer","minimum":96,"maximum":160,"default":120},
                "timeout_seconds":{"type":"integer","minimum":15,"maximum":180,"default":120},
                "pdf_target_path":{"type":"string","description":"Optional distinct workspace-relative .pdf export after the converted PDF passes validation."},
                "overwrite":{"type":"boolean","default":false,"description":"Allow replacement only for an existing regular non-symlink PDF export target."}
            },
            "required":["path"],
            "additionalProperties":false
        }),
    )
}

fn create_xlsx_tool() -> Value {
    tool(
        "create_xlsx",
        "Create a bounded XLSX workbook locally with one to 64 worksheets, typed values, safe formulas, built-in number formats, column widths, and frozen header rows.",
        json!({
            "type":"object",
            "properties":{
                "target_path":{"type":"string","description":"Workspace-relative .xlsx output path."},
                "sheet_name":{"type":"string","minLength":1,"maxLength":31,"default":"Sheet1","description":"Legacy single-sheet mode name."},
                "rows":spreadsheet_rows_schema(),
                "worksheets":{
                    "type":"array",
                    "minItems":1,
                    "maxItems":64,
                    "items":{
                        "type":"object",
                        "properties":{
                            "name":{"type":"string","minLength":1,"maxLength":31},
                            "rows":spreadsheet_rows_schema(),
                            "freeze_rows":{"type":"integer","minimum":0,"maximum":1000,"default":0},
                            "column_widths":{
                                "type":"array",
                                "maxItems":256,
                                "items":{
                                    "type":"object",
                                    "properties":{
                                        "column":{"type":"string","pattern":"^[A-Za-z]{1,3}$"},
                                        "width":{"type":"number","minimum":0.1,"maximum":255}
                                    },
                                    "required":["column","width"],
                                    "additionalProperties":false
                                }
                            }
                        },
                        "required":["name","rows"],
                        "additionalProperties":false
                    }
                },
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["target_path"],
            "oneOf":[
                {"required":["rows"],"not":{"required":["worksheets"]}},
                {"required":["worksheets"],"not":{"required":["rows"]}}
            ],
            "additionalProperties":false
        }),
    )
}

fn update_xlsx_range_tool() -> Value {
    tool(
        "update_xlsx_range",
        "Write a bounded rectangular value range to one existing XLSX worksheet and save a distinct output while preserving unrelated package entries and unchanged cells. Merged cells and shared/array/data-table formula intersections fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .xlsx path."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .xlsx output path."},
                "sheet_name":{"type":"string","minLength":1,"maxLength":31},
                "start_cell":{"type":"string","pattern":"^[A-Za-z]{1,3}[1-9][0-9]{0,6}$","description":"Top-left cell in A1 notation."},
                "values":spreadsheet_rows_schema(),
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path","sheet_name","start_cell","values"],
            "additionalProperties":false
        }),
    )
}

fn create_csv_tool() -> Value {
    tool(
        "create_csv",
        "Create a bounded RFC 4180-style UTF-8 CSV with CRLF records and spreadsheet formula-injection protection for string cells.",
        text_table_output_schema(".csv"),
    )
}

fn update_csv_range_tool() -> Value {
    update_delimited_range_tool("update_csv_range", "CSV", ".csv")
}

fn create_tsv_tool() -> Value {
    tool(
        "create_tsv",
        "Create a bounded UTF-8 TSV with CRLF records, unambiguous RFC 4180-style quoted fields, and spreadsheet formula-injection protection for string cells.",
        text_table_output_schema(".tsv"),
    )
}

fn update_tsv_range_tool() -> Value {
    update_delimited_range_tool("update_tsv_range", "TSV", ".tsv")
}

fn update_delimited_range_tool(name: &str, label: &str, extension: &str) -> Value {
    tool(
        name,
        format!("Safely replace one exact rectangular A1 range in a bounded rectangular UTF-8 {label}, using the source SHA-256 returned by inspect_spreadsheet and a distinct output path while preserving every unchanged cell.").as_str(),
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":format!("Workspace-relative regular non-symlink source {extension} path.")},
                "expected_sha256":{"type":"string","pattern":"^[0-9a-f]{64}$","description":"Exact source SHA-256 returned by inspect_spreadsheet."},
                "start_cell":{"type":"string","pattern":"^[A-Za-z]{1,3}(?:[1-9][0-9]{0,3}|10000)$","description":"Inclusive top-left cell in A1 notation."},
                "end_cell":{"type":"string","pattern":"^[A-Za-z]{1,3}(?:[1-9][0-9]{0,3}|10000)$","description":"Inclusive bottom-right cell in A1 notation."},
                "values":text_table_rows_schema(true),
                "target_path":{"type":"string","description":format!("Distinct workspace-relative {extension} output path.")},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","expected_sha256","start_cell","end_cell","values","target_path"],
            "additionalProperties":false
        }),
    )
}
