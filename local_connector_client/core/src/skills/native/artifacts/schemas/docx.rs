// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use super::{path_only_schema, tool};

pub(super) fn tool_definitions() -> Vec<Value> {
    vec![
        inspect_docx_tool(),
        render_docx_pages_tool(),
        update_docx_metadata_tool(),
        create_docx_tool(),
        create_structured_docx_tool(),
        append_docx_content_tool(),
        insert_docx_content_at_paragraph_tool(),
        insert_docx_content_at_paragraph_index_tool(),
        delete_docx_paragraph_tool(),
        delete_docx_paragraph_at_index_tool(),
        move_docx_paragraph_tool(),
        move_docx_paragraph_at_index_tool(),
        replace_docx_paragraph_with_content_tool(),
        replace_docx_paragraph_at_index_with_content_tool(),
        replace_docx_text_tool(),
        replace_docx_text_across_runs_tool(),
    ]
}

fn update_docx_metadata_tool() -> Value {
    tool(
        "update_docx_metadata",
        "Set or remove bounded standard DOCX core title, author, subject, and keywords properties while preserving unrelated core properties and all other package entries. Missing standard core-properties metadata is created conservatively.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "title":{"type":"string","maxLength":1000},
                "author":{"type":"string","maxLength":256},
                "subject":{"type":"string","maxLength":1000},
                "keywords":{"type":"string","maxLength":1000},
                "remove_fields":{"type":"array","maxItems":4,"uniqueItems":true,"items":{"type":"string","enum":["title","author","subject","keywords"]}},
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path"],
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

fn render_docx_pages_tool() -> Value {
    tool(
        "render_docx_pages",
        "Convert one regular non-symlink workspace DOCX with the packaged, manifest-verified LibreOffice runtime, validate the resulting PDF, and attach a bounded page range as transient PNG model input for visual QA. Optionally persist the verified PDF to a distinct workspace path. Rendering does not itself claim that layout passed visual review.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "first_page":{"type":"integer","minimum":1,"maximum":500,"default":1},
                "last_page":{"type":"integer","minimum":1,"maximum":500,"description":"Inclusive final page. At most 8 pages may be attached per call; omit to render up to 8 pages from first_page."},
                "dpi":{"type":"integer","minimum":96,"maximum":160,"default":120},
                "timeout_seconds":{"type":"integer","minimum":15,"maximum":180,"default":120},
                "pdf_target_path":{"type":"string","description":"Optional distinct workspace-relative .pdf output path. The DOCX source is never overwritten."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path"],
            "additionalProperties":false
        }),
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

fn create_structured_docx_tool() -> Value {
    tool(
        "create_structured_docx",
        "Create a DOCX from bounded styled paragraph, table, and page-break blocks inside the authorized workspace.",
        json!({
            "type":"object",
            "properties":{
                "target_path":{"type":"string"},
                "blocks":docx_blocks_schema(),
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["target_path","blocks"],
            "additionalProperties":false
        }),
    )
}

fn append_docx_content_tool() -> Value {
    tool(
        "append_docx_content",
        "Append bounded structured blocks before the final section properties of a DOCX and write a distinct workspace output while preserving other ZIP entries.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string"},
                "target_path":{"type":"string"},
                "blocks":docx_blocks_schema(),
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path","blocks"],
            "additionalProperties":false
        }),
    )
}

fn insert_docx_content_at_paragraph_tool() -> Value {
    tool(
        "insert_docx_content_at_paragraph",
        "Insert bounded structured paragraph, table, or page-break blocks immediately before or after one globally unique eligible top-level DOCX paragraph while preserving the anchor and all unrelated package entries.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "anchor_text":{"type":"string","minLength":1,"maxLength":4096,"description":"Complete visible text of one globally unique eligible top-level paragraph."},
                "position":{"type":"string","enum":["before","after"]},
                "blocks":docx_blocks_schema(),
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","anchor_text","position","blocks","target_path"],
            "additionalProperties":false
        }),
    )
}

