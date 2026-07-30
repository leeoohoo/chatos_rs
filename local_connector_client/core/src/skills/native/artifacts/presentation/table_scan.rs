// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

use super::limits::{
    MAX_PPTX_TABLES_PER_SLIDE, MAX_PPTX_TABLE_CELLS, MAX_PPTX_TABLE_CELL_TEXT_CHARS,
    MAX_PPTX_TABLE_COLUMNS, MAX_PPTX_TABLE_PREVIEW_CHARS, MAX_PPTX_TABLE_ROWS,
    MAX_PPTX_TABLE_TOTAL_TEXT_CHARS,
};
use super::model::{PptxTableScan, PptxXmlElementRange, SimplePptxTable, SimplePptxTableCell};
use super::{
    drawing_text_runs, find_next_pptx_xml_tag_start, pptx_direct_child_local_names,
    pptx_direct_element_ranges, pptx_opening_attribute,
    pptx_paragraph_has_unsupported_cross_run_content, pptx_xml_element_ranges,
    pptx_xml_open_element_stack_at, pptx_xml_tag_end, simple_pptx_text_runs,
};

pub(super) fn scan_pptx_tables(xml: &str) -> Result<Vec<PptxTableScan>> {
    let ranges = pptx_xml_element_ranges(
        xml,
        "<a:tbl",
        "</a:tbl>",
        MAX_PPTX_TABLES_PER_SLIDE,
        "PPTX slide tables",
    )?;
    ranges
        .into_iter()
        .map(|range| scan_pptx_table(xml, range))
        .collect()
}

fn scan_pptx_table(xml: &str, range: PptxXmlElementRange) -> Result<PptxTableScan> {
    let table_xml = &xml[range.start..range.end];
    let row_ranges = pptx_xml_element_ranges(
        table_xml,
        "<a:tr",
        "</a:tr>",
        MAX_PPTX_TABLE_ROWS,
        "PPTX table rows",
    )?;
    let mut rows = Vec::with_capacity(row_ranges.len());
    let mut cells = 0usize;
    let mut columns = 0usize;
    let mut cell_text_truncated = false;
    for row in &row_ranges {
        let row_xml = &table_xml[row.start..row.end];
        let cell_ranges = pptx_xml_element_ranges(
            row_xml,
            "<a:tc",
            "</a:tc>",
            MAX_PPTX_TABLE_COLUMNS,
            "PPTX table row cells",
        )?;
        cells = cells.saturating_add(cell_ranges.len());
        if cells > MAX_PPTX_TABLE_CELLS {
            return Err(anyhow!(
                "PPTX table cells exceed the {MAX_PPTX_TABLE_CELLS} cell safety limit"
            ));
        }
        columns = columns.max(cell_ranges.len());
        let mut row_text = Vec::with_capacity(cell_ranges.len());
        for cell in cell_ranges {
            let cell_xml = &row_xml[cell.start..cell.end];
            let preview = drawing_text_runs(cell_xml, MAX_PPTX_TABLE_PREVIEW_CHARS + 1)?.join("");
            let truncated = preview.chars().count() > MAX_PPTX_TABLE_PREVIEW_CHARS;
            cell_text_truncated |= truncated;
            row_text.push(
                preview
                    .chars()
                    .take(MAX_PPTX_TABLE_PREVIEW_CHARS)
                    .collect::<String>(),
            );
        }
        rows.push(row_text);
    }
    let (simple, unsupported_reason) = match simple_pptx_table(xml, range) {
        Ok(table) => (Some(table), None),
        Err(error) => (None, Some(error.to_string())),
    };
    Ok(PptxTableScan {
        rows: row_ranges.len(),
        columns,
        cells,
        cell_text: rows,
        cell_text_truncated,
        simple,
        unsupported_reason,
    })
}

