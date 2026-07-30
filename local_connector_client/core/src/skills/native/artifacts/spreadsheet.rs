// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::{Reader, Writer, XmlVersion};
use serde_json::{json, Value};
use zip::ZipArchive;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::{
    input_file, optional_bool, read_zip_text, require_extension, required_text,
    safe_workspace_path, MAX_XML_BYTES,
};

mod xlsx_generation;
mod xlsx_input;
mod xlsx_inspection;
mod xlsx_model;
mod xlsx_package;
mod xlsx_package_write;
mod xlsx_rewrite;

use xlsx_input::{parse_cell_rows, validate_sheet_name};
use xlsx_model::{CellInput, CellValue, NumberFormat};
use xlsx_package::{
    optional_attribute, read_workbook_parts, required_attribute, validate_xlsx_package,
    workbook_sheet_parts, workbook_styles_part,
};
use xlsx_package_write::{ensure_distinct_xlsx_paths, rewrite_xlsx_package, write_new_xlsx};
use xlsx_rewrite::{event_name, force_formula_recalculation, rewrite_worksheet};

#[cfg(test)]
use xlsx_generation::workbook_entries;
#[cfg(test)]
use xlsx_input::{parse_cell_input, parse_worksheets, validate_formula};

const MAX_XLSX_SHEETS: usize = 64;
const MAX_XLSX_ROWS: u32 = 1_048_576;
const MAX_XLSX_COLUMNS: u16 = 16_384;
const MAX_XLSX_ZIP_ENTRIES: usize = 10_000;
const MAX_FORMULA_BYTES: usize = 4_096;
const MAX_CELL_TEXT_CHARS: usize = 32_767;
const MAX_COLUMN_WIDTH: f64 = 255.0;

pub(super) fn create_xlsx(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    xlsx_generation::create_xlsx(arguments, state, request)
}

pub(super) fn inspect_xlsx(path: &Path, relative: &str) -> Result<Value> {
    xlsx_inspection::inspect_xlsx(path, relative)
}

pub(super) fn validate_xlsx_for_render(path: &Path) -> Result<()> {
    xlsx_inspection::validate_xlsx_for_render(path)
}

