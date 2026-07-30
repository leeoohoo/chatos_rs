// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use super::tool;

pub(super) fn tool_definitions() -> Vec<Value> {
    vec![
        inspect_artifact_template_tool(),
        create_artifact_template_tool(),
        instantiate_artifact_template_tool(),
        render_artifact_template_preview_tool(),
    ]
}

fn inspect_artifact_template_tool() -> Value {
    tool(
        "inspect_artifact_template",
        "Inspect a ChatOS artifact template directory, verify its source artifact hash, and validate declared semantic placeholder occurrences.",
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
        "Package a local DOCX, PDF, PPTX, XLSX, or CSV artifact as a reusable ChatOS template. DOCX, PPTX, and XLSX templates may declare bounded {{NAME}} placeholders contained within individual text runs or cells.",
        json!({
            "type":"object",
            "properties":{
                "source_path":{"type":"string"},
                "target_directory":{"type":"string"},
                "template_name":{"type":"string"},
                "version":{"type":"string","default":"1.0.0"},
                "description":{"type":"string","default":""},
                "placeholders":{
                    "type":"array","maxItems":100,"uniqueItems":true,
                    "items":{
                        "type":"object",
                        "properties":{
                            "name":{"type":"string","pattern":"^[A-Za-z][A-Za-z0-9_]{0,63}$"},
                            "description":{"type":"string","maxLength":1000,"default":""},
                            "required":{"type":"boolean","default":true},
                            "default":{"type":"string","maxLength":100000},
                            "max_length":{"type":"integer","minimum":1,"maximum":100000,"default":100000}
                        },
                        "required":["name"],
                        "additionalProperties":false
                    }
                },
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
        "Instantiate a verified ChatOS artifact template. Schema-v2 DOCX, PPTX, and XLSX templates replace declared {{NAME}} placeholders inside individual text runs or cells while preserving unrelated package parts.",
        json!({
            "type":"object",
            "properties":{
                "template_directory":{"type":"string"},
                "target_path":{"type":"string"},
                "values":{"type":"object","maxProperties":100,"additionalProperties":{"type":"string","maxLength":100000},"default":{}},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["template_directory","target_path"],
            "additionalProperties":false
        }),
    )
}

fn render_artifact_template_preview_tool() -> Value {
    tool(
        "render_artifact_template_preview",
        "Verify and render the immutable DOCX, PDF, PPTX, or XLSX reference stored in a ChatOS artifact template. Rendered pages are transient model input and still require explicit visual review.",
        json!({
            "type":"object",
            "properties":{
                "template_directory":{"type":"string"},
                "first_page":{"type":"integer","minimum":1,"maximum":500,"default":1,"description":"First document/PDF/spreadsheet page, or first visible PPTX slide, to render."},
                "last_page":{"type":"integer","minimum":1,"maximum":500,"description":"Inclusive last page or visible slide. At most eight consecutive items may be attached per call."},
                "dpi":{"type":"integer","minimum":96,"maximum":160,"default":120},
                "timeout_seconds":{"type":"integer","minimum":15,"maximum":180,"default":120}
            },
            "required":["template_directory"],
            "additionalProperties":false
        }),
    )
}
