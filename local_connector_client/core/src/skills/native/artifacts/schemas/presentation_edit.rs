// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use super::{presentation::create_pptx_input_schema, tool};

pub(super) fn tool_definitions() -> Vec<Value> {
    vec![
        append_pptx_slides_tool(),
        reorder_pptx_slides_tool(),
        delete_pptx_slides_tool(),
        replace_pptx_text_tool(),
        replace_pptx_text_across_runs_tool(),
        replace_pptx_table_cell_text_tool(),
        copy_pptx_table_cell_format_tool(),
        delete_pptx_table_row_tool(),
        insert_pptx_table_row_tool(),
        move_pptx_table_row_tool(),
        delete_pptx_table_column_tool(),
        insert_pptx_table_column_tool(),
        move_pptx_table_column_tool(),
        replace_pptx_notes_text_tool(),
    ]
}

fn append_pptx_slides_tool() -> Value {
    let mut schema = create_pptx_input_schema();
    if let Value::Object(object) = &mut schema {
        if let Some(Value::Object(properties)) = object.get_mut("properties") {
            properties.remove("target_path");
            properties.insert(
                "path".to_string(),
                json!({"type":"string","description":"Workspace-relative source .pptx path. The source is never modified."}),
            );
            properties.insert(
                "target_path".to_string(),
                json!({"type":"string","description":"Distinct workspace-relative .pptx output path."}),
            );
        }
        object.insert(
            "required".to_string(),
            json!(["path", "target_path", "slides"]),
        );
    }
    tool(
        "append_pptx_slides",
        "Append bounded editable slides, including self-contained standard DrawingML column/bar/line/pie/area/doughnut/radar/scatter/bubble chart slides with optional canonical RGB series colors, to an existing PPTX while preserving unchanged package parts and writing a distinct output file. New slides inherit the last existing slide's layout. Speaker notes require an existing notes master.",
        schema,
    )
}

fn reorder_pptx_slides_tool() -> Value {
    tool(
        "reorder_pptx_slides",
        "Create a distinct PPTX whose visible slides follow one exact full permutation of the current presentation order. Slide parts, relationships, notes, media, masters, themes, and all unrelated package entries remain unchanged.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pptx path. The source is never modified."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pptx output path."},
                "slide_order":{"type":"array","minItems":1,"maxItems":200,"uniqueItems":true,"items":{"type":"integer","minimum":1,"maximum":200},"description":"Every current one-based slide position exactly once, in the desired output order. Omitting slides is not supported by this release."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path","slide_order"],
            "additionalProperties":false
        }),
    )
}

fn delete_pptx_slides_tool() -> Value {
    tool(
        "delete_pptx_slides",
        "Delete selected visible slides from an existing PPTX, remove their slide/relationship and uniquely owned speaker-note parts, preserve all remaining slide order and package content, and write a distinct output. At least one slide must remain.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pptx path. The source is never modified."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pptx output path."},
                "slide_numbers":{"type":"array","minItems":1,"maxItems":199,"uniqueItems":true,"items":{"type":"integer","minimum":1,"maximum":200},"description":"One-based positions in the current visible presentation order to delete. At least one slide must remain."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path","slide_numbers"],
            "additionalProperties":false
        }),
    )
}

fn replace_pptx_text_tool() -> Value {
    tool(
        "replace_pptx_text",
        "Replace exact visible text only inside individual DrawingML text runs of selected slides, preserve run formatting and all unrelated package parts, and write a distinct PPTX output.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pptx path. The source is never modified."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pptx output path."},
                "find":{"type":"string","minLength":1,"maxLength":10000,"description":"Exact text to find inside a single visible DrawingML text run. Matches never cross runs."},
                "replacement":{"type":"string","maxLength":100000},
                "slide_numbers":{"type":"array","minItems":1,"maxItems":200,"uniqueItems":true,"items":{"type":"integer","minimum":1,"maximum":200},"description":"Optional one-based positions in presentation order. Omit to search all slides."},
                "max_replacements":{"type":"integer","minimum":1,"maximum":10000,"default":100},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path","find","replacement"],
            "additionalProperties":false
        }),
    )
}

