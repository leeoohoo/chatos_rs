// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

pub(super) fn tool_definitions(skill_id: &str) -> Vec<Value> {
    match skill_id {
        "internal_skill_pdf" => vec![
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
            add_pdf_text_annotation_tool(),
            add_pdf_markup_annotation_tool(),
            add_pdf_link_annotation_tool(),
            add_pdf_annotation_reply_tool(),
            update_pdf_annotation_text_tool(),
            delete_pdf_annotation_tool(),
            add_pdf_file_attachment_annotation_tool(),
            extract_pdf_file_attachment_tool(),
            extract_pdf_embedded_file_tool(),
            stamp_pdf_text_tool(),
            stamp_pdf_page_numbers_tool(),
            stamp_pdf_image_tool(),
        ],
        "internal_skill_documents" => vec![
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
        ],
        "internal_skill_spreadsheets" => vec![
            inspect_spreadsheet_tool(),
            render_spreadsheet_pages_tool(),
            create_xlsx_tool(),
            update_xlsx_range_tool(),
            create_csv_tool(),
            update_csv_range_tool(),
            create_tsv_tool(),
            update_tsv_range_tool(),
        ],
        "internal_skill_presentations" => vec![
            inspect_pptx_tool(),
            inspect_pptx_charts_tool(),
            replace_pptx_chart_tool(),
            inspect_pptx_table_tool(),
            render_presentation_pages_tool(),
            create_pptx_tool(),
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
        ],
        "internal_skill_template_creator" => vec![
            inspect_artifact_template_tool(),
            create_artifact_template_tool(),
            instantiate_artifact_template_tool(),
            render_artifact_template_preview_tool(),
        ],
        _ => Vec::new(),
    }
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

fn add_pdf_text_annotation_tool() -> Value {
    tool(
        "add_pdf_text_annotation",
        "Add one bounded standard PDF Text annotation to an unrotated physical page while preserving existing annotations and the source file. Unicode contents and author text are encoded as PDF text strings; malformed annotation arrays fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pdf path."},
                "page":{"type":"integer","minimum":1,"maximum":5000,"description":"One-based physical page position."},
                "text":{"type":"string","minLength":1,"maxLength":4096,"description":"Annotation contents. Unicode is supported; unsafe control characters are rejected."},
                "author":{"type":"string","minLength":1,"maxLength":256},
                "position":{"type":"string","enum":["top_left","top_right","bottom_left","bottom_right"],"default":"top_right"},
                "icon":{"type":"string","enum":["note","comment","help","key","paragraph","insert","new_paragraph"],"default":"comment"},
                "color":{"type":"string","enum":["yellow","blue","green","red"],"default":"yellow"},
                "size_points":{"type":"number","minimum":12,"maximum":72,"default":24},
                "margin_points":{"type":"number","minimum":12,"maximum":144,"default":36},
                "open":{"type":"boolean","default":false},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pdf output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","page","text","target_path"],
            "additionalProperties":false
        }),
    )
}

fn add_pdf_markup_annotation_tool() -> Value {
    tool(
        "add_pdf_markup_annotation",
        "Add one bounded standard PDF highlight, underline, strikeout, or squiggly markup annotation to an unrotated physical page. Geometry uses CropBox-relative PDF points and is checked against the exact page bounds; existing annotations and the source remain unchanged.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pdf path."},
                "page":{"type":"integer","minimum":1,"maximum":5000,"description":"One-based physical page position."},
                "markup":{"type":"string","enum":["highlight","underline","strikeout","squiggly"]},
                "rectangles":{
                    "type":"array",
                    "minItems":1,
                    "maxItems":64,
                    "description":"Axis-aligned text rectangles in PDF points relative to the effective page CropBox lower-left corner.",
                    "items":{
                        "type":"object",
                        "properties":{
                            "x":{"type":"number","minimum":0,"maximum":20000},
                            "y":{"type":"number","minimum":0,"maximum":20000},
                            "width":{"type":"number","minimum":0.1,"maximum":20000},
                            "height":{"type":"number","minimum":0.1,"maximum":20000}
                        },
                        "required":["x","y","width","height"],
                        "additionalProperties":false
                    }
                },
                "text":{"type":"string","minLength":1,"maxLength":4096,"description":"Optional Unicode annotation contents."},
                "author":{"type":"string","minLength":1,"maxLength":256},
                "color":{"type":"string","enum":["yellow","blue","green","red"],"default":"yellow"},
                "opacity":{"type":"number","minimum":0.05,"maximum":1,"default":0.35},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pdf output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","page","markup","rectangles","target_path"],
            "additionalProperties":false
        }),
    )
}

