// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

use super::super::MAX_XML_BYTES;
use super::table_row_common::{
    ensure_docx_table_row_operation_has_no_range_markup, select_simple_docx_table_rows,
    simple_docx_table_row_cell_texts,
};
use super::{find_next_xml_tag_start, xml_open_element_stack_at};

pub(super) fn move_simple_docx_table_row(
    document_xml: &str,
    table_index: usize,
    row_index: usize,
    expected_cells: &[String],
    reference_row_index: usize,
    reference_expected_cells: &[String],
    position: &str,
) -> Result<(String, usize, usize)> {
    if document_xml.contains("<!--")
        || document_xml.contains("<![CDATA[")
        || document_xml.contains("<!DOCTYPE")
    {
        return Err(anyhow!(
            "DOCX table row movement does not support comments, CDATA, or DTD markup"
        ));
    }
    if !xml_open_element_stack_at(document_xml, document_xml.len())?.is_empty() {
        return Err(anyhow!(
            "DOCX table row movement requires structurally complete document XML"
        ));
    }
    ensure_docx_table_row_operation_has_no_range_markup(document_xml, "movement")?;
    let rows = select_simple_docx_table_rows(document_xml, table_index)?;
    let row = *rows.get(row_index - 1).ok_or_else(|| {
        anyhow!(
            "row index {row_index} is outside the selected table's 1..={} range",
            rows.len()
        )
    })?;
    let reference_row = *rows.get(reference_row_index - 1).ok_or_else(|| {
        anyhow!(
            "reference_row index {reference_row_index} is outside the selected table's 1..={} range",
            rows.len()
        )
    })?;
    if row_index == reference_row_index {
        return Err(anyhow!("row and reference_row must select different rows"));
    }
    if (position == "before" && row_index + 1 == reference_row_index)
        || (position == "after" && reference_row_index + 1 == row_index)
    {
        return Err(anyhow!(
            "requested DOCX table row move is already in the requested position"
        ));
    }
    let row_xml = &document_xml[row.start..row.end];
    let reference_row_xml = &document_xml[reference_row.start..reference_row.end];
    if find_next_xml_tag_start(row_xml, "<w:tblHeader", 0).is_some()
        || find_next_xml_tag_start(reference_row_xml, "<w:tblHeader", 0).is_some()
    {
        return Err(anyhow!(
            "repeating table header rows cannot participate in DOCX table row movement"
        ));
    }
    let actual_cells = simple_docx_table_row_cell_texts(document_xml, row)?;
    if actual_cells != expected_cells {
        return Err(anyhow!(
            "selected DOCX table row cells do not match expected_cells"
        ));
    }
    let actual_reference_cells = simple_docx_table_row_cell_texts(document_xml, reference_row)?;
    if actual_reference_cells != reference_expected_cells {
        return Err(anyhow!(
            "selected DOCX reference row cells do not match reference_expected_cells"
        ));
    }

    let mut output = String::with_capacity(document_xml.len());
    match (row.start < reference_row.start, position) {
        (true, "before") => {
            output.push_str(&document_xml[..row.start]);
            output.push_str(&document_xml[row.end..reference_row.start]);
            output.push_str(row_xml);
            output.push_str(&document_xml[reference_row.start..]);
        }
        (true, "after") => {
            output.push_str(&document_xml[..row.start]);
            output.push_str(&document_xml[row.end..reference_row.end]);
            output.push_str(row_xml);
            output.push_str(&document_xml[reference_row.end..]);
        }
        (false, "before") => {
            output.push_str(&document_xml[..reference_row.start]);
            output.push_str(row_xml);
            output.push_str(&document_xml[reference_row.start..row.start]);
            output.push_str(&document_xml[row.end..]);
        }
        (false, "after") => {
            output.push_str(&document_xml[..reference_row.end]);
            output.push_str(row_xml);
            output.push_str(&document_xml[reference_row.end..row.start]);
            output.push_str(&document_xml[row.end..]);
        }
        (_, _) => return Err(anyhow!("position must be before or after")),
    }
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    let moved_row = match (row_index < reference_row_index, position) {
        (true, "before") => reference_row_index - 1,
        (true, "after") => reference_row_index,
        (false, "before") => reference_row_index,
        (false, "after") => reference_row_index + 1,
        (_, _) => return Err(anyhow!("position must be before or after")),
    };
    Ok((output, rows.len(), moved_row))
}
