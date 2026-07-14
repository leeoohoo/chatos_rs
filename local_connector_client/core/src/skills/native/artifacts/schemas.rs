// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

pub(super) fn tool_definitions(skill_id: &str) -> Vec<Value> {
    match skill_id {
        "internal_skill_pdf" => vec![inspect_pdf_tool(), extract_pdf_text_tool()],
        "internal_skill_documents" => vec![inspect_docx_tool(), create_docx_tool()],
        "internal_skill_spreadsheets" => vec![
            inspect_spreadsheet_tool(),
            create_xlsx_tool(),
            create_csv_tool(),
        ],
        "internal_skill_presentations" => vec![inspect_pptx_tool(), create_pptx_tool()],
        "internal_skill_template_creator" => vec![
            inspect_artifact_template_tool(),
            create_artifact_template_tool(),
            instantiate_artifact_template_tool(),
        ],
        _ => Vec::new(),
    }
}

fn inspect_pdf_tool() -> Value {
    tool(
        "inspect_pdf",
        "Inspect a PDF inside the authorized local workspace and report its page count and metadata.",
        json!({
            "type":"object",
            "properties":{"path":{"type":"string"}},
            "required":["path"],
            "additionalProperties":false
        }),
    )
}

fn extract_pdf_text_tool() -> Value {
    tool(
        "extract_pdf_text",
        "Extract text from a PDF locally. Optionally save the extracted UTF-8 text inside the authorized workspace.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string"},
                "target_path":{"type":"string","description":"Optional workspace-relative .txt output path."},
                "max_chars":{"type":"integer","minimum":1,"maximum":500000,"default":100000},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path"],
            "additionalProperties":false
        }),
    )
}

fn inspect_docx_tool() -> Value {
    tool(
        "inspect_docx",
        "Inspect and extract a text preview from a DOCX file in the authorized local workspace.",
        path_only_schema(),
    )
}

fn create_docx_tool() -> Value {
    tool(
        "create_docx",
        "Create a standards-based DOCX document locally from a title and paragraphs.",
        json!({
            "type":"object",
            "properties":{
                "target_path":{"type":"string"},
                "title":{"type":"string"},
                "paragraphs":{"type":"array","items":{"type":"string"},"maxItems":2000},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["target_path","paragraphs"],
            "additionalProperties":false
        }),
    )
}

fn inspect_spreadsheet_tool() -> Value {
    tool(
        "inspect_spreadsheet",
        "Inspect a local CSV or XLSX workbook and report its basic structure.",
        path_only_schema(),
    )
}

fn create_xlsx_tool() -> Value {
    tool(
        "create_xlsx",
        "Create a basic XLSX workbook locally from a two-dimensional JSON array.",
        table_output_schema(".xlsx"),
    )
}

fn create_csv_tool() -> Value {
    tool(
        "create_csv",
        "Create an RFC 4180-style UTF-8 CSV file locally from a two-dimensional JSON array.",
        table_output_schema(".csv"),
    )
}

fn inspect_pptx_tool() -> Value {
    tool(
        "inspect_pptx",
        "Inspect a PPTX presentation in the authorized local workspace.",
        path_only_schema(),
    )
}

fn create_pptx_tool() -> Value {
    tool(
        "create_pptx",
        "Create a basic widescreen PPTX presentation locally from title/body slide definitions.",
        json!({
            "type":"object",
            "properties":{
                "target_path":{"type":"string"},
                "slides":{
                    "type":"array",
                    "minItems":1,
                    "maxItems":200,
                    "items":{
                        "type":"object",
                        "properties":{"title":{"type":"string"},"body":{"type":"string"}},
                        "required":["title"],
                        "additionalProperties":false
                    }
                },
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["target_path","slides"],
            "additionalProperties":false
        }),
    )
}

fn inspect_artifact_template_tool() -> Value {
    tool(
        "inspect_artifact_template",
        "Inspect a ChatOS artifact template directory and verify its bundled source artifact hash.",
        json!({
            "type":"object",
            "properties":{"template_directory":{"type":"string"}},
            "required":["template_directory"],
            "additionalProperties":false
        }),
    )
}

fn create_artifact_template_tool() -> Value {
    tool(
        "create_artifact_template",
        "Package a local DOCX, PDF, PPTX, XLSX, or CSV artifact as a reusable ChatOS template.",
        json!({
            "type":"object",
            "properties":{
                "source_path":{"type":"string"},
                "target_directory":{"type":"string"},
                "template_name":{"type":"string"},
                "version":{"type":"string","default":"1.0.0"},
                "description":{"type":"string","default":""},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["source_path","target_directory","template_name"],
            "additionalProperties":false
        }),
    )
}

fn instantiate_artifact_template_tool() -> Value {
    tool(
        "instantiate_artifact_template",
        "Create a new local artifact by copying the immutable source artifact from a verified ChatOS template.",
        json!({
            "type":"object",
            "properties":{
                "template_directory":{"type":"string"},
                "target_path":{"type":"string"},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["template_directory","target_path"],
            "additionalProperties":false
        }),
    )
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

fn table_output_schema(extension: &str) -> Value {
    json!({
        "type":"object",
        "properties":{
            "target_path":{"type":"string","description":format!("Workspace-relative {extension} output path.")},
            "sheet_name":{"type":"string","default":"Sheet1"},
            "rows":{"type":"array","items":{"type":"array","items":{}},"maxItems":10000},
            "overwrite":{"type":"boolean","default":false}
        },
        "required":["target_path","rows"],
        "additionalProperties":false
    })
}