fn add_pdf_link_annotation_tool() -> Value {
    tool(
        "add_pdf_link_annotation",
        "Add one standard PDF Link annotation to an unrotated physical page using exact source SHA-256 binding and CropBox-relative geometry. Only credential-free HTTPS destinations or direct in-document Fit page destinations are created; JavaScript, Launch, file, remote-file, additional-action, chained-action, stale-source, and in-place paths fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pdf path."},
                "expected_source_sha256":{"type":"string","pattern":"^[0-9a-f]{64}$","description":"Exact source SHA-256 returned by inspect_pdf."},
                "page":{"type":"integer","minimum":1,"maximum":5000,"description":"One-based physical page receiving the Link annotation."},
                "x":{"type":"number","minimum":0,"maximum":20000,"description":"Left edge in PDF points relative to the effective CropBox lower-left corner."},
                "y":{"type":"number","minimum":0,"maximum":20000,"description":"Bottom edge in PDF points relative to the effective CropBox lower-left corner."},
                "width":{"type":"number","minimum":0.1,"maximum":20000},
                "height":{"type":"number","minimum":0.1,"maximum":20000},
                "destination_type":{"type":"string","enum":["https","page"]},
                "url":{"type":"string","minLength":1,"maxLength":2048,"description":"Credential-free HTTPS URL. Only valid when destination_type=https."},
                "destination_page":{"type":"integer","minimum":1,"maximum":5000,"description":"Existing one-based physical destination page. Only valid when destination_type=page."},
                "description":{"type":"string","minLength":1,"maxLength":4096,"description":"Optional bounded Unicode Link contents/accessibility description."},
                "author":{"type":"string","minLength":1,"maxLength":256},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pdf output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","expected_source_sha256","page","x","y","width","height","destination_type","target_path"],
            "additionalProperties":false
        }),
    )
}

fn add_pdf_annotation_reply_tool() -> Value {
    tool(
        "add_pdf_annotation_reply",
        "Append one standard Unicode PDF annotation reply to an inspected indirect Text or markup root annotation. The exact source SHA-256 and one-based page-local annotation index are required; direct annotations, replies-to-replies, stale sources, malformed relationships, and in-place output fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pdf path."},
                "expected_source_sha256":{"type":"string","pattern":"^[0-9a-f]{64}$","description":"Exact source SHA-256 returned by inspect_pdf."},
                "page":{"type":"integer","minimum":1,"maximum":5000,"description":"One-based physical page containing the inspected root annotation."},
                "annotation_index":{"type":"integer","minimum":1,"maximum":100,"description":"One-based page-local annotation index returned in the focused inspect_pdf annotation preview."},
                "text":{"type":"string","minLength":1,"maxLength":4096,"description":"Unicode reply contents. Bounded line breaks and tabs are supported."},
                "author":{"type":"string","minLength":1,"maxLength":256},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pdf output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","expected_source_sha256","page","annotation_index","text","target_path"],
            "additionalProperties":false
        }),
    )
}

fn update_pdf_annotation_text_tool() -> Value {
    tool(
        "update_pdf_annotation_text",
        "Update or remove the Unicode contents and author of one inspected PDF Text or markup annotation in a distinct output. The exact source SHA-256, physical page, page-local preview index, subtype, and root/reply/group relation are required; unsupported subtypes, stale snapshots, no-op updates, and in-place targets fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pdf path."},
                "expected_source_sha256":{"type":"string","pattern":"^[0-9a-f]{64}$","description":"Exact source SHA-256 returned by inspect_pdf."},
                "page":{"type":"integer","minimum":1,"maximum":5000,"description":"One-based physical page containing the inspected annotation."},
                "annotation_index":{"type":"integer","minimum":1,"maximum":100,"description":"One-based page-local annotation index returned by inspect_pdf(annotation_page=page)."},
                "expected_subtype":{"type":"string","enum":["Text","Highlight","Underline","StrikeOut","Squiggly"],"description":"Exact annotation subtype returned by the focused preview."},
                "expected_relation_type":{"type":"string","enum":["root","reply","group"],"description":"Use root when the preview has no relation_type; otherwise submit the exact reply or group relation."},
                "text":{"type":"string","minLength":1,"maxLength":4096,"description":"Optional replacement Unicode annotation contents. Line breaks and tabs are allowed."},
                "author":{"type":"string","minLength":1,"maxLength":256,"description":"Optional replacement Unicode annotation author."},
                "remove_fields":{"type":"array","minItems":1,"maxItems":2,"uniqueItems":true,"items":{"type":"string","enum":["text","author"]},"description":"Optional existing fields to remove."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pdf output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","expected_source_sha256","page","annotation_index","expected_subtype","expected_relation_type","target_path"],
            "additionalProperties":false
        }),
    )
}

