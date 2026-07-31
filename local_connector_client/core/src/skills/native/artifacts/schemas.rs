// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

mod artifact_template;
mod docx;
mod docx_advanced;
mod pdf;
mod pdf_annotations;
mod presentation;
mod presentation_edit;
mod spreadsheet;

pub(super) fn tool_definitions(skill_id: &str) -> Vec<Value> {
    match skill_id {
        "internal_skill_pdf" => {
            let mut definitions = pdf::tool_definitions();
            definitions.extend(pdf_annotations::tool_definitions());
            definitions
        }
        "internal_skill_documents" => {
            let mut definitions = docx::tool_definitions();
            definitions.extend(docx_advanced::tool_definitions());
            definitions
        }
        "internal_skill_spreadsheets" => spreadsheet::tool_definitions(),
        "internal_skill_presentations" => {
            let mut definitions = presentation::tool_definitions();
            definitions.extend(presentation_edit::tool_definitions());
            definitions
        }
        "internal_skill_template_creator" => artifact_template::tool_definitions(),
        _ => Vec::new(),
    }
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({"name":name,"description":description,"inputSchema":input_schema})
}

fn path_only_schema() -> Value {
    json!({
        "type":"object",
        "properties":{"path":{"type":"string"}},
        "required":["path"],
        "additionalProperties":false
    })
}

fn text_table_output_schema(extension: &str) -> Value {
    json!({
        "type":"object",
        "properties":{
            "target_path":{"type":"string","description":format!("Workspace-relative {extension} output path.")},
            "rows":text_table_rows_schema(false),
            "overwrite":{"type":"boolean","default":false}
        },
        "required":["target_path","rows"],
        "additionalProperties":false
    })
}

fn text_table_rows_schema(require_non_empty: bool) -> Value {
    let mut schema = json!({
        "type":"array",
        "maxItems":10000,
        "items":{
            "type":"array",
            "minItems":1,
            "maxItems":16384,
            "items":{
                "oneOf":[
                    {"type":"null"},
                    {"type":"boolean"},
                    {"type":"number"},
                    {"type":"string","maxLength":32767}
                ]
            }
        }
    });
    if require_non_empty {
        if let Value::Object(properties) = &mut schema {
            properties.insert("minItems".to_string(), json!(1));
        }
    }
    schema
}

fn spreadsheet_rows_schema() -> Value {
    json!({
        "type":"array",
        "maxItems":100000,
        "items":{
            "type":"array",
            "maxItems":16384,
            "items":spreadsheet_cell_schema()
        }
    })
}

fn spreadsheet_cell_schema() -> Value {
    json!({
        "oneOf":[
            {"type":"null"},
            {"type":"boolean"},
            {"type":"number"},
            {"type":"string","maxLength":32767},
            {
                "type":"object",
                "properties":{
                    "value":{"oneOf":[{"type":"null"},{"type":"boolean"},{"type":"number"},{"type":"string","maxLength":32767}]},
                    "formula":{"type":"string","minLength":1,"maxLength":4096,"description":"Safe local formula, with or without a leading equals sign. Only ABS, AND, AVERAGE, COUNT, COUNTA, IF, MAX, MIN, NOT, OR, ROUND, and SUM are allowed; external links and string literals are rejected."},
                    "cached_value":{"oneOf":[{"type":"null"},{"type":"boolean"},{"type":"number"},{"type":"string","maxLength":32767}]},
                    "number_format":{"type":"string","enum":["general","integer","decimal_2","percent_2","date","datetime"]}
                },
                "oneOf":[
                    {"required":["value"],"not":{"anyOf":[{"required":["formula"]},{"required":["cached_value"]}]}},
                    {"required":["formula"],"not":{"required":["value"]}}
                ],
                "additionalProperties":false
            }
        ]
    })
}
