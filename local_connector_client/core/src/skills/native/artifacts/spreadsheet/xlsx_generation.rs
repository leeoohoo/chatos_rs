// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::Result;
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{optional_bool, require_extension, required_text, safe_workspace_path};
use super::xlsx_input::parse_worksheets;
use super::xlsx_model::{CellInput, CellValue, NumberFormat, PrimitiveCellValue, WorksheetInput};
use super::{cell_reference, write_new_xlsx};

pub(super) fn create_xlsx(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let target = required_text(arguments, "target_path")?;
    require_extension(target, ".xlsx")?;
    let worksheets = parse_worksheets(arguments)?;
    let entries = workbook_entries(worksheets.as_slice())?;
    let (path, relative) = safe_workspace_path(state, request, target)?;
    let bytes = write_new_xlsx(
        path.as_path(),
        entries,
        optional_bool(arguments, "overwrite"),
    )?;
    let cells = worksheets
        .iter()
        .flat_map(|sheet| sheet.rows.iter())
        .map(Vec::len)
        .sum::<usize>();
    let formulas = worksheets
        .iter()
        .flat_map(|sheet| sheet.rows.iter())
        .flatten()
        .filter(|cell| matches!(cell.value, CellValue::Formula { .. }))
        .count();
    Ok(json!({
        "created": true,
        "path": relative,
        "bytes": bytes,
        "worksheets": worksheets.len(),
        "sheet_names": worksheets.iter().map(|sheet| sheet.name.as_str()).collect::<Vec<_>>(),
        "cells": cells,
        "formula_cells": formulas,
        "recalculation_on_open": formulas > 0,
    }))
}

pub(super) fn workbook_entries(worksheets: &[WorksheetInput]) -> Result<Vec<(String, Vec<u8>)>> {
    let mut entries = vec![
        (
            "[Content_Types].xml".to_string(),
            xlsx_content_types(worksheets.len()).into_bytes(),
        ),
        ("_rels/.rels".to_string(), root_relationships().into_bytes()),
        (
            "xl/workbook.xml".to_string(),
            workbook_xml(worksheets).into_bytes(),
        ),
        (
            "xl/_rels/workbook.xml.rels".to_string(),
            workbook_relationships(worksheets.len()).into_bytes(),
        ),
        ("xl/styles.xml".to_string(), styles_xml().into_bytes()),
    ];
    for (index, sheet) in worksheets.iter().enumerate() {
        entries.push((
            format!("xl/worksheets/sheet{}.xml", index + 1),
            worksheet_xml(sheet)?.into_bytes(),
        ));
    }
    Ok(entries)
}

fn xlsx_content_types(sheet_count: usize) -> String {
    let sheets = (1..=sheet_count)
        .map(|index| format!("<Override PartName=\"/xl/worksheets/sheet{index}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"))
        .collect::<String>();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>{sheets}</Types>"#
    )
}

fn root_relationships() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.to_string()
}

fn workbook_xml(worksheets: &[WorksheetInput]) -> String {
    let sheets = worksheets
        .iter()
        .enumerate()
        .map(|(index, sheet)| {
            format!(
                "<sheet name=\"{}\" sheetId=\"{}\" r:id=\"rId{}\"/>",
                escape_xml(sheet.name.as_str()),
                index + 1,
                index + 1
            )
        })
        .collect::<String>();
    let formulas_present = worksheets
        .iter()
        .flat_map(|sheet| sheet.rows.iter())
        .flatten()
        .any(|cell| matches!(cell.value, CellValue::Formula { .. }));
    let calculation = if formulas_present {
        r#"<calcPr calcId="0" calcMode="auto" fullCalcOnLoad="1" forceFullCalc="1"/>"#
    } else {
        ""
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><bookViews><workbookView activeTab="0"/></bookViews><sheets>{sheets}</sheets>{calculation}</workbook>"#
    )
}

fn workbook_relationships(sheet_count: usize) -> String {
    let sheets = (1..=sheet_count)
        .map(|index| format!("<Relationship Id=\"rId{index}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{index}.xml\"/>"))
        .collect::<String>();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{sheets}<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#,
        sheet_count + 1
    )
}

fn styles_xml() -> String {
    let format_xfs = NumberFormat::all()
        .iter()
        .map(|format| style_xf(*format))
        .collect::<String>();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Calibri"/><family val="2"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="6"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>{format_xfs}</cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>"#
    )
}