fn delete_pdf_annotation_tool() -> Value {
    tool(
        "delete_pdf_annotation",
        "Delete one inspected standard PDF Text, markup, Link, or FileAttachment annotation from a distinct output using exact source SHA-256, physical page, page-local preview index, subtype, and relation binding. Widgets, unsupported subtypes, structure-tree membership, annotations still referenced by replies/groups/popups or other reachable objects, stale sources, and in-place targets fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pdf path."},
                "expected_source_sha256":{"type":"string","pattern":"^[0-9a-f]{64}$","description":"Exact source SHA-256 returned by inspect_pdf."},
                "page":{"type":"integer","minimum":1,"maximum":5000,"description":"One-based physical page containing the inspected annotation."},
                "annotation_index":{"type":"integer","minimum":1,"maximum":100,"description":"One-based page-local annotation index returned by inspect_pdf(annotation_page=page)."},
                "expected_subtype":{"type":"string","enum":["Text","Highlight","Underline","StrikeOut","Squiggly","Link","FileAttachment"],"description":"Exact annotation subtype returned by the focused preview."},
                "expected_relation_type":{"type":"string","enum":["root","reply","group"],"description":"Use root when the preview has no relation_type; otherwise submit the exact reply or group relation."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pdf output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","expected_source_sha256","page","annotation_index","expected_subtype","expected_relation_type","target_path"],
            "additionalProperties":false
        }),
    )
}

fn add_pdf_file_attachment_annotation_tool() -> Value {
    tool(
        "add_pdf_file_attachment_annotation",
        "Embed one bounded workspace file and append a standard PDF FileAttachment annotation to an unrotated physical page. The source PDF snapshot, attachment type and content signature, CropBox-relative geometry, indirect object chain, and distinct output are validated fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pdf path."},
                "expected_source_sha256":{"type":"string","pattern":"^[0-9a-f]{64}$","description":"Exact source SHA-256 returned by inspect_pdf."},
                "attachment_path":{"type":"string","description":"Workspace-relative regular non-symlink attachment path. Supported types: PDF, TXT, MD, CSV, JSON, DOCX, XLSX, PPTX, PNG, JPG, and JPEG."},
                "page":{"type":"integer","minimum":1,"maximum":5000,"description":"One-based physical page position."},
                "x":{"type":"number","minimum":0,"maximum":20000,"description":"Left edge in PDF points relative to the effective CropBox lower-left corner."},
                "y":{"type":"number","minimum":0,"maximum":20000,"description":"Bottom edge in PDF points relative to the effective CropBox lower-left corner."},
                "icon_size":{"type":"number","minimum":12,"maximum":72,"default":24,"description":"Square annotation icon size in PDF points."},
                "description":{"type":"string","minLength":1,"maxLength":4096,"description":"Optional Unicode attachment description stored in both the Filespec and annotation contents."},
                "author":{"type":"string","minLength":1,"maxLength":256},
                "icon":{"type":"string","enum":["graph","push_pin","paperclip","tag"],"default":"push_pin"},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pdf output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","expected_source_sha256","attachment_path","page","x","y","target_path"],
            "additionalProperties":false
        }),
    )
}

fn extract_pdf_file_attachment_tool() -> Value {
    tool(
        "extract_pdf_file_attachment",
        "Extract one inspected standard PDF FileAttachment to a distinct workspace file. Exact source and attachment SHA-256 values, the focused page-local annotation index, the complete indirect object chain, content signature, output extension, and atomic write are validated fail closed without returning attachment content.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pdf path."},
                "expected_source_sha256":{"type":"string","pattern":"^[0-9a-f]{64}$","description":"Exact source SHA-256 returned by inspect_pdf."},
                "page":{"type":"integer","minimum":1,"maximum":5000,"description":"One-based physical page containing the inspected FileAttachment annotation."},
                "annotation_index":{"type":"integer","minimum":1,"maximum":100,"description":"One-based page-local annotation index returned in the focused inspect_pdf annotation preview."},
                "expected_attachment_sha256":{"type":"string","pattern":"^[0-9a-f]{64}$","description":"Exact attachment SHA-256 returned in inspect_pdf annotation metadata."},
                "target_path":{"type":"string","description":"Distinct workspace-relative output path whose extension matches the inspected attachment."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","expected_source_sha256","page","annotation_index","expected_attachment_sha256","target_path"],
            "additionalProperties":false
        }),
    )
}

