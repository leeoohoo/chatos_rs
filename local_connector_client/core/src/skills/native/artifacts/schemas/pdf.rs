// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use super::tool;

pub(super) fn tool_definitions() -> Vec<Value> {
    vec![
        inspect_pdf_tool(),
        extract_pdf_text_tool(),
        render_pdf_pages_tool(),
        export_pdf_pages_to_png_tool(),
        create_text_pdf_tool(),
        create_pdf_from_images_tool(),
        update_pdf_metadata_tool(),
        fill_pdf_form_fields_tool(),
        merge_pdfs_tool(),
        extract_pdf_pages_tool(),
        arrange_pdf_pages_tool(),
        rotate_pdf_pages_tool(),
    ]
}

fn inspect_pdf_tool() -> Value {
    tool(
        "inspect_pdf",
        "Inspect a PDF inside the authorized local workspace and report its page count, metadata, annotations, forms, and optional exact page geometry and rotation for coordinate-bound annotation work.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string"},
                "page_geometry":{"type":"integer","minimum":1,"maximum":5000,"description":"Optional one-based physical page whose effective CropBox/MediaBox bounds and rotation should be returned."},
                "annotation_page":{"type":"integer","minimum":1,"maximum":5000,"description":"Optional one-based physical page used to focus the bounded annotation preview for exact reply targeting."}
            },
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

fn render_pdf_pages_tool() -> Value {
    tool(
        "render_pdf_pages",
        "Validate one regular non-symlink workspace PDF and use the packaged, manifest-verified Poppler runtime to attach a bounded page range as transient PNG model input for visual QA. Rendering does not itself claim that visual review passed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pdf path."},
                "first_page":{"type":"integer","minimum":1,"maximum":500,"default":1},
                "last_page":{"type":"integer","minimum":1,"maximum":500,"description":"Inclusive final page. At most 8 pages may be attached per call; omit to render up to 8 pages from first_page."},
                "dpi":{"type":"integer","minimum":96,"maximum":160,"default":120},
                "timeout_seconds":{"type":"integer","minimum":15,"maximum":180,"default":120}
            },
            "required":["path"],
            "additionalProperties":false
        }),
    )
}

fn export_pdf_pages_to_png_tool() -> Value {
    tool(
        "export_pdf_pages_to_png",
        "Validate one regular non-symlink workspace PDF and use the packaged, manifest-verified Poppler runtime to persist a bounded physical page range as PNG files in one new workspace directory. The complete batch is rendered and validated before output begins; existing directories are never replaced, and a handled write failure or cancellation rolls back the new directory.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pdf path."},
                "target_directory":{"type":"string","description":"Workspace-relative output directory. It must not already exist; the tool creates it and never replaces existing files or directories."},
                "first_page":{"type":"integer","minimum":1,"maximum":500,"default":1},
                "last_page":{"type":"integer","minimum":1,"maximum":500,"description":"Inclusive final physical page. At most 50 pages may be exported per call; omit to export up to 50 pages from first_page."},
                "dpi":{"type":"integer","minimum":96,"maximum":300,"default":150},
                "filename_prefix":{"type":"string","minLength":1,"maxLength":64,"pattern":"^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$","default":"page","description":"Safe ASCII prefix. Output names are <prefix>-<physical-page-number>.png."},
                "timeout_seconds":{"type":"integer","minimum":15,"maximum":300,"default":180}
            },
            "required":["path","target_directory"],
            "additionalProperties":false
        }),
    )
}

fn create_text_pdf_tool() -> Value {
    tool(
        "create_text_pdf",
        "Create a bounded text PDF locally with A4 or Letter pages, automatic wrapping and pagination, metadata, and optional page numbers. This release intentionally accepts printable ASCII only until a licensed embedded Unicode font is bundled.",
        json!({
            "type":"object",
            "properties":{
                "target_path":{"type":"string","description":"Workspace-relative .pdf output path."},
                "title":{"type":"string","maxLength":1000},
                "paragraphs":{
                    "type":"array",
                    "minItems":1,
                    "maxItems":2000,
                    "items":{"type":"string","maxLength":100000}
                },
                "page_size":{"type":"string","enum":["a4","letter"],"default":"a4"},
                "font_size":{"type":"number","minimum":8,"maximum":24,"default":11},
                "title_font_size":{"type":"number","minimum":12,"maximum":36,"default":20},
                "line_spacing":{"type":"number","minimum":1,"maximum":2,"default":1.25},
                "margin_points":{"type":"number","minimum":24,"maximum":144,"default":54},
                "page_numbers":{"type":"boolean","default":true},
                "author":{"type":"string","maxLength":256},
                "subject":{"type":"string","maxLength":1000},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["target_path","paragraphs"],
            "additionalProperties":false
        }),
    )
}

fn create_pdf_from_images_tool() -> Value {
    tool(
        "create_pdf_from_images",
        "Create a bounded multi-page PDF from 1-100 regular non-symlink workspace PNG or JPEG images. Each image becomes one page in input order, with image-sized, A4, or Letter pages and contain/cover fitting. Source images are never modified.",
        json!({
            "type":"object",
            "properties":{
                "image_paths":{
                    "type":"array",
                    "minItems":1,
                    "maxItems":100,
                    "items":{"type":"string","description":"Workspace-relative PNG/JPG/JPEG path. Each image is limited to 10 MiB, 10000 px per edge, and 16 megapixels."}
                },
                "target_path":{"type":"string","description":"Distinct workspace-relative .pdf output path."},
                "page_size":{"type":"string","enum":["image","a4","letter"],"default":"image","description":"image uses one PDF point per source pixel plus margins; A4 and Letter use portrait pages."},
                "fit":{"type":"string","enum":["contain","cover"],"default":"contain","description":"contain preserves the whole image; cover centers and clips it to the page content box."},
                "margin_points":{"type":"number","minimum":0,"maximum":144,"default":0},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["image_paths","target_path"],
            "additionalProperties":false
        }),
    )
}

fn update_pdf_metadata_tool() -> Value {
    tool(
        "update_pdf_metadata",
        "Set or remove bounded standard PDF Document Info title, author, subject, and keywords fields while preserving all unrelated Info entries and the source file. Unicode is encoded as a PDF text string.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pdf path."},
                "title":{"type":"string","minLength":1,"maxLength":1000},
                "author":{"type":"string","minLength":1,"maxLength":256},
                "subject":{"type":"string","minLength":1,"maxLength":1000},
                "keywords":{"type":"string","minLength":1,"maxLength":2000},
                "remove_fields":{
                    "type":"array",
                    "minItems":1,
                    "maxItems":4,
                    "uniqueItems":true,
                    "items":{"type":"string","enum":["title","author","subject","keywords"]}
                },
                "target_path":{"type":"string","description":"Distinct workspace-relative .pdf output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path"],
            "additionalProperties":false
        }),
    )
}

