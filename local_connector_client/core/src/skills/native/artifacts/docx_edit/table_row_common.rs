// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

use super::table_cell_operations::simple_docx_table_cell_text;
use super::{
    count_exact_xml_tags, find_next_xml_tag_start, xml_element_ranges, xml_open_element_stack_at,
    XmlElementRange, MAX_DOCX_BLOCKS, MAX_DOCX_TABLE_COLUMNS,
};

pub(super) fn select_simple_docx_table_rows(
    document_xml: &str,
    table_index: usize,
) -> Result<Vec<XmlElementRange>> {
    let bodies = xml_element_ranges(document_xml, "<w:body", "</w:body>", 1, "DOCX bodies")?;
    if bodies.len() != 1 {
        return Err(anyhow!("DOCX must contain exactly one standard w:body"));
    }
    let body = bodies[0];
    let tables = xml_element_ranges(
        &document_xml[body.open_end..body.close_start],
        "<w:tbl",
        "</w:tbl>",
        MAX_DOCX_BLOCKS,
        "DOCX tables",
    )?
    .into_iter()
    .map(|relative| XmlElementRange {
        start: body.open_end + relative.start,
        open_end: body.open_end + relative.open_end,
        close_start: body.open_end + relative.close_start,
        end: body.open_end + relative.end,
    })
    .filter_map(
        |table| match xml_open_element_stack_at(document_xml, table.start) {
            Ok(stack) if stack == ["w:document", "w:body"] => Some(Ok(table)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        },
    )
    .collect::<Result<Vec<_>>>()?;
    let table = *tables.get(table_index - 1).ok_or_else(|| {
        anyhow!(
            "table index {table_index} is outside the available direct top-level 1..={} range",
            tables.len()
        )
    })?;
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
    xml_element_ranges(
        &document_xml[table.open_end..table.close_start],
        "<w:tr",
        "</w:tr>",
        MAX_DOCX_BLOCKS,
        "DOCX table rows",
    )?
    .into_iter()
    .map(|relative| XmlElementRange {
        start: table.open_end + relative.start,
        open_end: table.open_end + relative.open_end,
        close_start: table.open_end + relative.close_start,
        end: table.open_end + relative.end,
    })
    .filter_map(
        |row| match xml_open_element_stack_at(document_xml, row.start) {
            Ok(stack) if stack == ["w:document", "w:body", "w:tbl"] => Some(Ok(row)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        },
    )
    .collect()
}

pub(super) fn simple_docx_table_row_cell_texts(
    document_xml: &str,
    row: XmlElementRange,
) -> Result<Vec<String>> {
    let row_xml = &document_xml[row.start..row.end];
    if count_exact_xml_tags(row_xml, "<w:tr") != 1 {
        return Err(anyhow!("selected DOCX table row is structurally ambiguous"));
    }
    xml_element_ranges(
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
    .collect::<Result<Vec<_>>>()?
    .into_iter()
    .map(|cell| simple_docx_table_cell_text(&document_xml[cell.start..cell.end]))
    .collect()
}

pub(super) fn ensure_docx_table_row_operation_has_no_range_markup(
    document_xml: &str,
    operation: &str,
) -> Result<()> {
    const RANGE_MARKERS: &[&str] = &[
        "<w:commentRange",
        "<w:commentReference",
        "<w:bookmark",
        "<w:perm",
        "<w:proofErr",
        "<w:moveFromRange",
        "<w:moveToRange",
        "<w:customXmlInsRange",
        "<w:customXmlDelRange",
        "<w:customXmlMoveFromRange",
        "<w:customXmlMoveToRange",
    ];
    if let Some(marker) = RANGE_MARKERS
        .iter()
        .find(|marker| document_xml.contains(**marker))
    {
        return Err(anyhow!(
            "DOCX table row {operation} does not support document range markup: {marker}"
        ));
    }
    Ok(())
}

pub(super) fn strip_docx_clone_identity_attributes(xml: &mut String) -> Result<usize> {
    let mut removed = 0usize;
    for attribute in ["w14:paraId", "w14:textId", "w16cid:durableId"] {
        removed += strip_xml_attribute(xml, attribute)?;
    }
    Ok(removed)
}

fn strip_xml_attribute(xml: &mut String, attribute: &str) -> Result<usize> {
    let mut removed = 0usize;
    let mut cursor = 0usize;
    while let Some(offset) = xml[cursor..].find(attribute) {
        let start = cursor + offset;
        let inside_opening_tag = xml[..start].rfind('<').is_some_and(|opening| {
            xml[..start]
                .rfind('>')
                .is_none_or(|closing| opening > closing)
        });
        let preceded_by_space = start > 0 && xml.as_bytes()[start - 1].is_ascii_whitespace();
        let mut equals = start + attribute.len();
        while xml
            .as_bytes()
            .get(equals)
            .is_some_and(u8::is_ascii_whitespace)
        {
            equals += 1;
        }
        if !inside_opening_tag
            || !preceded_by_space
            || xml.as_bytes().get(equals).copied() != Some(b'=')
        {
            cursor = start + attribute.len();
            continue;
        }
        let mut quote_start = equals + 1;
        while xml
            .as_bytes()
            .get(quote_start)
            .is_some_and(u8::is_ascii_whitespace)
        {
            quote_start += 1;
        }
        let quote = xml
            .as_bytes()
            .get(quote_start)
            .copied()
            .filter(|quote| matches!(quote, b'\'' | b'"'))
            .ok_or_else(|| anyhow!("DOCX clone identity attribute is malformed"))?;
        let quote_end = xml.as_bytes()[quote_start + 1..]
            .iter()
            .position(|byte| *byte == quote)
            .map(|offset| quote_start + 1 + offset + 1)
            .ok_or_else(|| anyhow!("DOCX clone identity attribute is unterminated"))?;
        xml.replace_range(start..quote_end, "");
        removed += 1;
        cursor = start;
    }
    Ok(removed)
}
