// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

use super::table_cell_operations::simple_docx_table_cell_text;
use super::table_row_common::{
    ensure_docx_table_row_operation_has_no_range_markup, select_simple_docx_table_rows,
};
use super::{
    count_exact_xml_tags, xml_element_ranges, xml_open_element_stack_at, XmlElementRange,
    MAX_DOCX_TABLE_COLUMNS,
};

pub(super) fn delete_simple_docx_table_row(
    document_xml: &str,
    table_index: usize,
    row_index: usize,
    expected_cells: &[String],
) -> Result<(String, usize)> {
    if document_xml.contains("<!--")
        || document_xml.contains("<![CDATA[")
        || document_xml.contains("<!DOCTYPE")
    {
        return Err(anyhow!(
            "DOCX table row deletion does not support comments, CDATA, or DTD markup"
        ));
    }
    if !xml_open_element_stack_at(document_xml, document_xml.len())?.is_empty() {
        return Err(anyhow!(
            "DOCX table row deletion requires structurally complete document XML"
        ));
    }
    ensure_docx_table_row_operation_has_no_range_markup(document_xml, "deletion")?;
    let rows = select_simple_docx_table_rows(document_xml, table_index)?;
    if rows.len() <= 1 {
        return Err(anyhow!(
            "DOCX table row deletion refuses to remove the selected table's only row"
        ));
    }
    let row = *rows.get(row_index - 1).ok_or_else(|| {
        anyhow!(
            "row index {row_index} is outside the selected table's 1..={} range",
            rows.len()
        )
    })?;
    let row_xml = &document_xml[row.start..row.end];
    if count_exact_xml_tags(row_xml, "<w:tr") != 1 {
        return Err(anyhow!("selected DOCX table row is structurally ambiguous"));
    }
    let cells = xml_element_ranges(
        &document_xml[row.open_end..row.close_start],
        "<w:tc",
        "</w:tc>",
        MAX_DOCX_TABLE_COLUMNS,
        "DOCX table cells",
    )?
    .into_iter()
    .map(|relative| XmlElementRange {
        start: row.open_end + relative.start,
        open_end: row.open_end + relative.open_end,
        close_start: row.open_end + relative.close_start,
        end: row.open_end + relative.end,
    })
    .filter_map(
        |cell| match xml_open_element_stack_at(document_xml, cell.start) {
            Ok(stack) if stack == ["w:document", "w:body", "w:tbl", "w:tr"] => Some(Ok(cell)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        },
    )
    .collect::<Result<Vec<_>>>()?;
    let actual_cells = cells
        .iter()
        .map(|cell| simple_docx_table_cell_text(&document_xml[cell.start..cell.end]))
        .collect::<Result<Vec<_>>>()?;
    if actual_cells != expected_cells {
        return Err(anyhow!(
            "selected DOCX table row cells do not match expected_cells"
        ));
    }
    let mut output = String::with_capacity(document_xml.len() - (row.end - row.start));
    output.push_str(&document_xml[..row.start]);
    output.push_str(&document_xml[row.end..]);
    Ok((output, rows.len()))
}
