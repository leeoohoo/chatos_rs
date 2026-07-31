// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use super::tool;

pub(super) fn tool_definitions() -> Vec<Value> {
    vec![
        replace_docx_header_footer_text_tool(),
        replace_docx_table_cell_text_tool(),
        delete_docx_table_row_tool(),
        insert_docx_table_row_tool(),
        move_docx_table_row_tool(),
        insert_docx_image_tool(),
        add_docx_header_footer_tool(),
        add_docx_comment_tool(),
        replace_docx_text_tracked_tool(),
        resolve_docx_tracked_changes_tool(),
    ]
}

fn replace_docx_header_footer_text_tool() -> Value {
    tool(
        "replace_docx_header_footer_text",
        "Replace exact text inside individual Word text runs of referenced DOCX header/footer parts while preserving multi-section references, formatting, relationships, and all unrelated package entries. Write a distinct workspace output.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path. The source is never modified."},
                "find":{"type":"string","minLength":1,"maxLength":4096,"description":"Exact text contained inside one w:t element. Matches never cross runs or package parts."},
                "replacement":{"type":"string","maxLength":4096},
                "part_names":{"type":"array","minItems":1,"maxItems":128,"uniqueItems":true,"items":{"type":"string","minLength":1},"description":"Optional exact package part names from inspect_docx header_parts/footer_parts. Omit to search every referenced header/footer part."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "max_replacements":{"type":"integer","minimum":1,"maximum":10000,"default":100},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","find","replacement","target_path"],
            "additionalProperties":false
        }),
    )
}

fn replace_docx_table_cell_text_tool() -> Value {
    tool(
        "replace_docx_table_cell_text",
        "Replace the complete text of one explicitly indexed simple DOCX table cell while preserving its cell, paragraph, and run formatting. Merged or nested tables, multiple paragraphs or runs, revisions, comments, fields, drawings, and mismatched expected text fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "table":{"type":"integer","minimum":1,"maximum":2000,"description":"One-based top-level table index."},
                "row":{"type":"integer","minimum":1,"maximum":2000,"description":"One-based row index inside the selected table."},
                "column":{"type":"integer","minimum":1,"maximum":63,"description":"One-based cell index inside the selected row."},
                "expected_text":{"type":"string","maxLength":4096,"description":"Complete decoded text currently stored in the selected simple cell."},
                "replacement":{"type":"string","maxLength":4096},
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","table","row","column","expected_text","replacement","target_path"],
            "additionalProperties":false
        }),
    )
}

fn delete_docx_table_row_tool() -> Value {
    tool(
        "delete_docx_table_row",
        "Delete one explicitly indexed simple row from a top-level DOCX table after exact expected-cells verification. The only row, merged or nested tables, complex cells, revisions, document ranges, and mismatched expected text fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "table":{"type":"integer","minimum":1,"maximum":2000,"description":"One-based direct top-level table index."},
                "row":{"type":"integer","minimum":1,"maximum":2000,"description":"One-based direct row index inside the selected table."},
                "expected_cells":{"type":"array","minItems":1,"maxItems":63,"items":{"type":"string","maxLength":4096},"description":"Complete decoded texts of every physical cell in the selected simple row, in order."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","table","row","expected_cells","target_path"],
            "additionalProperties":false
        }),
    )
}

fn insert_docx_table_row_tool() -> Value {
    tool(
        "insert_docx_table_row",
        "Insert one simple DOCX table row immediately before or after an explicitly indexed reference row after exact expected-cells verification. The new row clones eligible reference formatting while removing paragraph identity attributes. Header rows, merged or nested tables, complex cells, revisions, and document ranges fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "table":{"type":"integer","minimum":1,"maximum":2000,"description":"One-based direct top-level table index."},
                "reference_row":{"type":"integer","minimum":1,"maximum":2000,"description":"One-based direct reference row index inside the selected table."},
                "position":{"type":"string","enum":["before","after"]},
                "expected_cells":{"type":"array","minItems":1,"maxItems":63,"items":{"type":"string","maxLength":4096},"description":"Complete decoded texts of every physical cell in the reference row, in order."},
                "cells":{"type":"array","minItems":1,"maxItems":63,"items":{"type":"string","maxLength":4096},"description":"Complete texts for the inserted row. The count must equal expected_cells."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","table","reference_row","position","expected_cells","cells","target_path"],
            "additionalProperties":false
        }),
    )
}

