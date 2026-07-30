// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

use super::limits::{MAX_PPTX_TABLE_COLUMNS, MAX_PPTX_TABLE_ROWS, SLIDE_HEIGHT, SLIDE_WIDTH};
use super::model::{
    PptxXmlElementRange, SimplePptxTable, SimplePptxTableCell, SimplePptxTableColumn,
    SimplePptxTableRow,
};
use super::{escape_xml, pptx_direct_element_ranges, pptx_text_opening_for_value};

pub(super) fn simple_pptx_table_rows(
    xml: &str,
    table: &SimplePptxTable,
) -> Result<Vec<SimplePptxTableRow>> {
    let table_xml = &xml[table.range.start..table.range.end];
    let ranges = pptx_direct_element_ranges(
        table_xml,
        "<a:tr",
        "</a:tr>",
        MAX_PPTX_TABLE_ROWS,
        "a:tbl",
        "PPTX table rows",
    )?;
    if ranges.len() != table.rows {
        return Err(anyhow!(
            "simple PPTX table row structure changed during validation"
        ));
    }
    let mut rows = Vec::with_capacity(ranges.len());
    let mut total_height = 0i64;
    for range in ranges {
        let absolute = PptxXmlElementRange {
            start: table.range.start + range.start,
            open_end: table.range.start + range.open_end,
            close_start: table.range.start + range.close_start,
            end: table.range.start + range.end,
        };
        let opening = &xml[absolute.start..absolute.open_end];
        let height = canonical_pptx_table_row_height(opening)?;
        total_height = total_height
            .checked_add(height)
            .filter(|value| *value <= SLIDE_HEIGHT)
            .ok_or_else(|| anyhow!("PPTX table total row height exceeds the slide height"))?;
        rows.push(SimplePptxTableRow {
            range: absolute,
            height,
        });
    }
    Ok(rows)
}

pub(super) fn simple_pptx_table_columns(
    xml: &str,
    table: &SimplePptxTable,
) -> Result<Vec<SimplePptxTableColumn>> {
    let table_xml = &xml[table.range.start..table.range.end];
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
            "simple PPTX table column editing requires one non-empty direct tblGrid"
        ));
    }
    let grid = grid_ranges[0];
    let grid_xml = &table_xml[grid.start..grid.end];
    let ranges = pptx_direct_element_ranges(
        grid_xml,
        "<a:gridCol",
        "</a:gridCol>",
        MAX_PPTX_TABLE_COLUMNS,
        "a:tblGrid",
        "PPTX table grid columns",
    )?;
    if ranges.len() != table.columns {
        return Err(anyhow!(
            "simple PPTX table column structure changed during validation"
        ));
    }
    let mut columns = Vec::with_capacity(ranges.len());
    let mut total_width = 0i64;
    for range in ranges {
        let absolute = PptxXmlElementRange {
            start: table.range.start + grid.start + range.start,
            open_end: table.range.start + grid.start + range.open_end,
            close_start: table.range.start + grid.start + range.close_start,
            end: table.range.start + grid.start + range.end,
        };
        let opening = &xml[absolute.start..absolute.open_end];
        let width = canonical_pptx_table_column_width(opening)?;
        total_width = total_width
            .checked_add(width)
            .filter(|value| *value <= SLIDE_WIDTH)
            .ok_or_else(|| anyhow!("PPTX table total column width exceeds the slide width"))?;
        columns.push(SimplePptxTableColumn {
            range: absolute,
            width,
        });
    }
    Ok(columns)
}

fn canonical_pptx_table_column_width(opening: &str) -> Result<i64> {
    let raw = opening
        .strip_prefix("<a:gridCol w=\"")
        .and_then(|value| value.strip_suffix("\"/>"))
        .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .ok_or_else(|| {
            anyhow!(
                "simple PPTX table column editing requires canonical a:gridCol elements with one w attribute"
            )
        })?;
    raw.parse::<i64>()
        .ok()
        .filter(|width| (1..=SLIDE_WIDTH).contains(width))
        .ok_or_else(|| anyhow!("PPTX table column width is outside the local safety limit"))
}

pub(super) fn canonical_pptx_table_column_opening(width: i64) -> String {
    format!("<a:gridCol w=\"{width}\"/>")
}

fn canonical_pptx_table_row_height(opening: &str) -> Result<i64> {
    let raw = opening
        .strip_prefix("<a:tr h=\"")
        .and_then(|value| value.strip_suffix("\">"))
        .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .ok_or_else(|| {
            anyhow!(
                "simple PPTX table row editing requires canonical a:tr elements with one h attribute"
            )
        })?;
    raw.parse::<i64>()
        .ok()
        .filter(|height| (1..=SLIDE_HEIGHT).contains(height))
        .ok_or_else(|| anyhow!("PPTX table row height is outside the local safety limit"))
}

pub(super) fn canonical_pptx_table_row_opening(height: i64) -> String {
    format!("<a:tr h=\"{height}\">")
}

pub(super) fn pptx_table_row_with_height(
    xml: &str,
    row: SimplePptxTableRow,
    height: i64,
) -> Result<String> {
    let mut output = xml[row.range.start..row.range.end].to_string();
    output.replace_range(
        0..row.range.open_end - row.range.start,
        canonical_pptx_table_row_opening(height).as_str(),
    );
    Ok(output)
}

pub(super) fn clone_pptx_table_row_with_text(
    xml: &str,
    row: SimplePptxTableRow,
    cells: &[SimplePptxTableCell],
    values: &[String],
    height: i64,
) -> Result<String> {
    if cells.len() != values.len() {
        return Err(anyhow!(
            "inserted PPTX table row cell count does not match the reference row"
        ));
    }
    let mut output = xml[row.range.start..row.range.end].to_string();
    let mut edits = cells
        .iter()
        .zip(values)
        .map(|(cell, value)| {
            let opening = &xml[cell.text_start..cell.text_open_end];
            let opening = pptx_text_opening_for_value(opening, value)?;
            Ok((
                cell.text_start - row.range.start,
                cell.text_close_end - row.range.start,
                format!("{opening}{}</a:t>", escape_xml(value)),
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    edits.sort_by(|left, right| right.0.cmp(&left.0));
    for (start, end, replacement) in edits {
        output.replace_range(start..end, replacement.as_str());
    }
    output.replace_range(
        0..row.range.open_end - row.range.start,
        canonical_pptx_table_row_opening(height).as_str(),
    );
    Ok(output)
}

pub(super) fn clone_pptx_table_cell_with_text(
    xml: &str,
    cell: &SimplePptxTableCell,
    value: &str,
) -> Result<String> {
    let mut output = xml[cell.range.start..cell.range.end].to_string();
    let opening = &xml[cell.text_start..cell.text_open_end];
    let opening = pptx_text_opening_for_value(opening, value)?;
    output.replace_range(
        cell.text_start - cell.range.start..cell.text_close_end - cell.range.start,
        format!("{opening}{}</a:t>", escape_xml(value)).as_str(),
    );
    Ok(output)
}
