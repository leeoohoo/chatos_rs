// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::File;

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use zip::ZipArchive;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::format_helpers::{escape_xml, office_root_relationships};
use super::super::{
    input_file, optional_bool, read_zip_text, require_extension, required_text,
    safe_workspace_path, MAX_XML_BYTES,
};
use super::package_write::{block_result, docx_output_path, rewrite_docx, write_new_docx};
use super::{
    find_last_xml_tag_start, DocxBlockStats, MAX_DOCX_BLOCKS, MAX_DOCX_TABLE_CELLS,
    MAX_DOCX_TABLE_COLUMNS, MAX_DOCX_TEXT_CHARS,
};

pub(super) fn create_structured_docx(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".docx")?;
    let (body, stats) = render_blocks(arguments)?;
    let document_xml = document_xml(body.as_str());
    let entries = vec![
        ("[Content_Types].xml".to_string(), docx_content_types()),
        (
            "_rels/.rels".to_string(),
            office_root_relationships("word/document.xml"),
        ),
        ("word/document.xml".to_string(), document_xml),
        (
            "word/_rels/document.xml.rels".to_string(),
            docx_document_relationships(),
        ),
        ("word/styles.xml".to_string(), docx_styles_xml()),
    ];
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    let bytes = write_new_docx(
        target.as_path(),
        entries,
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(block_result(
        "create_structured",
        target_relative,
        bytes,
        &stats,
    ))
}

pub(super) fn append_docx_content(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let existing_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    let (body, stats) = render_blocks(arguments)?;
    let updated_xml = append_before_section(existing_xml.as_str(), body.as_str())?;
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;
    let bytes = rewrite_docx(
        source.as_path(),
        target.as_path(),
        updated_xml.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    let mut result = block_result("append", target_relative, bytes, &stats);
    result["source_path"] = Value::String(source_relative);
    Ok(result)
}

pub(super) fn render_blocks(arguments: &Value) -> Result<(String, DocxBlockStats)> {
    let blocks = arguments
        .get("blocks")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("blocks must be an array"))?;
    if blocks.is_empty() || blocks.len() > MAX_DOCX_BLOCKS {
        return Err(anyhow!(
            "blocks must contain between 1 and {MAX_DOCX_BLOCKS} items"
        ));
    }
    let mut output = String::new();
    let mut stats = DocxBlockStats::default();
    for block in blocks {
        let block = block
            .as_object()
            .ok_or_else(|| anyhow!("each DOCX block must be an object"))?;
        let kind = block
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("each DOCX block requires type"))?;
        match kind {
            "paragraph" => {
                let text = block
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("paragraph block requires text"))?;
                add_characters(&mut stats, text)?;
                let style = block
                    .get("style")
                    .and_then(Value::as_str)
                    .unwrap_or("normal");
                let align = block.get("align").and_then(Value::as_str).unwrap_or("left");
                let bold = block.get("bold").and_then(Value::as_bool).unwrap_or(false);
                let italic = block
                    .get("italic")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                output.push_str(paragraph_xml(text, style, align, bold, italic)?.as_str());
                stats.paragraphs += 1;
            }
            "table" => {
                let rows = block
                    .get("rows")
                    .and_then(Value::as_array)
                    .ok_or_else(|| anyhow!("table block requires rows"))?;
                let header_row = block
                    .get("header_row")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                output.push_str(table_xml(rows, header_row, &mut stats)?.as_str());
                stats.tables += 1;
                stats.table_rows += rows.len();
            }
            "page_break" => {
                output.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
                stats.page_breaks += 1;
                stats.paragraphs += 1;
            }
            _ => return Err(anyhow!("unsupported DOCX block type: {kind}")),
        }
    }
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("rendered DOCX XML exceeds the local size limit"));
    }
    Ok((output, stats))
}

fn paragraph_xml(text: &str, style: &str, align: &str, bold: bool, italic: bool) -> Result<String> {
    let (size, style_bold, style_italic, keep_next, indent, default_align, style_id) = match style {
        "normal" => (22, false, false, false, "", "left", "Normal"),
        "title" => (36, true, false, true, "", "center", "Title"),
        "subtitle" => (26, false, true, true, "", "center", "Subtitle"),
        "heading1" => (32, true, false, true, "", "left", "Heading1"),
        "heading2" => (28, true, false, true, "", "left", "Heading2"),
        "heading3" => (24, true, false, true, "", "left", "Heading3"),
        "quote" => (
            22,
            false,
            true,
            false,
            "<w:ind w:left=\"720\" w:right=\"720\"/>",
            "left",
            "Quote",
        ),
        _ => return Err(anyhow!("unsupported paragraph style: {style}")),
    };
    let align = if align == "left" {
        default_align
    } else {
        align
    };
    if !matches!(align, "left" | "center" | "right" | "justify") {
        return Err(anyhow!("unsupported paragraph alignment: {align}"));
    }
    let keep_next = if keep_next { "<w:keepNext/>" } else { "" };
    let paragraph_properties = format!(
        "<w:pPr><w:pStyle w:val=\"{style_id}\"/>{keep_next}{indent}<w:jc w:val=\"{align}\"/><w:spacing w:after=\"160\" w:line=\"276\" w:lineRule=\"auto\"/></w:pPr>"
    );
    let bold = if bold || style_bold { "<w:b/>" } else { "" };
    let italic = if italic || style_italic { "<w:i/>" } else { "" };
    Ok(format!(
        "<w:p>{paragraph_properties}<w:r><w:rPr>{bold}{italic}<w:sz w:val=\"{size}\"/><w:szCs w:val=\"{size}\"/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        escape_xml(text)
    ))
}

