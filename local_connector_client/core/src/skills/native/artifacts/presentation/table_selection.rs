// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::File;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use super::super::read_zip_text;
use super::limits::MAX_PPTX_TABLE_CELL_TEXT_CHARS;
use super::model::{PptxTableScan, SimplePptxTable, SimplePptxTableCell};
use super::package_io::validate_pptx_package;
use super::package_metadata::{parse_relationship_document, presentation_slide_metadata};
use super::relationship_inspection::ordered_presentation_slide_paths;
use super::table_scan::scan_pptx_tables;
use super::text_validation::validate_slide_text;

pub(super) fn required_pptx_index(arguments: &Value, key: &str, maximum: usize) -> Result<usize> {
    arguments
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| (1..=maximum).contains(value))
        .ok_or_else(|| anyhow!("{key} must be an integer between 1 and {maximum}"))
}

pub(super) fn selected_pptx_table(
    source: &Path,
    slide_number: usize,
    table_number: usize,
) -> Result<(String, String, PptxTableScan)> {
    let names = validate_pptx_package(source)?;
    for required in ["ppt/presentation.xml", "ppt/_rels/presentation.xml.rels"] {
        if !names.contains(required) {
            return Err(anyhow!("PPTX is missing required package part: {required}"));
        }
    }
    let mut archive = ZipArchive::new(File::open(source)?)
        .with_context(|| format!("open PPTX {}", source.display()))?;
    let presentation_xml = read_zip_text(&mut archive, "ppt/presentation.xml")?;
    let presentation_relationships_xml =
        read_zip_text(&mut archive, "ppt/_rels/presentation.xml.rels")?;
    let slide_metadata = presentation_slide_metadata(presentation_xml.as_str())?;
    let presentation_relationships = parse_relationship_document(
        presentation_relationships_xml.as_str(),
        "ppt/presentation.xml",
    )?;
    let ordered_slide_paths =
        ordered_presentation_slide_paths(&slide_metadata, &presentation_relationships, &names)?;
    let slide_path = ordered_slide_paths.get(slide_number - 1).ok_or_else(|| {
        anyhow!(
            "slide_number {slide_number} is out-of-range for a PPTX with {} visible slides",
            ordered_slide_paths.len()
        )
    })?;
    let slide_xml = read_zip_text(&mut archive, slide_path.as_str())?;
    let mut tables = scan_pptx_tables(slide_xml.as_str())?;
    if table_number == 0 || table_number > tables.len() {
        return Err(anyhow!(
            "table_number {table_number} is out-of-range for visible slide {slide_number}, which contains {} tables",
            tables.len()
        ));
    }
    let table = tables.remove(table_number - 1);
    Ok((slide_path.clone(), slide_xml, table))
}

pub(super) fn required_pptx_sha256(arguments: &Value, key: &str) -> Result<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .map(str::to_string)
        .ok_or_else(|| {
            anyhow!("{key} must be one lowercase SHA-256 value returned by inspect_pptx_table")
        })
}

pub(super) fn pptx_table_cell_xml_sha256(xml: &str, cell: &SimplePptxTableCell) -> String {
    hex::encode(Sha256::digest(
        &xml.as_bytes()[cell.range.start..cell.range.end],
    ))
}

pub(super) fn simple_pptx_table_cell_xml_sha256(
    xml: &str,
    table: &SimplePptxTable,
) -> Vec<Vec<String>> {
    (1..=table.rows)
        .map(|row| {
            table
                .cells
                .iter()
                .filter(|cell| cell.row == row)
                .map(|cell| pptx_table_cell_xml_sha256(xml, cell))
                .collect()
        })
        .collect()
}

pub(super) fn ensure_pptx_table_cell_xml_sha256(
    xml: &str,
    cell: &SimplePptxTableCell,
    expected: &str,
    label: &str,
) -> Result<()> {
    if pptx_table_cell_xml_sha256(xml, cell) != expected {
        return Err(anyhow!(
            "{label} PPTX table cell XML does not match the inspected SHA-256 snapshot"
        ));
    }
    Ok(())
}

pub(super) fn required_pptx_table_row_cells(
    arguments: &Value,
    key: &str,
    columns: usize,
) -> Result<Vec<String>> {
    required_pptx_table_cells(arguments, key, columns)
}

pub(super) fn ensure_expected_pptx_table_row(
    table: &SimplePptxTable,
    row: usize,
    expected_cells: &[String],
) -> Result<()> {
    let actual = table
        .cells
        .iter()
        .filter(|cell| cell.row == row)
        .map(|cell| cell.decoded.as_str())
        .collect::<Vec<_>>();
    if actual.len() != expected_cells.len()
        || !actual
            .iter()
            .zip(expected_cells)
            .all(|(actual, expected)| *actual == expected)
    {
        return Err(anyhow!(
            "selected PPTX table row does not match expected_cells"
        ));
    }
    Ok(())
}

pub(super) fn required_pptx_table_column_cells(
    arguments: &Value,
    key: &str,
    rows: usize,
) -> Result<Vec<String>> {
    required_pptx_table_cells(arguments, key, rows)
}

pub(super) fn ensure_expected_pptx_table_column(
    table: &SimplePptxTable,
    column: usize,
    expected_cells: &[String],
) -> Result<()> {
    let actual = table
        .cells
        .iter()
        .filter(|cell| cell.column == column)
        .map(|cell| cell.decoded.as_str())
        .collect::<Vec<_>>();
    if actual.len() != expected_cells.len()
        || !actual
            .iter()
            .zip(expected_cells)
            .all(|(actual, expected)| *actual == expected)
    {
        return Err(anyhow!(
            "selected PPTX table column does not match expected_cells"
        ));
    }
    Ok(())
}

fn required_pptx_table_cells(
    arguments: &Value,
    key: &str,
    expected_count: usize,
) -> Result<Vec<String>> {
    let values = arguments
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("{key} must be an array of complete cell strings"))?;
    if values.len() != expected_count {
        return Err(anyhow!(
            "{key} must contain exactly {expected_count} cell strings for the selected PPTX table"
        ));
    }
    let mut output = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let value = value
            .as_str()
            .ok_or_else(|| anyhow!("{key} cell {} must be a string", index + 1))?;
        validate_slide_text(
            value,
            format!("{key} cell {}", index + 1).as_str(),
            MAX_PPTX_TABLE_CELL_TEXT_CHARS,
        )?;
        output.push(value.to_string());
    }
    Ok(output)
}