fn simple_pptx_table(xml: &str, range: PptxXmlElementRange) -> Result<SimplePptxTable> {
    if xml.contains("<!--") || xml.contains("<![CDATA[") || xml.contains("<!DOCTYPE") {
        return Err(anyhow!(
            "simple PPTX table editing does not support comments, CDATA, or DTD markup"
        ));
    }
    let without_declaration = xml
        .trim_start()
        .strip_prefix("<?xml")
        .and_then(|value| value.find("?>").map(|end| &value[end + 2..]))
        .unwrap_or(xml);
    if without_declaration.contains("<?") {
        return Err(anyhow!(
            "simple PPTX table editing does not support processing instructions"
        ));
    }
    let table_xml = &xml[range.start..range.end];
    if &table_xml[..range.open_end - range.start] != "<a:tbl>" {
        return Err(anyhow!(
            "simple PPTX table editing requires a standard a:tbl opening tag"
        ));
    }
    if find_next_pptx_xml_tag_start(
        table_xml,
        "<a:tbl",
        range.open_end.saturating_sub(range.start),
    )
    .is_some_and(|start| start < range.close_start.saturating_sub(range.start))
    {
        return Err(anyhow!("nested DrawingML tables are not supported"));
    }
    let stack = pptx_xml_open_element_stack_at(xml, range.start)?;
    if stack.last().map(String::as_str) != Some("a:graphicData") {
        return Err(anyhow!(
            "DrawingML table is not a direct child of a:graphicData"
        ));
    }
    let graphic_data_start = xml[..range.start]
        .rfind("<a:graphicData")
        .ok_or_else(|| anyhow!("DrawingML table is missing its graphicData parent"))?;
    let graphic_data_open_end = pptx_xml_tag_end(xml, graphic_data_start, range.start)?;
    let graphic_data_opening = &xml[graphic_data_start..graphic_data_open_end];
    let uri = pptx_opening_attribute(graphic_data_opening, "graphicData", "uri")?;
    if uri.as_deref() != Some("http://schemas.openxmlformats.org/drawingml/2006/table") {
        return Err(anyhow!(
            "DrawingML table graphicData has a nonstandard table URI"
        ));
    }

    let table_children = pptx_direct_child_local_names(table_xml, "tbl")?;
    if table_children.len() < 3
        || table_children[0] != "tblPr"
        || table_children[1] != "tblGrid"
        || table_children[2..].iter().any(|name| name != "tr")
    {
        return Err(anyhow!(
            "simple PPTX table requires one tblPr, one tblGrid, and direct rows in canonical order"
        ));
    }

    let grid_ranges = pptx_direct_element_ranges(
        table_xml,
        "<a:tblGrid",
        "</a:tblGrid>",
        1,
        "a:tbl",
        "PPTX table grids",
    )?;
    if grid_ranges.len() != 1 || grid_ranges[0].open_end == grid_ranges[0].end {
        return Err(anyhow!(
            "simple PPTX table requires one non-empty direct tblGrid"
        ));
    }
    let grid_xml = &table_xml[grid_ranges[0].start..grid_ranges[0].end];
    let grid_children = pptx_direct_child_local_names(grid_xml, "tblGrid")?;
    if grid_children.is_empty() || grid_children.iter().any(|name| name != "gridCol") {
        return Err(anyhow!(
            "simple PPTX table grid must contain only gridCol children"
        ));
    }
    let grid_columns = grid_children.len();
    if grid_columns > MAX_PPTX_TABLE_COLUMNS {
        return Err(anyhow!(
            "PPTX table columns exceed the {MAX_PPTX_TABLE_COLUMNS} column safety limit"
        ));
    }

    let row_ranges = pptx_direct_element_ranges(
        table_xml,
        "<a:tr",
        "</a:tr>",
        MAX_PPTX_TABLE_ROWS,
        "a:tbl",
        "PPTX table rows",
    )?;
    if row_ranges.is_empty() {
        return Err(anyhow!("simple PPTX table contains no rows"));
    }
    let mut cells = Vec::new();
    let mut total_text_chars = 0usize;
    for (row_index, row_range) in row_ranges.iter().enumerate() {
        let row_xml = &table_xml[row_range.start..row_range.end];
        let row_children = pptx_direct_child_local_names(row_xml, "tr")?;
        if row_children.is_empty() || row_children.iter().any(|name| name != "tc") {
            return Err(anyhow!(
                "simple PPTX table rows must contain only direct table cells"
            ));
        }
        if row_children.len() != grid_columns {
            return Err(anyhow!(
                "simple PPTX table must be rectangular and match its table grid"
            ));
        }
        let cell_ranges = pptx_direct_element_ranges(
            row_xml,
            "<a:tc",
            "</a:tc>",
            MAX_PPTX_TABLE_COLUMNS,
            "a:tr",
            "PPTX table row cells",
        )?;
        if cell_ranges.len() != grid_columns {
            return Err(anyhow!(
                "simple PPTX table must be rectangular and match its table grid"
            ));
        }
        for (column_index, cell_range) in cell_ranges.iter().enumerate() {
            let cell_xml = &row_xml[cell_range.start..cell_range.end];
            if &cell_xml[..cell_range.open_end - cell_range.start] != "<a:tc>" {
                return Err(anyhow!(
                    "merged or attributed PPTX table cells are not supported"
                ));
            }
            let cell_children = pptx_direct_child_local_names(cell_xml, "tc")?;
            let standard_cell_children = (cell_children.len() == 1 && cell_children[0] == "txBody")
                || (cell_children.len() == 2
                    && cell_children[0] == "txBody"
                    && cell_children[1] == "tcPr");
            if !standard_cell_children {
                return Err(anyhow!(
                    "simple PPTX table cells require one direct txBody followed by optional tcPr"
                ));
            }
            let text_body_ranges = pptx_direct_element_ranges(
                cell_xml,
                "<a:txBody",
                "</a:txBody>",
                1,
                "a:tc",
                "PPTX table cell text bodies",
            )?;
            if text_body_ranges.len() != 1
                || text_body_ranges[0].open_end == text_body_ranges[0].end
            {
                return Err(anyhow!(
                    "simple PPTX table cell requires one non-empty text body"
                ));
            }
            let text_body = text_body_ranges[0];
            let text_body_xml = &cell_xml[text_body.start..text_body.end];
            let body_children = pptx_direct_child_local_names(text_body_xml, "txBody")?;
            let mut body_cursor = 0usize;
            if body_children.get(body_cursor).map(String::as_str) == Some("bodyPr") {
                body_cursor += 1;
            }
            if body_children.get(body_cursor).map(String::as_str) == Some("lstStyle") {
                body_cursor += 1;
            }
            if body_children.get(body_cursor).map(String::as_str) != Some("p")
                || body_cursor + 1 != body_children.len()
            {
                return Err(anyhow!(
                    "simple PPTX table cell text body requires optional bodyPr/lstStyle followed by exactly one paragraph"
                ));
            }
            let paragraph_ranges = pptx_direct_element_ranges(
                text_body_xml,
                "<a:p",
                "</a:p>",
                1,
                "a:txBody",
                "PPTX table cell paragraphs",
            )?;
            if paragraph_ranges.len() != 1
                || paragraph_ranges[0].open_end == paragraph_ranges[0].end
            {
                return Err(anyhow!(
                    "simple PPTX table cell requires one non-empty paragraph"
                ));
            }
            let paragraph = paragraph_ranges[0];
            let paragraph_xml = &text_body_xml[paragraph.start..paragraph.end];
            let paragraph_children = pptx_direct_child_local_names(paragraph_xml, "p")?;
            let mut paragraph_cursor = 0usize;
            if paragraph_children.get(paragraph_cursor).map(String::as_str) == Some("pPr") {
                paragraph_cursor += 1;
            }
            if paragraph_children.get(paragraph_cursor).map(String::as_str) != Some("r") {
                return Err(anyhow!(
                    "simple PPTX table cell paragraph requires exactly one direct text run"
                ));
            }
            paragraph_cursor += 1;
            if paragraph_children.get(paragraph_cursor).map(String::as_str) == Some("endParaRPr") {
                paragraph_cursor += 1;
            }
            if paragraph_cursor != paragraph_children.len() {
                return Err(anyhow!(
                    "simple PPTX table cell paragraph contains unsupported direct content"
                ));
            }
            if pptx_paragraph_has_unsupported_cross_run_content(paragraph_xml) {
                return Err(anyhow!(
                    "simple PPTX table cell contains a field, break, hyperlink, extension, or other unsupported content"
                ));
            }
            let paragraph_absolute_start = range.start
                + row_range.start
                + cell_range.start
                + text_body.start
                + paragraph.start;
            let paragraph_absolute = PptxXmlElementRange {
                start: paragraph_absolute_start,
                open_end: paragraph_absolute_start + (paragraph.open_end - paragraph.start),
                close_start: paragraph_absolute_start + (paragraph.close_start - paragraph.start),
                end: paragraph_absolute_start + (paragraph.end - paragraph.start),
            };
            let runs = simple_pptx_text_runs(xml, paragraph_absolute)?;
            if runs.len() != 1 {
                return Err(anyhow!(
                    "simple PPTX table cell must contain exactly one DrawingML text run"
                ));
            }
            let run = runs.into_iter().next().ok_or_else(|| {
                anyhow!("simple PPTX table cell is missing its validated text run")
            })?;
            let characters = run.decoded.chars().count();
            if characters > MAX_PPTX_TABLE_CELL_TEXT_CHARS {
                return Err(anyhow!(
                    "PPTX table cell text exceeds the {MAX_PPTX_TABLE_CELL_TEXT_CHARS} character safety limit"
                ));
            }
            total_text_chars = total_text_chars.saturating_add(characters);
            if total_text_chars > MAX_PPTX_TABLE_TOTAL_TEXT_CHARS {
                return Err(anyhow!(
                    "PPTX table text exceeds the {MAX_PPTX_TABLE_TOTAL_TEXT_CHARS} character safety limit"
                ));
            }
            cells.push(SimplePptxTableCell {
                row: row_index + 1,
                column: column_index + 1,
                range: PptxXmlElementRange {
                    start: range.start + row_range.start + cell_range.start,
                    open_end: range.start + row_range.start + cell_range.open_end,
                    close_start: range.start + row_range.start + cell_range.close_start,
                    end: range.start + row_range.start + cell_range.end,
                },
                text_start: run.text_start,
                text_open_end: run.text_open_end,
                text_close_end: run.text_close_end,
                decoded: run.decoded,
            });
            if cells.len() > MAX_PPTX_TABLE_CELLS {
                return Err(anyhow!(
                    "PPTX table cells exceed the {MAX_PPTX_TABLE_CELLS} cell safety limit"
                ));
            }
        }
    }
    Ok(SimplePptxTable {
        range,
        rows: row_ranges.len(),
        columns: grid_columns,
        cells,
    })
}