fn replace_pptx_text_across_runs_tool() -> Value {
    tool(
        "replace_pptx_text_across_runs",
        "Replace one globally unique visible-text selection spanning 2 to 16 adjacent simple DrawingML runs with identical run properties inside one paragraph, preserve unrelated package content, and write a distinct PPTX output. Complex or ambiguous matches fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pptx path. The source is never modified."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pptx output path."},
                "selection":{"type":"string","minLength":1,"maxLength":10000,"description":"Exact visible text that must occur once across 2 to 16 adjacent simple same-format DrawingML runs in one paragraph."},
                "replacement":{"type":"string","maxLength":100000},
                "slide_numbers":{"type":"array","minItems":1,"maxItems":200,"uniqueItems":true,"items":{"type":"integer","minimum":1,"maximum":200},"description":"Optional one-based positions in current visible presentation order. Omit to search all slides."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path","selection","replacement"],
            "additionalProperties":false
        }),
    )
}

fn replace_pptx_table_cell_text_tool() -> Value {
    tool(
        "replace_pptx_table_cell_text",
        "Replace the exact text of one addressed cell in a conservative rectangular DrawingML table, preserving table, cell, paragraph, and run formatting plus all unrelated package parts in a distinct PPTX output.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pptx path. The source is never modified."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pptx output path."},
                "slide_number":{"type":"integer","minimum":1,"maximum":200,"description":"One-based position in true visible presentation order."},
                "table_number":{"type":"integer","minimum":1,"maximum":100,"description":"One-based DrawingML table order within the selected slide."},
                "row":{"type":"integer","minimum":1,"maximum":500,"description":"One-based physical row index in the simple rectangular table."},
                "column":{"type":"integer","minimum":1,"maximum":64,"description":"One-based physical column index in the simple rectangular table."},
                "expected_text":{"type":"string","maxLength":10000,"description":"Complete current visible text of the addressed cell, as returned by inspect_pptx_table for eligible cells."},
                "replacement":{"type":"string","maxLength":10000},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path","slide_number","table_number","row","column","expected_text","replacement"],
            "additionalProperties":false
        }),
    )
}

fn copy_pptx_table_cell_format_tool() -> Value {
    tool(
        "copy_pptx_table_cell_format",
        "Copy the complete formatting of one eligible simple DrawingML table cell onto a different cell in the same table while preserving the target cell text. Exact text and full-cell XML SHA-256 snapshots for both cells are required; the source deck and unrelated package parts remain unchanged.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pptx path. The source is never modified."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pptx output path."},
                "slide_number":{"type":"integer","minimum":1,"maximum":200,"description":"One-based position in true visible presentation order."},
                "table_number":{"type":"integer","minimum":1,"maximum":100,"description":"One-based DrawingML table order within the selected slide."},
                "row":{"type":"integer","minimum":1,"maximum":500,"description":"One-based physical row index of the target cell whose text must remain unchanged."},
                "column":{"type":"integer","minimum":1,"maximum":64,"description":"One-based physical column index of the target cell."},
                "expected_text":{"type":"string","maxLength":10000,"description":"Complete current visible text of the target cell."},
                "expected_cell_xml_sha256":{"type":"string","pattern":"^[0-9a-f]{64}$","description":"Exact target cell XML SHA-256 returned at the same address by inspect_pptx_table."},
                "reference_row":{"type":"integer","minimum":1,"maximum":500,"description":"One-based physical row index of the reference cell whose complete formatting is copied."},
                "reference_column":{"type":"integer","minimum":1,"maximum":64,"description":"One-based physical column index of the reference cell."},
                "reference_expected_text":{"type":"string","maxLength":10000,"description":"Complete current visible text of the reference cell."},
                "reference_expected_cell_xml_sha256":{"type":"string","pattern":"^[0-9a-f]{64}$","description":"Exact reference cell XML SHA-256 returned at the same address by inspect_pptx_table."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path","slide_number","table_number","row","column","expected_text","expected_cell_xml_sha256","reference_row","reference_column","reference_expected_text","reference_expected_cell_xml_sha256"],
            "additionalProperties":false
        }),
    )
}