fn insert_docx_content_at_paragraph_index_tool() -> Value {
    tool(
        "insert_docx_content_at_paragraph_index",
        "Insert bounded structured paragraph, table, or page-break blocks immediately before or after one explicitly indexed direct top-level DOCX paragraph after complete expected-text verification. Empty paragraphs and repeated paragraph text are supported; complex paragraphs, section properties, document ranges, mismatched text, and in-place output fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "paragraph":{"type":"integer","minimum":1,"maximum":2000,"description":"One-based direct top-level paragraph index returned by inspect_docx."},
                "expected_text":{"type":"string","maxLength":4096,"description":"Complete visible text of the indexed paragraph. Use an empty string for an empty paragraph."},
                "position":{"type":"string","enum":["before","after"]},
                "blocks":docx_blocks_schema(),
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","paragraph","expected_text","position","blocks","target_path"],
            "additionalProperties":false
        }),
    )
}

fn delete_docx_paragraph_tool() -> Value {
    tool(
        "delete_docx_paragraph",
        "Delete one globally unique eligible top-level DOCX paragraph selected by its complete visible text while preserving every unrelated paragraph and package entry in a distinct output.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "anchor_text":{"type":"string","minLength":1,"maxLength":4096,"description":"Complete visible text of one globally unique eligible top-level paragraph."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","anchor_text","target_path"],
            "additionalProperties":false
        }),
    )
}

fn delete_docx_paragraph_at_index_tool() -> Value {
    tool(
        "delete_docx_paragraph_at_index",
        "Delete one explicitly indexed direct top-level DOCX paragraph after complete expected-text verification. Unlike text-anchor deletion, this supports empty paragraphs and repeated paragraph text. Complex paragraphs, section properties, document ranges, mismatched text, and in-place output fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "paragraph":{"type":"integer","minimum":1,"maximum":2000,"description":"One-based direct top-level paragraph index returned by inspect_docx."},
                "expected_text":{"type":"string","maxLength":4096,"description":"Complete visible text of the indexed paragraph. Use an empty string for an empty paragraph."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","paragraph","expected_text","target_path"],
            "additionalProperties":false
        }),
    )
}

fn move_docx_paragraph_tool() -> Value {
    tool(
        "move_docx_paragraph",
        "Move one globally unique eligible top-level DOCX paragraph immediately before or after another globally unique eligible top-level paragraph while preserving exact paragraph XML and unrelated package entries.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "anchor_text":{"type":"string","minLength":1,"maxLength":4096,"description":"Complete visible text of the globally unique eligible top-level paragraph to move."},
                "reference_text":{"type":"string","minLength":1,"maxLength":4096,"description":"Complete visible text of the distinct globally unique eligible top-level destination paragraph."},
                "position":{"type":"string","enum":["before","after"]},
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","anchor_text","reference_text","position","target_path"],
            "additionalProperties":false
        }),
    )
}

fn move_docx_paragraph_at_index_tool() -> Value {
    tool(
        "move_docx_paragraph_at_index",
        "Move one explicitly indexed direct top-level DOCX paragraph immediately before or after another explicitly indexed direct top-level paragraph after complete expected-text verification for both selections. Empty paragraphs and repeated paragraph text are supported; complex paragraphs, document ranges, same-paragraph selection, no-op adjacency, mismatched text, and in-place output fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "paragraph":{"type":"integer","minimum":1,"maximum":2000,"description":"One-based direct top-level paragraph index to move, returned by inspect_docx."},
                "expected_text":{"type":"string","maxLength":4096,"description":"Complete visible text of the indexed paragraph to move. Use an empty string for an empty paragraph."},
                "reference_paragraph":{"type":"integer","minimum":1,"maximum":2000,"description":"One-based direct top-level reference paragraph index returned by inspect_docx."},
                "reference_expected_text":{"type":"string","maxLength":4096,"description":"Complete visible text of the indexed reference paragraph. Use an empty string for an empty paragraph."},
                "position":{"type":"string","enum":["before","after"]},
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","paragraph","expected_text","reference_paragraph","reference_expected_text","position","target_path"],
            "additionalProperties":false
        }),
    )
}

