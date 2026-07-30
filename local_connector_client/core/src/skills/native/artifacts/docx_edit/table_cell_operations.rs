// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::File;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use zip::ZipArchive;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::format_helpers::{escape_xml, unescape_xml};
use super::super::{input_file, optional_bool, read_zip_text, required_text, MAX_XML_BYTES};
use super::package_write::{docx_output_path, rewrite_docx};
use super::{
    count_exact_xml_tags, find_next_xml_tag_start, inside_open_xml_wrapper, required_docx_index,
    validate_xml_text, xml_element_ranges, MAX_DOCX_BLOCKS, MAX_DOCX_TABLE_COLUMNS,
};

pub(super) struct SimpleDocxTableCellText {
    pub(super) text_start: usize,
    pub(super) text_open_end: usize,
    pub(super) text_close_start: usize,
    pub(super) text_close_end: usize,
    pub(super) decoded: String,
}

pub(super) fn replace_docx_table_cell_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let table_index = required_docx_index(arguments, "table", MAX_DOCX_BLOCKS)?;
    let row_index = required_docx_index(arguments, "row", MAX_DOCX_BLOCKS)?;
    let column_index = required_docx_index(arguments, "column", MAX_DOCX_TABLE_COLUMNS)?;
    let expected = arguments
        .get("expected_text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("expected_text must be a string"))?;
    let replacement = arguments
        .get("replacement")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("replacement must be a string"))?;
    for (field, text) in [("expected_text", expected), ("replacement", replacement)] {
        if text.chars().count() > 4_096 {
            return Err(anyhow!("{field} exceeds the 4096 character safety limit"));
        }
        validate_xml_text(text, field)?;
    }
    if expected == replacement {
        return Err(anyhow!(
            "replacement must differ from expected_text for a DOCX table cell edit"
        ));
    }

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    let document_xml = read_zip_text(&mut archive, "word/document.xml")?;
    drop(archive);
    if document_xml.contains("<!--")
        || document_xml.contains("<![CDATA[")
        || document_xml.contains("<!DOCTYPE")
    {
        return Err(anyhow!(
            "DOCX table cell editing does not support comments, CDATA, or DTD markup"
        ));
    }

    let tables = xml_element_ranges(
        document_xml.as_str(),
        "<w:tbl",
        "</w:tbl>",
        MAX_DOCX_BLOCKS,
        "top-level DOCX tables",
    )?;
    let table = *tables.get(table_index - 1).ok_or_else(|| {
        anyhow!(
            "table index {table_index} is outside the available 1..={} range",
            tables.len()
        )
    })?;
    if [
        ("<w:ins", "</w:ins>"),
        ("<w:del", "</w:del>"),
        ("<w:moveFrom", "</w:moveFrom>"),
        ("<w:moveTo", "</w:moveTo>"),
        ("<w:sdt", "</w:sdt>"),
        ("<w:customXml", "</w:customXml>"),
    ]
    .iter()
    .any(|(opening, closing)| {
        inside_open_xml_wrapper(&document_xml[..table.start], opening, closing)
    }) {
        return Err(anyhow!(
            "selected DOCX table is inside revision or structured-content markup"
        ));
    }
    let table_xml = &document_xml[table.start..table.end];
    for marker in [
        "<w:ins",
        "<w:del",
        "<w:moveFrom",
        "<w:moveTo",
        "<w:cellIns",
        "<w:cellDel",
        "<w:cellMerge",
        "<w:tblPrChange",
        "<w:tblPrExChange",
        "<w:trPrChange",
        "<w:tcPrChange",
        "<w:commentRange",
        "<w:sdt",
        "<w:customXml",
    ] {
        if find_next_xml_tag_start(table_xml, marker, 0).is_some() {
            return Err(anyhow!(
                "selected DOCX table contains revision or structured-content markup"
            ));
        }
    }
    if count_exact_xml_tags(table_xml, "<w:tbl") != 1 {
        return Err(anyhow!(
            "selected DOCX table contains a nested table and cannot be edited safely"
        ));
    }
    let table_inner = &document_xml[table.open_end..table.close_start];
    let rows = xml_element_ranges(
        table_inner,
        "<w:tr",
        "</w:tr>",
        MAX_DOCX_BLOCKS,
        "DOCX table rows",
    )?;
    let row = *rows.get(row_index - 1).ok_or_else(|| {
        anyhow!(
            "row index {row_index} is outside the selected table's 1..={} range",
            rows.len()
        )
    })?;
    let row_start = table.open_end + row.start;
    let row_open_end = table.open_end + row.open_end;
    let row_close_start = table.open_end + row.close_start;
    let row_end = table.open_end + row.end;
    let row_xml = &document_xml[row_start..row_end];
    if count_exact_xml_tags(row_xml, "<w:tr") != 1 {
        return Err(anyhow!("selected DOCX table row is structurally ambiguous"));
    }
    let row_inner = &document_xml[row_open_end..row_close_start];
    let cells = xml_element_ranges(
        row_inner,
        "<w:tc",
        "</w:tc>",
        MAX_DOCX_TABLE_COLUMNS,
        "DOCX table cells",
    )?;
    let cell = *cells.get(column_index - 1).ok_or_else(|| {
        anyhow!(
            "column index {column_index} is outside the selected row's 1..={} range",
            cells.len()
        )
    })?;
    let cell_start = row_open_end + cell.start;
    let cell_open_end = row_open_end + cell.open_end;
    let cell_close_start = row_open_end + cell.close_start;
    let cell_end = row_open_end + cell.end;
    let cell_xml = &document_xml[cell_start..cell_end];
    validate_simple_docx_table_cell(cell_xml)?;

    let cell_inner = &document_xml[cell_open_end..cell_close_start];
    let paragraphs = xml_element_ranges(
        cell_inner,
        "<w:p",
        "</w:p>",
        2,
        "DOCX table cell paragraphs",
    )?;
    if paragraphs.len() != 1 {
        return Err(anyhow!(
            "selected DOCX table cell must contain exactly one paragraph"
        ));
    }
    let paragraph = paragraphs[0];
    let paragraph_open_end = cell_open_end + paragraph.open_end;
    let paragraph_close_start = cell_open_end + paragraph.close_start;
    let paragraph_inner = &document_xml[paragraph_open_end..paragraph_close_start];
    let runs = xml_element_ranges(paragraph_inner, "<w:r", "</w:r>", 2, "DOCX table cell runs")?;
    if runs.len() != 1 {
        return Err(anyhow!(
            "selected DOCX table cell must contain exactly one text run"
        ));
    }
    let run = runs[0];
    let run_open_end = paragraph_open_end + run.open_end;
    let run_close_start = paragraph_open_end + run.close_start;
    let run_inner = &document_xml[run_open_end..run_close_start];
    let texts = xml_element_ranges(
        run_inner,
        "<w:t",
        "</w:t>",
        2,
        "DOCX table cell text elements",
    )?;
    if texts.len() != 1 {
        return Err(anyhow!(
            "selected DOCX table cell must contain exactly one text element"
        ));
    }
    let text = texts[0];
    let text_open_end = run_open_end + text.open_end;
    let text_close_start = run_open_end + text.close_start;
    let decoded = unescape_xml(&document_xml[text_open_end..text_close_start]);
    if decoded != expected {
        return Err(anyhow!(
            "selected DOCX table cell text does not match expected_text"
        ));
    }

    let escaped_replacement = escape_xml(replacement);
    let mut updated_xml = String::with_capacity(
        document_xml
            .len()
            .saturating_sub(text_close_start - text_open_end)
            .saturating_add(escaped_replacement.len()),
    );
    updated_xml.push_str(&document_xml[..text_open_end]);
    updated_xml.push_str(escaped_replacement.as_str());
    updated_xml.push_str(&document_xml[text_close_start..]);
    if updated_xml.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }

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
    Ok(json!({
        "created": true,
        "operation": "replace_table_cell_text",
        "source_path": source_relative,
        "path": target_relative,
        "table": table_index,
        "row": row_index,
        "column": column_index,
        "previous_characters": expected.chars().count(),
        "replacement_characters": replacement.chars().count(),
        "formatting_preserved": true,
        "bytes": bytes,
    }))
}