pub(super) fn update_xlsx_range(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".xlsx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".xlsx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_xlsx_paths(source.as_path(), target.as_path())?;

    let sheet_name = required_text(arguments, "sheet_name")?;
    validate_sheet_name(sheet_name)?;
    let (start_column, start_row) = parse_cell_reference(required_text(arguments, "start_cell")?)?;
    let rows = parse_cell_rows(
        arguments
            .get("values")
            .ok_or_else(|| anyhow!("values is required"))?,
        "values",
    )?;
    if rows.is_empty() || rows.iter().all(Vec::is_empty) {
        return Err(anyhow!("values must contain at least one cell"));
    }
    let width = rows[0].len();
    if width == 0 || rows.iter().any(|row| row.len() != width) {
        return Err(anyhow!(
            "values must be a non-empty rectangular two-dimensional array"
        ));
    }
    let mut updates = BTreeMap::<u32, BTreeMap<u16, CellInput>>::new();
    for (row_offset, row) in rows.into_iter().enumerate() {
        let row_number = start_row
            .checked_add(u32::try_from(row_offset).context("XLSX row offset overflow")?)
            .ok_or_else(|| anyhow!("XLSX update exceeds the row limit"))?;
        if row_number > MAX_XLSX_ROWS {
            return Err(anyhow!("XLSX update exceeds the row limit"));
        }
        for (column_offset, cell) in row.into_iter().enumerate() {
            let column = u32::from(start_column)
                .checked_add(u32::try_from(column_offset).context("XLSX column offset overflow")?)
                .ok_or_else(|| anyhow!("XLSX update exceeds the column limit"))?;
            if column > u32::from(MAX_XLSX_COLUMNS) {
                return Err(anyhow!("XLSX update exceeds the column limit"));
            }
            updates
                .entry(row_number)
                .or_default()
                .insert(column as u16, cell);
        }
    }

    let package_names = validate_xlsx_package(source.as_path())?;
    let (workbook_xml, relationships_xml) = read_workbook_parts(source.as_path())?;
    let sheets = workbook_sheet_parts(workbook_xml.as_str(), relationships_xml.as_str())?;
    let sheet = sheets
        .iter()
        .find(|sheet| sheet.name == sheet_name)
        .ok_or_else(|| anyhow!("XLSX worksheet does not exist: {sheet_name}"))?;
    if !package_names.contains(sheet.path.as_str()) {
        return Err(anyhow!("XLSX is missing worksheet part: {}", sheet.path));
    }

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open XLSX {}", source.display()))?;
    let sheet_xml = read_zip_text(&mut archive, sheet.path.as_str())?;
    reject_unsupported_update_intersections(sheet_xml.as_str(), &updates)?;

    let formats = updates
        .values()
        .flat_map(BTreeMap::values)
        .filter_map(|cell| cell.number_format)
        .collect::<BTreeSet<_>>();
    let mut replacements = BTreeMap::new();
    let style_ids = if formats.is_empty() {
        BTreeMap::new()
    } else {
        let styles_path = workbook_styles_part(relationships_xml.as_str())?
            .ok_or_else(|| anyhow!("formatted XLSX updates require an existing styles part"))?;
        if !package_names.contains(styles_path.as_str()) {
            return Err(anyhow!("XLSX is missing styles part: {styles_path}"));
        }
        let styles_xml = read_zip_text(&mut archive, styles_path.as_str())?;
        let (updated_styles, style_ids) =
            append_number_format_styles(styles_xml.as_str(), &formats)?;
        replacements.insert(styles_path, updated_styles.into_bytes());
        style_ids
    };
    drop(archive);

    let formula_cells = updates
        .values()
        .flat_map(BTreeMap::values)
        .filter(|cell| matches!(cell.value, CellValue::Formula { .. }))
        .count();
    let updated_sheet = rewrite_worksheet(sheet_xml.as_str(), updates, &style_ids)?;
    replacements.insert(sheet.path.clone(), updated_sheet.into_bytes());
    if formula_cells > 0 {
        replacements.insert(
            "xl/workbook.xml".to_string(),
            force_formula_recalculation(workbook_xml.as_str())?.into_bytes(),
        );
    }
    let updated_cells = replacements
        .get(sheet.path.as_str())
        .map(|_| {
            // Count the requested cells instead of reparsing the rewritten worksheet.
            arguments
                .get("values")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(Value::as_array)
                        .map(Vec::len)
                        .sum::<usize>()
                })
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let bytes = rewrite_xlsx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "source_path": source_relative,
        "path": target_relative,
        "sheet_name": sheet_name,
        "start_cell": cell_reference(start_column, start_row),
        "updated_cells": updated_cells,
        "formula_cells": formula_cells,
        "source_unchanged": true,
        "bytes": bytes,
    }))
}