fn fill_pdf_form_fields_tool() -> Value {
    tool(
        "fill_pdf_form_fields",
        "Safely fill bounded standard AcroForm text, checkbox, radio, editable or fixed single-select choice, and multi-select list fields in a distinct output PDF. Every update is bound to the exact current value; XFA, signatures, password/file/rich-text fields, push buttons, invalid choice flag combinations, read-only fields, ambiguous names, and malformed widget appearances fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pdf path."},
                "fields":{
                    "type":"array",
                    "minItems":1,
                    "maxItems":200,
                    "items":{
                        "type":"object",
                        "properties":{
                            "name":{"type":"string","minLength":1,"maxLength":512,"description":"Exact fully qualified field name returned by inspect_pdf.form.preview."},
                            "expected_value":{"oneOf":[{"type":"string","maxLength":16384},{"type":"boolean"},{"type":"array","maxItems":500,"uniqueItems":true,"items":{"type":"string","maxLength":1024}},{"type":"null"}],"description":"Exact current value returned by inspect_pdf; protects against stale or unintended writes. Multi-select choices use an array in exact option order."},
                            "value":{"oneOf":[{"type":"string","maxLength":16384},{"type":"boolean"},{"type":"array","maxItems":500,"uniqueItems":true,"items":{"type":"string","maxLength":1024}},{"type":"null"}],"description":"New exact value. Fixed radio/choice values must match inspected options; editable combos accept bounded text; multi-select choices use an exact-order array."}
                        },
                        "required":["name","expected_value","value"],
                        "additionalProperties":false
                    }
                },
                "target_path":{"type":"string","description":"Distinct workspace-relative .pdf output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","fields","target_path"],
            "additionalProperties":false
        }),
    )
}

fn merge_pdfs_tool() -> Value {
    tool(
        "merge_pdfs",
        "Merge 2 to 20 unencrypted PDFs from the authorized workspace into a distinct local PDF output. Inputs are limited to 200 MiB and 5000 pages combined.",
        json!({
            "type":"object",
            "properties":{
                "paths":{
                    "type":"array",
                    "minItems":2,
                    "maxItems":20,
                    "items":{"type":"string"}
                },
                "target_path":{"type":"string"},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["paths","target_path"],
            "additionalProperties":false
        }),
    )
}

fn extract_pdf_pages_tool() -> Value {
    tool(
        "extract_pdf_pages",
        "Create a new PDF from an ascending set of page numbers in an unencrypted workspace PDF. The source is never modified in place.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string"},
                "pages":{
                    "type":"array",
                    "minItems":1,
                    "maxItems":5000,
                    "uniqueItems":true,
                    "items":{"type":"integer","minimum":1}
                },
                "target_path":{"type":"string"},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","pages","target_path"],
            "additionalProperties":false
        }),
    )
}

fn rotate_pdf_pages_tool() -> Value {
    tool(
        "rotate_pdf_pages",
        "Create a new PDF with all pages, or an ascending selected page set, rotated clockwise by 90, 180, or 270 degrees. The source is never modified in place.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string"},
                "pages":{
                    "type":"array",
                    "minItems":1,
                    "maxItems":5000,
                    "uniqueItems":true,
                    "items":{"type":"integer","minimum":1}
                },
                "angle":{"type":"integer","enum":[90,180,270]},
                "target_path":{"type":"string"},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","angle","target_path"],
            "additionalProperties":false
        }),
    )
}

fn arrange_pdf_pages_tool() -> Value {
    tool(
        "arrange_pdf_pages",
        "Create a new PDF whose pages appear exactly in the requested unique order, optionally deleting omitted pages. Complex navigation, form, tagged-document, page-label, or annotation structures fail closed, and the source is never modified in place.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string"},
                "pages":{
                    "type":"array",
                    "minItems":1,
                    "maxItems":5000,
                    "uniqueItems":true,
                    "description":"One-based source page numbers in the exact desired output order.",
                    "items":{"type":"integer","minimum":1}
                },
                "target_path":{"type":"string"},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","pages","target_path"],
            "additionalProperties":false
        }),
    )
}