fn table_xml(rows: &[Value], header_row: bool, stats: &mut DocxBlockStats) -> Result<String> {
    if rows.is_empty() || rows.len() > MAX_DOCX_BLOCKS {
        return Err(anyhow!("table rows must contain between 1 and 2000 items"));
    }
    let mut output = String::from(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblBorders><w:top w:val=\"single\" w:sz=\"4\" w:color=\"B7B7B7\"/><w:left w:val=\"single\" w:sz=\"4\" w:color=\"B7B7B7\"/><w:bottom w:val=\"single\" w:sz=\"4\" w:color=\"B7B7B7\"/><w:right w:val=\"single\" w:sz=\"4\" w:color=\"B7B7B7\"/><w:insideH w:val=\"single\" w:sz=\"4\" w:color=\"D9D9D9\"/><w:insideV w:val=\"single\" w:sz=\"4\" w:color=\"D9D9D9\"/></w:tblBorders></w:tblPr>",
    );
    for (row_index, row) in rows.iter().enumerate() {
        let cells = row
            .as_array()
            .ok_or_else(|| anyhow!("each table row must be an array"))?;
        if cells.is_empty() || cells.len() > MAX_DOCX_TABLE_COLUMNS {
            return Err(anyhow!("table rows must contain between 1 and 63 cells"));
        }
        stats.table_cells = stats.table_cells.saturating_add(cells.len());
        if stats.table_cells > MAX_DOCX_TABLE_CELLS {
            return Err(anyhow!("DOCX tables exceed the 50000 cell safety limit"));
        }
        output.push_str("<w:tr>");
        for cell in cells {
            let text = cell
                .as_str()
                .ok_or_else(|| anyhow!("DOCX table cells must be strings"))?;
            add_characters(stats, text)?;
            let shading = if header_row && row_index == 0 {
                "<w:shd w:val=\"clear\" w:fill=\"D9EAF7\"/>"
            } else {
                ""
            };
            let bold = if header_row && row_index == 0 {
                "<w:b/>"
            } else {
                ""
            };
            output.push_str(
                format!(
                    "<w:tc><w:tcPr>{shading}<w:tcMar><w:top w:w=\"80\" w:type=\"dxa\"/><w:left w:w=\"100\" w:type=\"dxa\"/><w:bottom w:w=\"80\" w:type=\"dxa\"/><w:right w:w=\"100\" w:type=\"dxa\"/></w:tcMar></w:tcPr><w:p><w:r><w:rPr>{bold}<w:sz w:val=\"20\"/><w:szCs w:val=\"20\"/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p></w:tc>",
                    escape_xml(text)
                )
                .as_str(),
            );
        }
        output.push_str("</w:tr>");
    }
    output.push_str("</w:tbl>");
    Ok(output)
}

fn add_characters(stats: &mut DocxBlockStats, text: &str) -> Result<()> {
    stats.characters = stats.characters.saturating_add(text.chars().count());
    if stats.characters > MAX_DOCX_TEXT_CHARS {
        return Err(anyhow!(
            "DOCX content exceeds the 1000000 character safety limit"
        ));
    }
    Ok(())
}

fn document_xml(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr></w:body></w:document>"#
    )
}

pub(super) fn append_before_section(document_xml: &str, body: &str) -> Result<String> {
    let body_end = document_xml
        .rfind("</w:body>")
        .ok_or_else(|| anyhow!("DOCX document.xml is missing w:body"))?;
    let section =
        find_last_xml_tag_start(&document_xml[..body_end], "<w:sectPr").unwrap_or(body_end);
    let mut output = String::with_capacity(document_xml.len().saturating_add(body.len()));
    output.push_str(&document_xml[..section]);
    output.push_str(body);
    output.push_str(&document_xml[section..]);
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok(output)
}

pub(super) fn docx_content_types() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#.to_string()
}

pub(super) fn docx_document_relationships() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.to_string()
}

pub(super) fn docx_styles_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Noto Sans SC" w:hAnsi="Noto Sans SC" w:eastAsia="Noto Sans SC" w:cs="Noto Sans SC"/><w:sz w:val="22"/><w:szCs w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:qFormat/><w:rPr><w:b/><w:sz w:val="48"/><w:szCs w:val="48"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Subtitle"><w:name w:val="Subtitle"/><w:basedOn w:val="Normal"/><w:qFormat/><w:rPr><w:color w:val="666666"/><w:sz w:val="28"/><w:szCs w:val="28"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:rPr><w:b/><w:sz w:val="36"/><w:szCs w:val="36"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:rPr><w:b/><w:sz w:val="32"/><w:szCs w:val="32"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:rPr><w:b/><w:sz w:val="28"/><w:szCs w:val="28"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Quote"><w:name w:val="Quote"/><w:basedOn w:val="Normal"/><w:qFormat/><w:pPr><w:ind w:left="720" w:right="720"/></w:pPr><w:rPr><w:i/><w:color w:val="555555"/></w:rPr></w:style></w:styles>"#.to_string()
}
