// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

use super::super::MAX_XML_BYTES;
use super::model::PptxXmlElementRange;
use super::{scan_pptx_tables, simple_pptx_table_columns, simple_pptx_table_rows};

pub(super) fn ensure_changed_pptx_table_move(
    source: usize,
    reference: usize,
    position: &str,
    item: &str,
) -> Result<()> {
    if source == reference {
        return Err(anyhow!(
            "{item} and reference_{item} must select different {item}s"
        ));
    }
    if (position == "before" && source + 1 == reference)
        || (position == "after" && reference + 1 == source)
    {
        return Err(anyhow!(
            "requested PPTX table {item} move is already in the requested position"
        ));
    }
    Ok(())
}

pub(super) fn moved_pptx_table_index(
    source: usize,
    reference: usize,
    position: &str,
) -> Result<usize> {
    match (source < reference, position) {
        (true, "before") => Ok(reference - 1),
        (true, "after") => Ok(reference),
        (false, "before") => Ok(reference),
        (false, "after") => Ok(reference + 1),
        (_, _) => Err(anyhow!("position must be before or after")),
    }
}

pub(super) fn move_pptx_xml_element_edit(
    xml: &str,
    source: PptxXmlElementRange,
    reference: PptxXmlElementRange,
    position: &str,
) -> Result<(usize, usize, String)> {
    if source.start >= source.end
        || reference.start >= reference.end
        || source.end > xml.len()
        || reference.end > xml.len()
        || !xml.is_char_boundary(source.start)
        || !xml.is_char_boundary(source.end)
        || !xml.is_char_boundary(reference.start)
        || !xml.is_char_boundary(reference.end)
        || (source.start < reference.end && reference.start < source.end)
    {
        return Err(anyhow!(
            "PPTX table move element ranges are invalid or overlapping"
        ));
    }
    let source_xml = &xml[source.start..source.end];
    match (source.start < reference.start, position) {
        (true, "before") => Ok((
            source.start,
            reference.start,
            format!("{}{source_xml}", &xml[source.end..reference.start]),
        )),
        (true, "after") => Ok((
            source.start,
            reference.end,
            format!("{}{source_xml}", &xml[source.end..reference.end]),
        )),
        (false, "before") => Ok((
            reference.start,
            source.end,
            format!("{source_xml}{}", &xml[reference.start..source.start]),
        )),
        (false, "after") => Ok((
            reference.end,
            source.end,
            format!("{source_xml}{}", &xml[reference.end..source.start]),
        )),
        (_, _) => Err(anyhow!("position must be before or after")),
    }
}

pub(super) fn apply_pptx_xml_edits(
    xml: &str,
    mut edits: Vec<(usize, usize, String)>,
) -> Result<String> {
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.0));
    let mut next_start = xml.len();
    let mut output = xml.to_string();
    for (start, end, replacement) in edits {
        if start > end
            || end > xml.len()
            || !xml.is_char_boundary(start)
            || !xml.is_char_boundary(end)
            || end > next_start
        {
            return Err(anyhow!("PPTX XML edit ranges are invalid or overlapping"));
        }
        output.replace_range(start..end, replacement.as_str());
        next_start = start;
    }
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!(
            "updated PPTX slide XML exceeds the local XML size limit"
        ));
    }
    Ok(output)
}

pub(super) fn validate_updated_pptx_table_rows(
    xml: &str,
    table_number: usize,
    expected_rows: usize,
    expected_columns: usize,
) -> Result<()> {
    let tables = scan_pptx_tables(xml)?;
    let table = tables
        .get(table_number - 1)
        .ok_or_else(|| anyhow!("updated PPTX slide is missing the structurally edited table"))?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "updated PPTX table no longer satisfies the simple-table contract: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if simple.rows != expected_rows || simple.columns != expected_columns {
        return Err(anyhow!(
            "updated PPTX table dimensions do not match the requested row edit"
        ));
    }
    simple_pptx_table_rows(xml, simple)?;
    Ok(())
}

pub(super) fn validate_updated_pptx_table_columns(
    xml: &str,
    table_number: usize,
    expected_rows: usize,
    expected_columns: usize,
) -> Result<()> {
    let tables = scan_pptx_tables(xml)?;
    let table = tables
        .get(table_number - 1)
        .ok_or_else(|| anyhow!("updated PPTX slide is missing the structurally edited table"))?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "updated PPTX table no longer satisfies the simple-table contract: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if simple.rows != expected_rows || simple.columns != expected_columns {
        return Err(anyhow!(
            "updated PPTX table dimensions do not match the requested column edit"
        ));
    }
    simple_pptx_table_columns(xml, simple)?;
    Ok(())
}

pub(super) fn validate_updated_pptx_table_cells(
    xml: &str,
    table_number: usize,
    expected_rows: usize,
    expected_columns: usize,
) -> Result<()> {
    let tables = scan_pptx_tables(xml)?;
    let table = tables
        .get(table_number - 1)
        .ok_or_else(|| anyhow!("updated PPTX slide is missing the formatted table"))?;
    let simple = table.simple.as_ref().ok_or_else(|| {
        anyhow!(
            "updated PPTX table no longer satisfies the simple-table contract: {}",
            table
                .unsupported_reason
                .as_deref()
                .unwrap_or("unsupported DrawingML table structure")
        )
    })?;
    if simple.rows != expected_rows || simple.columns != expected_columns {
        return Err(anyhow!(
            "updated PPTX table dimensions changed during cell format copying"
        ));
    }
    Ok(())
}