pub(super) fn simple_docx_table_cell_text(cell_xml: &str) -> Result<String> {
    Ok(simple_docx_table_cell_text_element(cell_xml)?.decoded)
}

pub(super) fn simple_docx_table_cell_text_element(
    cell_xml: &str,
) -> Result<SimpleDocxTableCellText> {
    validate_simple_docx_table_cell(cell_xml)?;
    let paragraphs =
        xml_element_ranges(cell_xml, "<w:p", "</w:p>", 2, "DOCX table cell paragraphs")?;
    if paragraphs.len() != 1 {
        return Err(anyhow!(
            "selected DOCX table cell must contain exactly one paragraph"
        ));
    }
    let paragraph = paragraphs[0];
    let runs = xml_element_ranges(
        &cell_xml[paragraph.open_end..paragraph.close_start],
        "<w:r",
        "</w:r>",
        2,
        "DOCX table cell runs",
    )?;
    if runs.len() != 1 {
        return Err(anyhow!(
            "selected DOCX table cell must contain exactly one text run"
        ));
    }
    let run = runs[0];
    let run_open_end = paragraph.open_end + run.open_end;
    let run_close_start = paragraph.open_end + run.close_start;
    let texts = xml_element_ranges(
        &cell_xml[run_open_end..run_close_start],
        "<w:t",
        "</w:t>",
        2,
        "DOCX table cell text elements",
    )?;
    if texts.len() != 1 {
        return Err(anyhow!(
            "selected DOCX table cell must contain exactly one text element"
        ));
    }
    let text = texts[0];
    let text_open_end = run_open_end + text.open_end;
    let text_close_start = run_open_end + text.close_start;
    let raw = &cell_xml[text_open_end..text_close_start];
    if raw.contains('<') {
        return Err(anyhow!(
            "selected DOCX table cell text contains unsupported nested XML"
        ));
    }
    Ok(SimpleDocxTableCellText {
        text_start: run_open_end + text.start,
        text_open_end,
        text_close_start,
        text_close_end: run_open_end + text.end,
        decoded: unescape_xml(raw),
    })
}

fn validate_simple_docx_table_cell(cell_xml: &str) -> Result<()> {
    if count_exact_xml_tags(cell_xml, "<w:tc") != 1 {
        return Err(anyhow!(
            "selected DOCX table cell is structurally ambiguous"
        ));
    }
    for marker in [
        "<w:gridSpan",
        "<w:vMerge",
        "<w:hMerge",
        "<w:cellIns",
        "<w:cellDel",
        "<w:cellMerge",
        "<w:tbl",
        "<w:ins",
        "<w:del",
        "<w:moveFrom",
        "<w:moveTo",
        "<w:commentRange",
        "<w:commentReference",
        "<w:fldChar",
        "<w:instrText",
        "<w:drawing",
        "<w:object",
        "<w:hyperlink",
        "<w:sdt",
        "<w:customXml",
        "<w:smartTag",
        "<w:tab",
        "<w:br",
        "<w:cr",
        "<w:footnoteReference",
        "<w:endnoteReference",
        "<w:bookmarkStart",
        "<w:bookmarkEnd",
    ] {
        if find_next_xml_tag_start(cell_xml, marker, 0).is_some() {
            return Err(anyhow!(
                "selected DOCX table cell contains merged or complex content"
            ));
        }
    }
    Ok(())
}
