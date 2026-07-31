// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

use super::super::format_helpers::escape_xml;
use super::super::MAX_XML_BYTES;
use super::table_cell_operations::simple_docx_table_cell_text_element;
use super::table_row_common::{
    ensure_docx_table_row_operation_has_no_range_markup, select_simple_docx_table_rows,
    strip_docx_clone_identity_attributes,
};
use super::{
    count_exact_xml_tags, docx_text_opening_for_value, find_next_xml_tag_start, xml_element_ranges,
    xml_open_element_stack_at, XmlElementRange, MAX_DOCX_BLOCKS, MAX_DOCX_TABLE_COLUMNS,
};

pub(super) fn insert_simple_docx_table_row(
    document_xml: &str,
    table_index: usize,
    reference_row_index: usize,
    position: &str,
    expected_cells: &[String],
    inserted_cells: &[String],
) -> Result<(String, usize, usize, usize)> {
    if document_xml.contains("<!--")
        || document_xml.contains("<![CDATA[")
        || document_xml.contains("<!DOCTYPE")
    {
        return Err(anyhow!(
            "DOCX table row insertion does not support comments, CDATA, or DTD markup"
        ));
    }
    if !xml_open_element_stack_at(document_xml, document_xml.len())?.is_empty() {
        return Err(anyhow!(
            "DOCX table row insertion requires structurally complete document XML"
        ));
    }
    ensure_docx_table_row_operation_has_no_range_markup(document_xml, "insertion")?;
    let rows = select_simple_docx_table_rows(document_xml, table_index)?;
    if rows.len() >= MAX_DOCX_BLOCKS {
        return Err(anyhow!(
            "selected DOCX table already contains the 2000 row safety limit"
        ));
    }
    let reference_row = *rows.get(reference_row_index - 1).ok_or_else(|| {
        anyhow!(
            "reference_row index {reference_row_index} is outside the selected table's 1..={} range",
            rows.len()
        )
    })?;
    let row_xml = &document_xml[reference_row.start..reference_row.end];
    if count_exact_xml_tags(row_xml, "<w:tr") != 1 {
        return Err(anyhow!("selected DOCX table row is structurally ambiguous"));
    }
    if find_next_xml_tag_start(row_xml, "<w:tblHeader", 0).is_some() {
        return Err(anyhow!(
            "reference_row is a repeating table header and cannot be cloned safely"
        ));
    }
    let cells = xml_element_ranges(
        &document_xml[reference_row.open_end..reference_row.close_start],
        "<w:tc",
        "</w:tc>",
        MAX_DOCX_TABLE_COLUMNS,
        "DOCX table cells",
    )?
    .into_iter()
    .map(|relative| XmlElementRange {
        start: reference_row.open_end + relative.start,
        open_end: reference_row.open_end + relative.open_end,
        close_start: reference_row.open_end + relative.close_start,
        end: reference_row.open_end + relative.end,
    })
    .filter_map(
        |cell| match xml_open_element_stack_at(document_xml, cell.start) {
            Ok(stack) if stack == ["w:document", "w:body", "w:tbl", "w:tr"] => Some(Ok(cell)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        },
    )
    .collect::<Result<Vec<_>>>()?;
    let cell_texts = cells
        .iter()
        .map(|cell| simple_docx_table_cell_text_element(&document_xml[cell.start..cell.end]))
        .collect::<Result<Vec<_>>>()?;
    let actual_cells = cell_texts
        .iter()
        .map(|text| text.decoded.as_str())
        .collect::<Vec<_>>();
    if actual_cells
        != expected_cells
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    {
        return Err(anyhow!(
            "selected DOCX reference row cells do not match expected_cells"
        ));
    }
    if inserted_cells.len() != cells.len() {
        return Err(anyhow!(
            "cells must match the selected reference row's physical cell count"
        ));
    }

    let mut replacements = Vec::with_capacity(cells.len());
    for ((cell, text), value) in cells.iter().zip(&cell_texts).zip(inserted_cells) {
        let cell_offset = cell.start - reference_row.start;
        let text_start = cell_offset + text.text_start;
        let text_open_end = cell_offset + text.text_open_end;
        let text_close_start = cell_offset + text.text_close_start;
        let text_close_end = cell_offset + text.text_close_end;
        let opening = docx_text_opening_for_value(&row_xml[text_start..text_open_end], value)?;
        replacements.push((
            text_start,
            text_close_end,
            format!("{opening}{}</w:t>", escape_xml(value)),
            text_close_start,
        ));
    }
    let mut inserted_row_xml = row_xml.to_string();
    for (start, end, replacement, _) in replacements.into_iter().rev() {
        inserted_row_xml.replace_range(start..end, replacement.as_str());
    }
    let stripped_identity_attributes = strip_docx_clone_identity_attributes(&mut inserted_row_xml)?;
    let insertion_point = match position {
        "before" => reference_row.start,
        "after" => reference_row.end,
        _ => return Err(anyhow!("position must be before or after")),
    };
    let mut output =
        String::with_capacity(document_xml.len().saturating_add(inserted_row_xml.len()));
    output.push_str(&document_xml[..insertion_point]);
    output.push_str(inserted_row_xml.as_str());
    output.push_str(&document_xml[insertion_point..]);
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    let inserted_row = if position == "before" {
        reference_row_index
    } else {
        reference_row_index + 1
    };
    Ok((
        output,
        rows.len(),
        inserted_row,
        stripped_identity_attributes,
    ))
}