fn replace_docx_paragraph_with_content_tool() -> Value {
    tool(
        "replace_docx_paragraph_with_content",
        "Replace one globally unique eligible top-level DOCX paragraph with bounded structured paragraph, table, or page-break blocks while preserving all unrelated package entries in a distinct output.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "anchor_text":{"type":"string","minLength":1,"maxLength":4096,"description":"Complete visible text of one globally unique eligible top-level paragraph to replace."},
                "blocks":docx_blocks_schema(),
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","anchor_text","blocks","target_path"],
            "additionalProperties":false
        }),
    )
}

fn replace_docx_paragraph_at_index_with_content_tool() -> Value {
    tool(
        "replace_docx_paragraph_at_index_with_content",
        "Replace one explicitly indexed direct top-level DOCX paragraph with bounded structured paragraph, table, or page-break blocks after complete expected-text verification. Empty paragraphs and repeated paragraph text are supported; complex paragraphs, section properties, document ranges, mismatched text, byte-identical output, and in-place output fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "paragraph":{"type":"integer","minimum":1,"maximum":2000,"description":"One-based direct top-level paragraph index returned by inspect_docx."},
                "expected_text":{"type":"string","maxLength":4096,"description":"Complete visible text of the indexed paragraph. Use an empty string for an empty paragraph."},
                "blocks":docx_blocks_schema(),
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","paragraph","expected_text","blocks","target_path"],
            "additionalProperties":false
        }),
    )
}

fn replace_docx_text_tool() -> Value {
    tool(
        "replace_docx_text",
        "Replace exact text inside individual DOCX text runs and write a distinct workspace output. Matches spanning multiple runs are intentionally not guessed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string"},
                "find":{"type":"string","minLength":1,"maxLength":4096},
                "replace":{"type":"string","maxLength":4096},
                "target_path":{"type":"string"},
                "max_replacements":{"type":"integer","minimum":1,"maximum":10000,"default":1000},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","find","replace","target_path"],
            "additionalProperties":false
        }),
    )
}

fn replace_docx_text_across_runs_tool() -> Value {
    tool(
        "replace_docx_text_across_runs",
        "Replace one globally unique visible text selection that spans 2–16 directly adjacent simple DOCX runs with byte-identical run properties. Hyperlinks, fields, comments, revisions, bookmarks, drawings, tabs, breaks, mixed formatting, and ambiguous matches fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "selection":{"type":"string","minLength":1,"maxLength":4096,"description":"Exact globally unique visible text spanning at least two adjacent same-format runs in one paragraph."},
                "replacement":{"type":"string","maxLength":4096},
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","selection","replacement","target_path"],
            "additionalProperties":false
        }),
    )
}

fn docx_blocks_schema() -> Value {
    json!({
        "type":"array",
        "minItems":1,
        "maxItems":2000,
        "items":{
            "oneOf":[
                {
                    "type":"object",
                    "properties":{
                        "type":{"const":"paragraph"},
                        "text":{"type":"string"},
                        "style":{"type":"string","enum":["normal","title","subtitle","heading1","heading2","heading3","quote"],"default":"normal"},
                        "align":{"type":"string","enum":["left","center","right","justify"],"default":"left"},
                        "bold":{"type":"boolean","default":false},
                        "italic":{"type":"boolean","default":false}
                    },
                    "required":["type","text"],
                    "additionalProperties":false
                },
                {
                    "type":"object",
                    "properties":{
                        "type":{"const":"table"},
                        "rows":{
                            "type":"array",
                            "minItems":1,
                            "maxItems":2000,
                            "items":{
                                "type":"array",
                                "minItems":1,
                                "maxItems":63,
                                "items":{"type":"string"}
                            }
                        },
                        "header_row":{"type":"boolean","default":false}
                    },
                    "required":["type","rows"],
                    "additionalProperties":false
                },
                {
                    "type":"object",
                    "properties":{"type":{"const":"page_break"}},
                    "required":["type"],
                    "additionalProperties":false
                }
            ]
        }
    })
}