fn extract_pdf_embedded_file_tool() -> Value {
    tool(
        "extract_pdf_embedded_file",
        "Extract one inspected standard PDF Catalog Names/EmbeddedFiles entry to a distinct workspace file. Exact source and embedded-file SHA-256 values, the bounded inspection index, nested Name Tree structure, indirect Filespec/EmbeddedFile chain, content signature, output extension, and atomic write are validated fail closed without returning content.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pdf path."},
                "expected_source_sha256":{"type":"string","pattern":"^[0-9a-f]{64}$","description":"Exact source SHA-256 returned by inspect_pdf."},
                "embedded_file_index":{"type":"integer","minimum":1,"maximum":100,"description":"One-based index returned in inspect_pdf embedded_files.preview."},
                "expected_attachment_sha256":{"type":"string","pattern":"^[0-9a-f]{64}$","description":"Exact embedded-file SHA-256 returned by inspect_pdf."},
                "target_path":{"type":"string","description":"Distinct workspace-relative output path whose extension matches the inspected embedded file."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","expected_source_sha256","embedded_file_index","expected_attachment_sha256","target_path"],
            "additionalProperties":false
        }),
    )
}

fn stamp_pdf_text_tool() -> Value {
    tool(
        "stamp_pdf_text",
        "Overlay a bounded printable-ASCII text stamp on all pages, or an ascending selected page set, of an unencrypted workspace PDF. Supports safe positions, -45/0/45 degree rotation, opacity, and grayscale without modifying the source.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string"},
                "text":{"type":"string","minLength":1,"maxLength":256,"description":"Single-line printable ASCII stamp text."},
                "pages":{
                    "type":"array",
                    "minItems":1,
                    "maxItems":5000,
                    "uniqueItems":true,
                    "items":{"type":"integer","minimum":1}
                },
                "position":{"type":"string","enum":["top_left","top_center","top_right","center","bottom_left","bottom_center","bottom_right"],"default":"center"},
                "font_size":{"type":"number","minimum":8,"maximum":72,"default":24},
                "margin_points":{"type":"number","minimum":12,"maximum":144,"default":36},
                "rotation":{"type":"integer","enum":[-45,0,45],"default":0},
                "opacity":{"type":"number","minimum":0.05,"maximum":1,"default":0.25},
                "grayscale":{"type":"number","minimum":0,"maximum":1,"default":0.5},
                "target_path":{"type":"string"},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","text","target_path"],
            "additionalProperties":false
        }),
    )
}

fn stamp_pdf_page_numbers_tool() -> Value {
    tool(
        "stamp_pdf_page_numbers",
        "Overlay dynamic printable-ASCII page numbers on all pages, or an ascending selected page set, of an unencrypted workspace PDF. Labels are derived from physical one-based page positions and a bounded start number without modifying the source.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pdf path."},
                "pages":{"type":"array","minItems":1,"maxItems":5000,"uniqueItems":true,"items":{"type":"integer","minimum":1},"description":"Optional ascending physical page positions to stamp. Omit to stamp every page."},
                "format":{"type":"string","enum":["number","page_number","page_number_of_total"],"default":"page_number_of_total"},
                "start_number":{"type":"integer","minimum":1,"maximum":1000000,"default":1,"description":"Displayed number assigned to physical page 1. Selected pages keep their physical-position offset."},
                "position":{"type":"string","enum":["top_left","top_center","top_right","bottom_left","bottom_center","bottom_right"],"default":"bottom_center"},
                "font_size":{"type":"number","minimum":8,"maximum":24,"default":10},
                "margin_points":{"type":"number","minimum":12,"maximum":144,"default":36},
                "opacity":{"type":"number","minimum":0.05,"maximum":1,"default":1},
                "grayscale":{"type":"number","minimum":0,"maximum":1,"default":0},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pdf output path."},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path"],
            "additionalProperties":false
        }),
    )
}