fn style_xf(format: NumberFormat) -> String {
    format!(
        "<xf numFmtId=\"{}\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyNumberFormat=\"1\"/>",
        format.built_in_id()
    )
}

fn worksheet_xml(sheet: &WorksheetInput) -> Result<String> {
    let max_column = sheet.rows.iter().map(Vec::len).max().unwrap_or(1).max(1) as u16;
    let max_row = sheet.rows.len().max(1) as u32;
    let dimension = format!("A1:{}", cell_reference(max_column, max_row));
    let sheet_views = if sheet.freeze_rows == 0 {
        "<sheetViews><sheetView workbookViewId=\"0\"/></sheetViews>".to_string()
    } else {
        format!(
            "<sheetViews><sheetView workbookViewId=\"0\"><pane ySplit=\"{}\" topLeftCell=\"A{}\" activePane=\"bottomLeft\" state=\"frozen\"/></sheetView></sheetViews>",
            sheet.freeze_rows,
            sheet.freeze_rows + 1
        )
    };
    let columns = if sheet.column_widths.is_empty() {
        String::new()
    } else {
        let definitions = sheet
            .column_widths
            .iter()
            .map(|(column, width)| {
                format!(
                    "<col min=\"{column}\" max=\"{column}\" width=\"{width}\" customWidth=\"1\"/>"
                )
            })
            .collect::<String>();
        format!("<cols>{definitions}</cols>")
    };
    let mut sheet_data = String::new();
    for (row_index, row) in sheet.rows.iter().enumerate() {
        let row_number = row_index as u32 + 1;
        sheet_data.push_str(format!("<row r=\"{row_number}\">").as_str());
        for (column_index, cell) in row.iter().enumerate() {
            sheet_data.push_str(
                cell_xml(
                    column_index as u16 + 1,
                    row_number,
                    cell,
                    cell.number_format.map(NumberFormat::generated_style_id),
                )?
                .as_str(),
            );
        }
        sheet_data.push_str("</row>");
    }
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="{dimension}"/>{sheet_views}<sheetFormatPr defaultRowHeight="15"/>{columns}<sheetData>{sheet_data}</sheetData></worksheet>"#
    ))
}

pub(super) fn cell_xml(
    column: u16,
    row: u32,
    cell: &CellInput,
    style_id: Option<u32>,
) -> Result<String> {
    let reference = cell_reference(column, row);
    let style = style_id
        .map(|style| format!(" s=\"{style}\""))
        .unwrap_or_default();
    match &cell.value {
        CellValue::Primitive(value) => {
            primitive_cell_xml(reference.as_str(), style.as_str(), value)
        }
        CellValue::Formula {
            expression,
            cached_value,
        } => {
            let (cell_type, cached) = match cached_value {
                None | Some(PrimitiveCellValue::Blank) => (String::new(), String::new()),
                Some(PrimitiveCellValue::Bool(value)) => (
                    " t=\"b\"".to_string(),
                    format!("<v>{}</v>", if *value { 1 } else { 0 }),
                ),
                Some(PrimitiveCellValue::Number(value)) => {
                    (String::new(), format!("<v>{value}</v>"))
                }
                Some(PrimitiveCellValue::Text(value)) => (
                    " t=\"str\"".to_string(),
                    format!("<v>{}</v>", escape_xml(value)),
                ),
            };
            Ok(format!(
                "<c r=\"{reference}\"{style}{cell_type}><f>{}</f>{cached}</c>",
                escape_xml(expression)
            ))
        }
    }
}

fn primitive_cell_xml(reference: &str, style: &str, value: &PrimitiveCellValue) -> Result<String> {
    Ok(match value {
        PrimitiveCellValue::Blank => format!("<c r=\"{reference}\"{style}/>"),
        PrimitiveCellValue::Bool(value) => format!(
            "<c r=\"{reference}\"{style} t=\"b\"><v>{}</v></c>",
            if *value { 1 } else { 0 }
        ),
        PrimitiveCellValue::Number(value) => {
            format!("<c r=\"{reference}\"{style}><v>{value}</v></c>")
        }
        PrimitiveCellValue::Text(value) => format!(
            "<c r=\"{reference}\"{style} t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
            escape_xml(value)
        ),
    })
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