fn reject_unsupported_update_intersections(
    sheet_xml: &str,
    updates: &BTreeMap<u32, BTreeMap<u16, CellInput>>,
) -> Result<()> {
    let mut reader = Reader::from_str(sheet_xml);
    reader.config_mut().trim_text(false);
    let mut current_target = false;
    let mut current_cell_depth = 0usize;
    loop {
        match reader
            .read_event()
            .context("inspect XLSX update intersections")?
        {
            Event::Start(event) if event.local_name().as_ref() == b"c" => {
                let reference = required_attribute(&reader, &event, "r")?;
                let (column, row) = parse_cell_reference(reference.as_str())?;
                current_target = updates
                    .get(&row)
                    .is_some_and(|cells| cells.contains_key(&column));
                current_cell_depth = 1;
            }
            Event::Start(event) if current_cell_depth > 0 => {
                current_cell_depth += 1;
                if current_target && event.local_name().as_ref() == b"f" {
                    let formula_type = optional_attribute(&reader, &event, "t")?;
                    if formula_type
                        .as_deref()
                        .is_some_and(|value| matches!(value, "shared" | "array" | "dataTable"))
                    {
                        return Err(anyhow!(
                            "XLSX update intersects a shared, array, or data-table formula and was refused"
                        ));
                    }
                }
            }
            Event::Empty(event)
                if current_target
                    && current_cell_depth > 0
                    && event.local_name().as_ref() == b"f" =>
            {
                let formula_type = optional_attribute(&reader, &event, "t")?;
                if formula_type
                    .as_deref()
                    .is_some_and(|value| matches!(value, "shared" | "array" | "dataTable"))
                {
                    return Err(anyhow!(
                        "XLSX update intersects a shared, array, or data-table formula and was refused"
                    ));
                }
            }
            Event::End(event) if current_cell_depth > 0 => {
                current_cell_depth -= 1;
                if current_cell_depth == 0 && event.local_name().as_ref() == b"c" {
                    current_target = false;
                }
            }
            Event::Empty(event) if event.local_name().as_ref() == b"mergeCell" => {
                let range = required_attribute(&reader, &event, "ref")?;
                let (start, end) = parse_range_reference(range.as_str())?;
                if updates.iter().any(|(row, cells)| {
                    *row >= start.1
                        && *row <= end.1
                        && cells
                            .keys()
                            .any(|column| *column >= start.0 && *column <= end.0)
                }) {
                    return Err(anyhow!(
                        "XLSX update intersects a merged cell range and was refused"
                    ));
                }
            }
            Event::Start(event) if event.local_name().as_ref() == b"mergeCell" => {
                let range = required_attribute(&reader, &event, "ref")?;
                let (start, end) = parse_range_reference(range.as_str())?;
                if updates.iter().any(|(row, cells)| {
                    *row >= start.1
                        && *row <= end.1
                        && cells
                            .keys()
                            .any(|column| *column >= start.0 && *column <= end.0)
                }) {
                    return Err(anyhow!(
                        "XLSX update intersects a merged cell range and was refused"
                    ));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(())
}

fn append_number_format_styles(
    xml: &str,
    formats: &BTreeSet<NumberFormat>,
) -> Result<(String, BTreeMap<NumberFormat, u32>)> {
    let existing_count = count_cell_xfs(xml)?;
    if existing_count.saturating_add(formats.len()) > 65_000 {
        return Err(anyhow!(
            "XLSX style table is too large for a bounded update"
        ));
    }
    let mut style_ids = BTreeMap::new();
    for (offset, format) in formats.iter().enumerate() {
        style_ids.insert(*format, existing_count as u32 + offset as u32);
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(xml.len() + formats.len() * 128));
    let mut in_cell_xfs = false;
    let mut found = false;
    loop {
        match reader.read_event().context("parse XLSX styles XML")? {
            Event::Start(event) if event.local_name().as_ref() == b"cellXfs" => {
                if event_name(&event)?.contains(':') {
                    return Err(anyhow!(
                        "formatted XLSX updates do not support prefixed spreadsheet namespaces"
                    ));
                }
                found = true;
                in_cell_xfs = true;
                writer.write_event(Event::Start(rebuild_start_with_count(
                    &reader,
                    &event,
                    existing_count + formats.len(),
                )?))?;
            }
            Event::End(event) if in_cell_xfs && event.local_name().as_ref() == b"cellXfs" => {
                for format in formats {
                    writer.write_event(Event::Empty(style_xf_event(*format)))?;
                }
                writer.write_event(Event::End(event.into_owned()))?;
                in_cell_xfs = false;
            }
            Event::Empty(event) if event.local_name().as_ref() == b"cellXfs" => {
                if event_name(&event)?.contains(':') {
                    return Err(anyhow!(
                        "formatted XLSX updates do not support prefixed spreadsheet namespaces"
                    ));
                }
                found = true;
                writer.write_event(Event::Start(rebuild_start_with_count(
                    &reader,
                    &event,
                    formats.len(),
                )?))?;
                for format in formats {
                    writer.write_event(Event::Empty(style_xf_event(*format)))?;
                }
                writer.write_event(Event::End(BytesEnd::new(event_name(&event)?)))?;
            }
            Event::Eof => break,
            event => writer.write_event(event.into_owned())?,
        }
    }
    if !found {
        return Err(anyhow!("XLSX styles part is missing cellXfs"));
    }
    Ok((
        String::from_utf8(writer.into_inner()).context("encode updated XLSX styles XML")?,
        style_ids,
    ))
}

fn count_cell_xfs(xml: &str) -> Result<usize> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut in_cell_xfs = false;
    let mut count = 0usize;
    loop {
        match reader.read_event().context("parse XLSX styles XML")? {
            Event::Start(event) if event.local_name().as_ref() == b"cellXfs" => {
                in_cell_xfs = true;
            }
            Event::End(event) if in_cell_xfs && event.local_name().as_ref() == b"cellXfs" => {
                return Ok(count);
            }
            Event::Start(event) | Event::Empty(event)
                if in_cell_xfs && event.local_name().as_ref() == b"xf" =>
            {
                count = count.saturating_add(1);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Err(anyhow!("XLSX styles part is missing cellXfs"))
}

fn rebuild_start_with_count(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    count: usize,
) -> Result<BytesStart<'static>> {
    let name = event_name(event)?;
    let mut rebuilt = BytesStart::new(name);
    for attribute in event.attributes().with_checks(false) {
        let attribute = attribute.context("parse XLSX XML attribute")?;
        let key = std::str::from_utf8(attribute.key.as_ref())?.to_string();
        if key.rsplit(':').next() == Some("count") {
            continue;
        }
        let value = attribute
            .decoded_and_normalized_value(XmlVersion::Explicit1_0, reader.decoder())?
            .into_owned();
        rebuilt.push_attribute((key.as_str(), value.as_str()));
    }
    rebuilt.push_attribute(("count", count.to_string().as_str()));
    Ok(rebuilt.into_owned())
}

fn style_xf_event(format: NumberFormat) -> BytesStart<'static> {
    let mut event = BytesStart::new("xf");
    let num_fmt_id = format.built_in_id().to_string();
    event.push_attribute(("numFmtId", num_fmt_id.as_str()));
    event.push_attribute(("fontId", "0"));
    event.push_attribute(("fillId", "0"));
    event.push_attribute(("borderId", "0"));
    event.push_attribute(("xfId", "0"));
    event.push_attribute(("applyNumberFormat", "1"));
    event.into_owned()
}

fn parse_cell_reference(value: &str) -> Result<(u16, u32)> {
    let value = value.trim();
    let split = value
        .bytes()
        .position(|byte| byte.is_ascii_digit())
        .ok_or_else(|| anyhow!("cell reference must use A1 notation"))?;
    let (column, row) = value.split_at(split);
    if column.is_empty()
        || row.is_empty()
        || !column.bytes().all(|byte| byte.is_ascii_alphabetic())
        || !row.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(anyhow!("cell reference must use A1 notation"));
    }
    let column = parse_column_reference(column)?;
    let row = row
        .parse::<u32>()
        .context("cell row is outside the XLSX range")?;
    if row == 0 || row > MAX_XLSX_ROWS {
        return Err(anyhow!("cell row is outside the XLSX range"));
    }
    Ok((column, row))
}

fn parse_column_reference(value: &str) -> Result<u16> {
    if value.is_empty() || value.len() > 3 || !value.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        return Err(anyhow!("column must use A through XFD notation"));
    }
    let mut column = 0u32;
    for byte in value.bytes() {
        column = column
            .checked_mul(26)
            .and_then(|value| value.checked_add(u32::from(byte.to_ascii_uppercase() - b'A' + 1)))
            .ok_or_else(|| anyhow!("column is outside the XLSX range"))?;
    }
    if column == 0 || column > u32::from(MAX_XLSX_COLUMNS) {
        return Err(anyhow!("column must use A through XFD notation"));
    }
    Ok(column as u16)
}

fn cell_reference(column: u16, row: u32) -> String {
    let mut value = usize::from(column);
    let mut name = String::new();
    while value > 0 {
        let remainder = (value - 1) % 26;
        name.insert(0, (b'A' + remainder as u8) as char);
        value = (value - 1) / 26;
    }
    format!("{name}{row}")
}

fn parse_range_reference(value: &str) -> Result<((u16, u32), (u16, u32))> {
    let (start, end) = value
        .split_once(':')
        .map_or((value, value), |(start, end)| (start, end));
    let start = parse_cell_reference(start)?;
    let end = parse_cell_reference(end)?;
    if start.0 > end.0 || start.1 > end.1 {
        return Err(anyhow!("XLSX range reference is reversed"));
    }
    Ok((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_formula_allowlist_and_a1_references() {
        assert_eq!(
            validate_formula("=SUM(A1:B2)").expect("formula"),
            "SUM(A1:B2)"
        );
        assert!(validate_formula("WEBSERVICE(A1)").is_err());
        assert!(validate_formula("'[book.xlsx]Sheet1'!A1").is_err());
        assert!(validate_formula("DangerousDefinedName").is_err());
        assert_eq!(
            validate_formula("SUM('Sales Data'!A1:A2)").expect("quoted sheet formula"),
            "SUM('Sales Data'!A1:A2)"
        );
        assert!(validate_formula("SUM('Sales Data'!$A$1:$A$2)+1E-10").is_ok());
        assert_eq!(
            parse_cell_reference("XFD1048576").expect("max cell"),
            (16_384, 1_048_576)
        );
        assert!(parse_cell_reference("XFE1").is_err());
    }

    #[test]
    fn rewrites_missing_rows_and_cells_without_touching_unrelated_cells() {
        let xml = r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1:A3"/><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>keep</t></is></c></row><row r="3"><c r="A3"><v>3</v></c></row></sheetData></worksheet>"#;
        let mut updates = BTreeMap::new();
        updates.insert(
            2,
            BTreeMap::from([(
                2,
                parse_cell_input(&json!({"value":2.5,"number_format":"decimal_2"})).expect("cell"),
            )]),
        );
        updates.insert(
            3,
            BTreeMap::from([(3, parse_cell_input(&json!("new")).expect("cell"))]),
        );
        let rewritten =
            rewrite_worksheet(xml, updates, &BTreeMap::from([(NumberFormat::Decimal2, 8)]))
                .expect("rewrite");
        assert!(rewritten.contains("ref=\"A1:C3\""));
        assert!(rewritten.contains("<row r=\"2\"><c r=\"B2\" s=\"8\"><v>2.5</v></c></row>"));
        assert!(rewritten.contains("<c r=\"A1\" t=\"inlineStr\"><is><t>keep</t></is></c>"));
        assert!(rewritten.contains("<c r=\"C3\" t=\"inlineStr\""));
    }

    #[test]
    fn rejects_merged_and_shared_formula_intersections() {
        let updates = BTreeMap::from([(
            1,
            BTreeMap::from([(1, parse_cell_input(&json!(2)).expect("cell"))]),
        )]);
        let merged = r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><mergeCells><mergeCell ref="A1:B2"/></mergeCells></worksheet>"#;
        assert!(reject_unsupported_update_intersections(merged, &updates)
            .expect_err("merged range")
            .to_string()
            .contains("merged cell"));
        let shared = r#"<worksheet><sheetData><row r="1"><c r="A1"><f t="shared" si="0">A2</f><v>1</v></c></row></sheetData></worksheet>"#;
        assert!(reject_unsupported_update_intersections(shared, &updates)
            .expect_err("shared formula")
            .to_string()
            .contains("shared"));
    }

    #[test]
    fn render_validation_rejects_active_external_and_network_formula_content() {
        let worksheets = parse_worksheets(&json!({
            "worksheets":[{"name":"Sheet1","rows":[["safe",1]]}]
        }))
        .expect("worksheets");
        let baseline_entries = workbook_entries(worksheets.as_slice()).expect("entries");
        let directory = tempfile::tempdir().expect("directory");

        let safe = directory.path().join("safe.xlsx");
        write_new_xlsx(safe.as_path(), baseline_entries.clone(), false).expect("safe XLSX");
        validate_xlsx_for_render(safe.as_path()).expect("safe render validation");

        let active = directory.path().join("active.xlsx");
        let mut active_entries = baseline_entries.clone();
        active_entries.push(("xl/vbaProject.bin".to_string(), vec![0, 1, 2]));
        write_new_xlsx(active.as_path(), active_entries, false).expect("active XLSX");
        assert!(validate_xlsx_for_render(active.as_path())
            .expect_err("active content")
            .to_string()
            .contains("active"));

        let external = directory.path().join("external.xlsx");
        let mut external_entries = baseline_entries.clone();
        let relationships = external_entries
            .iter_mut()
            .find(|(name, _)| name == "xl/_rels/workbook.xml.rels")
            .expect("workbook relationships");
        let xml = String::from_utf8(relationships.1.clone()).expect("relationships XML");
        relationships.1 = xml
            .replace(
                "</Relationships>",
                r#"<Relationship Id="external" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink" Target="https://example.invalid/book.xlsx" TargetMode="External"/></Relationships>"#,
            )
            .into_bytes();
        write_new_xlsx(external.as_path(), external_entries, false).expect("external XLSX");
        assert!(validate_xlsx_for_render(external.as_path())
            .expect_err("external relationship")
            .to_string()
            .contains("external non-hyperlink"));

        let network_formula = directory.path().join("network-formula.xlsx");
        let mut network_entries = baseline_entries;
        let worksheet = network_entries
            .iter_mut()
            .find(|(name, _)| name == "xl/worksheets/sheet1.xml")
            .expect("worksheet");
        let xml = String::from_utf8(worksheet.1.clone()).expect("worksheet XML");
        worksheet.1 = xml
            .replace(
                "</sheetData>",
                r#"<row r="2"><c r="A2"><f>WEBSERVICE(A1)</f><v>0</v></c></row></sheetData>"#,
            )
            .into_bytes();
        write_new_xlsx(network_formula.as_path(), network_entries, false)
            .expect("network formula XLSX");
        assert!(validate_xlsx_for_render(network_formula.as_path())
            .expect_err("network formula")
            .to_string()
            .contains("network"));
    }
}