fn stamp_pdf_image_tool() -> Value {
    tool(
        "stamp_pdf_image",
        "Overlay one bounded PNG or JPEG image on all pages, or an ascending selected page set, of an unencrypted workspace PDF. Supports aspect-ratio-preserving size, safe positions, bounded rotation and opacity without modifying either source file.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string"},
                "image_path":{"type":"string","description":"Workspace-relative PNG/JPG/JPEG, at most 10 MiB, 10000 px per edge and 16 megapixels."},
                "pages":{
                    "type":"array",
                    "minItems":1,
                    "maxItems":5000,
                    "uniqueItems":true,
                    "items":{"type":"integer","minimum":1}
                },
                "position":{"type":"string","enum":["top_left","top_center","top_right","center","bottom_left","bottom_center","bottom_right"],"default":"center"},
                "width_points":{"type":"number","minimum":12,"maximum":1000,"default":144},
                "margin_points":{"type":"number","minimum":12,"maximum":144,"default":36},
                "rotation":{"type":"integer","enum":[-90,-45,0,45,90],"default":0},
                "opacity":{"type":"number","minimum":0.05,"maximum":1,"default":1},
                "target_path":{"type":"string"},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","image_path","target_path"],
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

fn inspect_pptx_tool() -> Value {
    tool(
        "inspect_pptx",
        "Inspect a PPTX presentation locally and report widescreen dimensions, ordered slide text previews, image relationships, table summaries, media files, and speaker-note previews.",
        path_only_schema(),
    )
}

fn inspect_pptx_charts_tool() -> Value {
    tool(
        "inspect_pptx_charts",
        "Inspect standard internally related DrawingML chart parts in true visible slide order, returning chart type, raw bar direction and radar style, title, legend state and position, data-label mode, literal or formula-backed axis titles, recognized or custom per-series RGB colors, bounded line-marker style/size, and line smoothing with raw values, formulas, bounded cached series/category/value previews, full chart XML SHA-256, embedded-workbook metadata, and an exact edit snapshot only when the chart is byte-identical to ChatOS canonical self-contained output. External, shared, missing, unreferenced, chartEx, or over-limit chart structures fail closed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pptx path."},
                "slide_numbers":{"type":"array","minItems":1,"maxItems":200,"uniqueItems":true,"items":{"type":"integer","minimum":1,"maximum":200},"description":"Optional one-based positions in true visible presentation order. Omit to inspect charts on every slide."}
            },
            "required":["path"],
            "additionalProperties":false
        }),
    )
}

fn replace_pptx_chart_tool() -> Value {
    tool(
        "replace_pptx_chart",
        "Replace one uniquely owned standard chart only when its XML is byte-identical to the canonical self-contained column/bar/line/pie/area/doughnut/radar form generated by ChatOS. The bounded replacement may also update per-series RGB colors, canonical line-marker style/size and smoothing, legend position, value/percentage data labels, and axis titles where supported. Requires the exact chart XML SHA-256 and complete edit snapshot from inspect_pptx_charts, writes a distinct PPTX, and preserves the slide frame, relationship, chart part name, and every unrelated package entry.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pptx path. The source is never modified."},
                "target_path":{"type":"string","description":"Distinct workspace-relative .pptx output path."},
                "slide_number":{"type":"integer","minimum":1,"maximum":200,"description":"One-based position in true visible presentation order."},
                "chart_number":{"type":"integer","minimum":1,"maximum":50,"description":"One-based standard chart order within the selected slide."},
                "expected_chart_xml_sha256":{"type":"string","pattern":"^[0-9a-f]{64}$","description":"Exact full chart XML SHA-256 returned at the same address by inspect_pptx_charts."},
                "expected_self_contained_edit_snapshot":self_contained_pptx_chart_schema(
                    "Complete canonical chart snapshot returned by inspect_pptx_charts. Every field is required and stale or altered snapshots fail closed.",
                    true,
                ),
                "replacement":self_contained_pptx_chart_schema(
                    "Replacement chart within the bounded self-contained column, bar, line, pie, area, doughnut, or radar contract.",
                    false,
                ),
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["path","target_path","slide_number","chart_number","expected_chart_xml_sha256","expected_self_contained_edit_snapshot","replacement"],
            "additionalProperties":false
        }),
    )
}

fn inspect_pptx_table_tool() -> Value {
    tool(
        "inspect_pptx_table",
        "Inspect one DrawingML table by visible slide position and table order, returning bounded row/column cell-text previews, exact full-cell XML SHA-256 snapshots for eligible simple cells, and conservative eligibility for cell replacement, cell-format copying, and row/column structure edits.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pptx path."},
                "slide_number":{"type":"integer","minimum":1,"maximum":200,"description":"One-based position in true visible presentation order."},
                "table_number":{"type":"integer","minimum":1,"maximum":100,"description":"One-based DrawingML table order within the selected slide."}
            },
            "required":["path","slide_number","table_number"],
            "additionalProperties":false
        }),
    )
}