fn move_docx_table_row_tool() -> Value {
    tool(
        "move_docx_table_row",
        "Move one explicitly indexed simple row within the same top-level DOCX table immediately before or after another explicitly indexed simple row. Both rows require complete expected-cells verification. No-op moves, repeating headers, merged or nested tables, complex cells, revisions, and document ranges fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "table":{"type":"integer","minimum":1,"maximum":2000,"description":"One-based direct top-level table index."},
                "row":{"type":"integer","minimum":1,"maximum":2000,"description":"One-based direct row index to move."},
                "expected_cells":{"type":"array","minItems":1,"maxItems":63,"items":{"type":"string","maxLength":4096},"description":"Complete decoded texts of every physical cell in the row being moved, in order."},
                "reference_row":{"type":"integer","minimum":1,"maximum":2000,"description":"One-based direct reference row index in the original table order."},
                "reference_expected_cells":{"type":"array","minItems":1,"maxItems":63,"items":{"type":"string","maxLength":4096},"description":"Complete decoded texts of every physical cell in the reference row, in order."},
                "position":{"type":"string","enum":["before","after"]},
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","table","row","expected_cells","reference_row","reference_expected_cells","position","target_path"],
            "additionalProperties":false
        }),
    )
}

fn insert_docx_image_tool() -> Value {
    tool(
        "insert_docx_image",
        "Append one bounded workspace PNG or JPEG to a DOCX and write a distinct output while preserving aspect ratio and verified package entries.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "image_path":{"type":"string","description":"Workspace-relative .png, .jpg, or .jpeg path, at most 10 MiB and 40 megapixels."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "width_inches":{"type":"number","minimum":0.25,"maximum":8.0,"default":6.0},
                "alt_text":{"type":"string","maxLength":1024,"default":"Embedded document image"},
                "align":{"type":"string","enum":["left","center","right","justify"],"default":"center"},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","image_path","target_path"],
            "additionalProperties":false
        }),
    )
}

fn add_docx_header_footer_tool() -> Value {
    tool(
        "add_docx_header_footer",
        "Add a default text header, footer, or both to the final section of a DOCX that does not already contain the corresponding references, writing a distinct workspace output.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "header_text":{"type":"string","maxLength":100000},
                "footer_text":{"type":"string","maxLength":100000},
                "header_align":{"type":"string","enum":["left","center","right","justify"],"default":"center"},
                "footer_align":{"type":"string","enum":["left","center","right","justify"],"default":"center"},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path"],
            "anyOf":[{"required":["header_text"]},{"required":["footer_text"]}],
            "additionalProperties":false
        }),
    )
}

fn add_docx_comment_tool() -> Value {
    tool(
        "add_docx_comment",
        "Add a comment to the complete text of one exact DOCX text run and write a distinct workspace output. Cross-run, substring, nested-range, drawing, field, tab, and break selections are intentionally rejected.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "selection":{"type":"string","minLength":1,"maxLength":4096,"description":"Exact complete text of one eligible Word text run."},
                "comment":{"type":"string","minLength":1,"maxLength":20000},
                "author":{"type":"string","minLength":1,"maxLength":128,"default":"ChatOS"},
                "initials":{"type":"string","minLength":1,"maxLength":16,"default":"AI"},
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","selection","comment","target_path"],
            "additionalProperties":false
        }),
    )
}

fn replace_docx_text_tracked_tool() -> Value {
    tool(
        "replace_docx_text_tracked",
        "Replace or delete the complete text of one eligible DOCX run using standard tracked deletion/insertion markup and write a distinct output. Existing revision nesting, comments, substrings, and cross-run guesses are rejected.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "selection":{"type":"string","minLength":1,"maxLength":4096,"description":"Exact complete text of one eligible Word text run."},
                "replacement":{"type":"string","maxLength":4096,"description":"Replacement text. Use an empty string for a tracked deletion."},
                "author":{"type":"string","minLength":1,"maxLength":128,"default":"ChatOS"},
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","selection","replacement","target_path"],
            "additionalProperties":false
        }),
    )
}

fn resolve_docx_tracked_changes_tool() -> Value {
    tool(
        "resolve_docx_tracked_changes",
        "Accept or reject all simple text insertion/deletion revisions in a DOCX, or only the uniquely identified revisions in revision_ids, and write a distinct output. Move, property, table-structure, nested, comment-crossing, field, drawing, and malformed revisions are rejected globally.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .docx path."},
                "action":{"type":"string","enum":["accept","reject"],"description":"Accept or reject every supported simple text revision when revision_ids is omitted, otherwise only the requested revisions."},
                "revision_ids":{"type":"array","minItems":1,"maxItems":1000,"uniqueItems":true,"items":{"type":"integer","minimum":0,"maximum":1000000},"description":"Optional strictly increasing revision IDs returned by inspect_docx. Omit to resolve all supported simple text revisions."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .docx output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","action","target_path"],
            "additionalProperties":false
        }),
    )
}