fn delete_pptx_table_row_tool() -> Value {
    tool(
        "delete_pptx_table_row",
        "Delete one addressed row from an eligible simple rectangular DrawingML table after complete expected-cells verification, transfer its height to an adjacent row, preserve the table frame and unrelated package parts, and write a distinct PPTX output.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pptx path. The source is never modified."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pptx output path."},
                "slide_number":{"type":"integer","minimum":1,"maximum":200,"description":"One-based position in true visible presentation order."},
                "table_number":{"type":"integer","minimum":1,"maximum":100,"description":"One-based DrawingML table order within the selected slide."},
                "row":{"type":"integer","minimum":1,"maximum":500,"description":"One-based physical row index to delete. The only row cannot be deleted."},
                "expected_cells":{"type":"array","minItems":1,"maxItems":64,"items":{"type":"string","maxLength":10000},"description":"Complete current visible text of every physical cell in the selected row, in order."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path","slide_number","table_number","row","expected_cells"],
            "additionalProperties":false
        }),
    )
}

fn insert_pptx_table_row_tool() -> Value {
    tool(
        "insert_pptx_table_row",
        "Insert one row before or after an addressed reference row in an eligible simple rectangular DrawingML table after complete expected-cells verification. Clone the reference cell/paragraph/run formatting, split its row height to preserve the table frame, and write a distinct PPTX output.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pptx path. The source is never modified."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pptx output path."},
                "slide_number":{"type":"integer","minimum":1,"maximum":200,"description":"One-based position in true visible presentation order."},
                "table_number":{"type":"integer","minimum":1,"maximum":100,"description":"One-based DrawingML table order within the selected slide."},
                "reference_row":{"type":"integer","minimum":1,"maximum":500,"description":"One-based physical reference row whose formatting and height are used."},
                "position":{"type":"string","enum":["before","after"]},
                "expected_cells":{"type":"array","minItems":1,"maxItems":64,"items":{"type":"string","maxLength":10000},"description":"Complete current visible text of every physical cell in the reference row, in order."},
                "cells":{"type":"array","minItems":1,"maxItems":64,"items":{"type":"string","maxLength":10000},"description":"Complete text for every inserted physical cell. The count must match the table grid."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path","slide_number","table_number","reference_row","position","expected_cells","cells"],
            "additionalProperties":false
        }),
    )
}

fn move_pptx_table_row_tool() -> Value {
    tool(
        "move_pptx_table_row",
        "Move one addressed row immediately before or after another row in the same eligible simple rectangular DrawingML table after complete source and reference expected-cells verification. Preserve exact row XML, row height, formatting, table frame, and unrelated package parts in a distinct PPTX output.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pptx path. The source is never modified."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pptx output path."},
                "slide_number":{"type":"integer","minimum":1,"maximum":200,"description":"One-based position in true visible presentation order."},
                "table_number":{"type":"integer","minimum":1,"maximum":100,"description":"One-based DrawingML table order within the selected slide."},
                "row":{"type":"integer","minimum":1,"maximum":500,"description":"One-based physical source row index in the original table."},
                "expected_cells":{"type":"array","minItems":1,"maxItems":64,"items":{"type":"string","maxLength":10000},"description":"Complete current visible text of every physical cell in the source row, in order."},
                "reference_row":{"type":"integer","minimum":1,"maximum":500,"description":"One-based physical reference row index in the original table."},
                "reference_expected_cells":{"type":"array","minItems":1,"maxItems":64,"items":{"type":"string","maxLength":10000},"description":"Complete current visible text of every physical cell in the reference row, in order."},
                "position":{"type":"string","enum":["before","after"]},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path","slide_number","table_number","row","expected_cells","reference_row","reference_expected_cells","position"],
            "additionalProperties":false
        }),
    )
}

fn delete_pptx_table_column_tool() -> Value {
    tool(
        "delete_pptx_table_column",
        "Delete one addressed column from an eligible simple rectangular DrawingML table after complete expected-cells verification, transfer its width to an adjacent grid column, preserve the table frame and unrelated package parts, and write a distinct PPTX output.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pptx path. The source is never modified."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pptx output path."},
                "slide_number":{"type":"integer","minimum":1,"maximum":200,"description":"One-based position in true visible presentation order."},
                "table_number":{"type":"integer","minimum":1,"maximum":100,"description":"One-based DrawingML table order within the selected slide."},
                "column":{"type":"integer","minimum":1,"maximum":64,"description":"One-based physical column index to delete. The only column cannot be deleted."},
                "expected_cells":{"type":"array","minItems":1,"maxItems":500,"items":{"type":"string","maxLength":10000},"description":"Complete current visible text of every physical cell in the selected column, from first row to last row."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path","slide_number","table_number","column","expected_cells"],
            "additionalProperties":false
        }),
    )
}