fn render_presentation_pages_tool() -> Value {
    tool(
        "render_presentation_pages",
        "Validate one regular non-symlink workspace PPTX, reject active, embedded, or externally connected content, convert it with the packaged manifest-verified LibreOffice runtime, and attach a bounded visible-slide range as transient PNG model input for visual QA. Rendering does not itself claim that visual review passed.",
        json!({
            "type":"object",
            "properties":{
                "path":{"type":"string","description":"Workspace-relative source .pptx path."},
                "first_slide":{"type":"integer","minimum":1,"maximum":200,"default":1,"description":"First slide in true visible presentation order."},
                "last_slide":{"type":"integer","minimum":1,"maximum":200,"description":"Inclusive final slide in true visible presentation order. At most 8 slides may be attached per call; omit to render up to 8 slides from first_slide."},
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

fn create_pptx_tool() -> Value {
    let chart_schema = self_contained_pptx_chart_schema(
        "Required only for chart layout. Creates one self-contained editable standard DrawingML chart with literal cached data and no embedded workbook, external data, formulas, macros, or executable content.",
        false,
    );
    tool(
        "create_pptx",
        "Create a bounded editable widescreen PPTX locally with title/body, title-only, section, two-column, image-right, full-image, simple rectangular table, or self-contained standard DrawingML column/bar/line/pie/area/doughnut/radar chart layouts with optional canonical RGB series colors, bounded line-marker styles, and line smoothing, optional PNG/JPEG images, bullet lines, and speaker notes.",
        json!({
            "type":"object",
            "properties":{
                "target_path":{"type":"string","description":"Workspace-relative .pptx output path."},
                "slides":{
                    "type":"array",
                    "minItems":1,
                    "maxItems":200,
                    "items":{
                        "type":"object",
                        "properties":{
                            "title":{"type":"string","maxLength":1000},
                            "layout":{"type":"string","enum":["title_body","title_only","section","two_column","image_right","image_full","table","chart"],"default":"title_body"},
                            "body":{"type":"string","maxLength":100000,"description":"Body or subtitle text. Lines beginning with '- ' or '* ' become bullets."},
                            "left_body":{"type":"string","maxLength":100000,"description":"Left-column text for two_column layout."},
                            "right_body":{"type":"string","maxLength":100000,"description":"Right-column text for two_column layout."},
                            "notes":{"type":"string","maxLength":100000,"description":"Optional editable speaker notes."},
                            "table":{
                                "type":"object",
                                "description":"Required only for table layout. Creates one simple editable rectangular DrawingML table below the title.",
                                "properties":{
                                    "cells":{
                                        "type":"array",
                                        "minItems":1,
                                        "maxItems":50,
                                        "items":{
                                            "type":"array",
                                            "minItems":1,
                                            "maxItems":20,
                                            "items":{"type":"string","maxLength":10000}
                                        }
                                    },
                                    "header_row":{"type":"boolean","default":true,"description":"Bold and style the first row as a header."}
                                },
                                "required":["cells"],
                                "additionalProperties":false
                            },
                            "chart": chart_schema,
                            "image":{
                                "type":"object",
                                "properties":{
                                    "path":{"type":"string","description":"Workspace-relative .png, .jpg, or .jpeg file, at most 10 MiB and 40 megapixels."},
                                    "alt_text":{"type":"string","minLength":1,"maxLength":1024,"default":"Presentation image"},
                                    "fit":{"type":"string","enum":["contain","cover"],"default":"contain"}
                                },
                                "required":["path"],
                                "additionalProperties":false
                            }
                        },
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

fn self_contained_pptx_chart_schema(description: &str, complete_snapshot: bool) -> Value {
    let required = if complete_snapshot {
        json!([
            "type",
            "title",
            "categories",
            "series",
            "show_legend",
            "legend_position",
            "data_labels",
            "category_axis_title",
            "value_axis_title",
            "secondary_value_axis_title",
            "value_axis_minimum",
            "value_axis_maximum",
            "value_axis_log_base",
            "value_axis_major_tick_mark",
            "value_axis_minor_tick_mark",
            "value_axis_major_unit",
            "value_axis_minor_unit",
            "value_axis_number_format",
            "secondary_value_axis_minimum",
            "secondary_value_axis_maximum",
            "secondary_value_axis_log_base",
            "secondary_value_axis_major_tick_mark",
            "secondary_value_axis_minor_tick_mark",
            "secondary_value_axis_major_unit",
            "secondary_value_axis_minor_unit",
            "secondary_value_axis_number_format"
        ])
    } else {
        json!(["type", "categories", "series"])
    };
    let series_required = if complete_snapshot {
        json!([
            "name",
            "values",
            "value_axis",
            "color",
            "marker_style",
            "marker_size",
            "smooth"
        ])
    } else {
        json!(["name", "values"])
    };
    json!({
        "type":"object",
        "description":description,
        "properties":{
            "type":{"type":"string","enum":["column","bar","line","pie","area","doughnut","radar"],"description":"column and bar create clustered vertical and horizontal bar charts respectively; line, pie, area, doughnut, and radar use bounded standard 2D forms. Radar uses exact standard style without markers. Pie and doughnut require exactly one non-negative series with at least one positive value."},
            "title":{"type":"string","maxLength":1000,"description":"Optional title inside the chart. The slide title remains a separate editable text box."},
            "categories":{"type":"array","minItems":1,"maxItems":50,"items":{"type":"string","minLength":1,"maxLength":1000}},
            "series":{
                "type":"array",
                "minItems":1,
                "maxItems":10,
                "items":{
                    "type":"object",
                    "properties":{
                        "name":{"type":"string","minLength":1,"maxLength":1000},
                        "values":{"type":"array","minItems":1,"maxItems":50,"items":{"type":"number","minimum":-1000000000000i64,"maximum":1000000000000i64}},
                        "value_axis":{"type":"string","enum":["primary","secondary"],"default":"primary","description":"Assign this series to the primary value axis (left for column/line/area/radar, bottom for bar) or canonical secondary value axis (right for column/line/area/radar, top for bar). Secondary is supported only for column, bar, line, area, or radar charts and requires at least one primary series."},
                        "color":{"type":["string","null"],"pattern":"^#[0-9A-Fa-f]{6}$","default":null,"description":"Optional canonical RGB series color in #RRGGBB form. Values are normalized to uppercase; line and radar charts use the series line color while other supported charts use the series fill color."},
                        "marker_style":{"type":["string","null"],"enum":["none","circle","square","diamond","triangle",null],"default":null,"description":"Optional canonical line-series marker style. Line charts default null to circle; non-line charts require null or omission."},
                        "marker_size":{"type":["integer","null"],"minimum":2,"maximum":72,"default":null,"description":"Optional line-series marker size. Non-none line markers default null to 5; marker_style=none and non-line charts require null or omission."},
                        "smooth":{"type":["boolean","null"],"default":null,"description":"Optional canonical line-series smoothing flag. Line charts default null to false; non-line charts require null or omission."}
                    },
                    "required":series_required,
                    "additionalProperties":false
                }
            },
            "show_legend":{"type":"boolean","default":true},
            "legend_position":{"type":"string","enum":["right","left","top","bottom"],"default":"right","description":"Canonical legend position. When show_legend=false this must remain right so hidden legend state has one unambiguous snapshot."},
            "data_labels":{"type":"string","enum":["none","value","percentage"],"default":"none","description":"Show no data labels, numeric values, or percentages. Percentage is supported only for pie and doughnut charts."},
            "category_axis_title":{"type":"string","maxLength":1000,"default":"","description":"Optional category-axis title for column, bar, line, area, or radar charts. Pie and doughnut charts reject axis titles."},
            "value_axis_title":{"type":"string","maxLength":1000,"default":"","description":"Optional primary value-axis title (left for column/line/area/radar, bottom for bar). Pie and doughnut charts reject axis titles."},
            "secondary_value_axis_title":{"type":"string","maxLength":1000,"default":"","description":"Optional secondary value-axis title (right for column/line/area/radar, top for bar). It requires at least one primary and one secondary column, bar, line, area, or radar series."},
            "value_axis_minimum":{"type":["number","null"],"minimum":-1000000000000i64,"maximum":1000000000000i64,"default":null,"description":"Optional explicit primary value-axis minimum. It must not hide any primary-axis series value and must be below value_axis_maximum when both are set."},
            "value_axis_maximum":{"type":["number","null"],"minimum":-1000000000000i64,"maximum":1000000000000i64,"default":null,"description":"Optional explicit primary value-axis maximum. It must not hide any primary-axis series value and must be above value_axis_minimum when both are set."},
            "value_axis_log_base":{"type":["number","null"],"minimum":2,"maximum":1000,"default":null,"description":"Optional logarithmic base for the primary value axis. Every primary-axis series value and every explicit primary bound must be strictly positive."},
            "value_axis_major_tick_mark":{"type":"string","enum":["none","inside","outside","cross"],"default":"none","description":"Canonical primary value-axis major tick-mark style."},
            "value_axis_minor_tick_mark":{"type":"string","enum":["none","inside","outside","cross"],"default":"none","description":"Canonical primary value-axis minor tick-mark style."},
            "value_axis_major_unit":{"type":["number","null"],"exclusiveMinimum":0,"maximum":1000000000000i64,"default":null,"description":"Optional explicit primary value-axis major unit. It must exceed value_axis_minor_unit when both are set and must not exceed an explicit minimum/maximum span."},
            "value_axis_minor_unit":{"type":["number","null"],"exclusiveMinimum":0,"maximum":1000000000000i64,"default":null,"description":"Optional explicit primary value-axis minor unit. It must remain below value_axis_major_unit when both are set and must not exceed an explicit minimum/maximum span."},
            "value_axis_number_format":{"type":"string","enum":["general","integer","decimal_1","decimal_2","thousands","thousands_2","percentage","percentage_1","scientific"],"default":"general","description":"Canonical primary value-axis number format."},
            "secondary_value_axis_minimum":{"type":["number","null"],"minimum":-1000000000000i64,"maximum":1000000000000i64,"default":null,"description":"Optional explicit secondary value-axis minimum. It requires secondary series and must not hide their values."},
            "secondary_value_axis_maximum":{"type":["number","null"],"minimum":-1000000000000i64,"maximum":1000000000000i64,"default":null,"description":"Optional explicit secondary value-axis maximum. It requires secondary series and must not hide their values."},
            "secondary_value_axis_log_base":{"type":["number","null"],"minimum":2,"maximum":1000,"default":null,"description":"Optional logarithmic base for the secondary value axis. It requires secondary series, and every secondary-axis series value and explicit secondary bound must be strictly positive."},
            "secondary_value_axis_major_tick_mark":{"type":"string","enum":["none","inside","outside","cross"],"default":"none","description":"Canonical secondary value-axis major tick-mark style. Non-none styles require secondary series."},
            "secondary_value_axis_minor_tick_mark":{"type":"string","enum":["none","inside","outside","cross"],"default":"none","description":"Canonical secondary value-axis minor tick-mark style. Non-none styles require secondary series."},
            "secondary_value_axis_major_unit":{"type":["number","null"],"exclusiveMinimum":0,"maximum":1000000000000i64,"default":null,"description":"Optional explicit secondary value-axis major unit. It requires secondary series, must exceed the secondary minor unit, and must not exceed an explicit secondary range."},
            "secondary_value_axis_minor_unit":{"type":["number","null"],"exclusiveMinimum":0,"maximum":1000000000000i64,"default":null,"description":"Optional explicit secondary value-axis minor unit. It requires secondary series, must remain below the secondary major unit, and must not exceed an explicit secondary range."},
            "secondary_value_axis_number_format":{"type":"string","enum":["general","integer","decimal_1","decimal_2","thousands","thousands_2","percentage","percentage_1","scientific"],"default":"general","description":"Canonical secondary value-axis number format. Non-general formats require secondary series."}
        },
        "required":required,
        "additionalProperties":false
    })
}

fn append_pptx_slides_tool() -> Value {
    let mut schema = create_pptx_tool()
        .get("inputSchema")
        .cloned()
        .expect("create_pptx parameters");
    let object = schema.as_object_mut().expect("PPTX schema object");
    let properties = object
        .get_mut("properties")
        .and_then(Value::as_object_mut)
        .expect("PPTX schema properties");
    properties.remove("target_path");
    properties.insert(
        "path".to_string(),
        json!({"type":"string","description":"Workspace-relative source .pptx path. The source is never modified."}),
    );
    properties.insert(
        "target_path".to_string(),
        json!({"type":"string","description":"Distinct workspace-relative .pptx output path."}),
    );
    object.insert(
        "required".to_string(),
        json!(["path", "target_path", "slides"]),
    );
    tool(
        "append_pptx_slides",
        "Append bounded editable slides, including self-contained standard DrawingML column/bar/line/pie/area/doughnut/radar chart slides with optional canonical RGB series colors, to an existing PPTX while preserving unchanged package parts and writing a distinct output file. New slides inherit the last existing slide's layout. Speaker notes require an existing notes master.",
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
        schema
            .as_object_mut()
            .expect("text table rows schema")
            .insert("minItems".to_string(), json!(1));
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
