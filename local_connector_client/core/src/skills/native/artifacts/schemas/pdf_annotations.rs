// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use super::tool;

pub(super) fn tool_definitions() -> Vec<Value> {
    vec![
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
    ]
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