fn insert_pptx_table_column_tool() -> Value {
    tool(
        "insert_pptx_table_column",
        "Insert one column before or after an addressed reference column in an eligible simple rectangular DrawingML table after complete expected-cells verification. Clone each reference cell's formatting, split the reference grid width to preserve the table frame, and write a distinct PPTX output.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pptx path. The source is never modified."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pptx output path."},
                "slide_number":{"type":"integer","minimum":1,"maximum":200,"description":"One-based position in true visible presentation order."},
                "table_number":{"type":"integer","minimum":1,"maximum":100,"description":"One-based DrawingML table order within the selected slide."},
                "reference_column":{"type":"integer","minimum":1,"maximum":64,"description":"One-based physical reference column whose cell formatting and grid width are used."},
                "position":{"type":"string","enum":["before","after"]},
                "expected_cells":{"type":"array","minItems":1,"maxItems":500,"items":{"type":"string","maxLength":10000},"description":"Complete current visible text of every physical cell in the reference column, from first row to last row."},
                "cells":{"type":"array","minItems":1,"maxItems":500,"items":{"type":"string","maxLength":10000},"description":"Complete text for every inserted physical cell. The count must match the table row count."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path","slide_number","table_number","reference_column","position","expected_cells","cells"],
            "additionalProperties":false
        }),
    )
}

fn move_pptx_table_column_tool() -> Value {
    tool(
        "move_pptx_table_column",
        "Move one addressed column immediately before or after another column in the same eligible simple rectangular DrawingML table after complete source and reference expected-cells verification. Preserve exact grid-column and cell XML, widths, formatting, table frame, and unrelated package parts in a distinct PPTX output.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pptx path. The source is never modified."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pptx output path."},
                "slide_number":{"type":"integer","minimum":1,"maximum":200,"description":"One-based position in true visible presentation order."},
                "table_number":{"type":"integer","minimum":1,"maximum":100,"description":"One-based DrawingML table order within the selected slide."},
                "column":{"type":"integer","minimum":1,"maximum":64,"description":"One-based physical source column index in the original table."},
                "expected_cells":{"type":"array","minItems":1,"maxItems":500,"items":{"type":"string","maxLength":10000},"description":"Complete current visible text of every physical cell in the source column, from first row to last row."},
                "reference_column":{"type":"integer","minimum":1,"maximum":64,"description":"One-based physical reference column index in the original table."},
                "reference_expected_cells":{"type":"array","minItems":1,"maxItems":500,"items":{"type":"string","maxLength":10000},"description":"Complete current visible text of every physical cell in the reference column, from first row to last row."},
                "position":{"type":"string","enum":["before","after"]},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path","slide_number","table_number","column","expected_cells","reference_column","reference_expected_cells","position"],
            "additionalProperties":false
        }),
    )
}

fn replace_pptx_notes_text_tool() -> Value {
    tool(
        "replace_pptx_notes_text",
        "Replace exact speaker-note text only inside individual DrawingML text runs of selected slides, preserve visible slide content and all unrelated package parts, and write a distinct PPTX output.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pptx path. The source is never modified."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pptx output path."},
                "find":{"type":"string","minLength":1,"maxLength":10000,"description":"Exact text to find inside a single speaker-note DrawingML text run. Matches never cross runs, shapes, or notes pages."},
                "replacement":{"type":"string","maxLength":100000},
                "slide_numbers":{"type":"array","minItems":1,"maxItems":200,"uniqueItems":true,"items":{"type":"integer","minimum":1,"maximum":200},"description":"Optional one-based positions in current visible presentation order. Omit to search notes owned by all slides."},
                "max_replacements":{"type":"integer","minimum":1,"maximum":10000,"default":100},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path","find","replacement"],
            "additionalProperties":false
        }),
    )
}
