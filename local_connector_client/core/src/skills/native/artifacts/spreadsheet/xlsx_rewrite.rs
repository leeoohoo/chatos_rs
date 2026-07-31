// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use anyhow::{anyhow, Context, Result};
use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::{Reader, Writer, XmlVersion};

use super::xlsx_generation::cell_xml;
use super::xlsx_inspection::inspect_worksheet;
use super::xlsx_model::{CellInput, NumberFormat};
use super::xlsx_package::{optional_attribute, required_attribute};
use super::{cell_reference, parse_cell_reference, MAX_XLSX_ROWS, MAX_XML_BYTES};

pub(super) fn rewrite_worksheet(
    xml: &str,
    mut updates: BTreeMap<u32, BTreeMap<u16, CellInput>>,
    style_ids: &BTreeMap<NumberFormat, u32>,
) -> Result<String> {
    let existing = inspect_worksheet(xml)?;
    let update_max_row = updates.keys().next_back().copied().unwrap_or(1);
    let update_max_column = updates
        .values()
        .flat_map(BTreeMap::keys)
        .next_back()
        .copied()
        .unwrap_or(1);
    let dimension = format!(
        "A1:{}",
        cell_reference(
            existing.max_column.max(update_max_column).max(1),
            existing.max_row.max(update_max_row).max(1)
        )
    );
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(xml.len() + 4096));
    let mut found_sheet_data = false;
    loop {
        match reader.read_event().context("parse XLSX worksheet XML")? {
            Event::Empty(event) if event.local_name().as_ref() == b"dimension" => {
                writer.write_event(Event::Empty(rebuild_start_with_attribute(
                    &reader,
                    &event,
                    "ref",
                    dimension.as_str(),
                )?))?;
            }
            Event::Start(event) if event.local_name().as_ref() == b"sheetData" => {
                if event_name(&event)?.contains(':') {
                    return Err(anyhow!(
                        "XLSX range updates do not support prefixed spreadsheet namespaces"
                    ));
                }
                found_sheet_data = true;
                writer.write_event(Event::Start(event.into_owned()))?;
                rewrite_sheet_data(&mut reader, &mut writer, &mut updates, style_ids)?;
            }
            Event::Eof => break,
            event => writer.write_event(event.into_owned())?,
        }
    }
    if !found_sheet_data || !updates.is_empty() {
        return Err(anyhow!(
            "XLSX worksheet is missing a writable sheetData section"
        ));
    }
    let output = writer.into_inner();
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated XLSX worksheet exceeds the XML size limit"));
    }
    String::from_utf8(output).context("encode updated XLSX worksheet XML")
}

fn rewrite_sheet_data(
    reader: &mut Reader<&[u8]>,
    writer: &mut Writer<Vec<u8>>,
    updates: &mut BTreeMap<u32, BTreeMap<u16, CellInput>>,
    style_ids: &BTreeMap<NumberFormat, u32>,
) -> Result<()> {
    let mut last_row = 0u32;
    loop {
        match reader.read_event().context("rewrite XLSX sheetData")? {
            Event::Start(event) if event.local_name().as_ref() == b"row" => {
                let row = required_attribute(reader, &event, "r")?
                    .parse::<u32>()
                    .context("XLSX row reference is not numeric")?;
                if row == 0 || row > MAX_XLSX_ROWS || row <= last_row {
                    return Err(anyhow!("XLSX worksheet rows are invalid or not ordered"));
                }
                write_missing_rows_before(writer, updates, row, style_ids)?;
                let row_updates = updates.remove(&row).unwrap_or_default();
                rewrite_existing_row(
                    reader,
                    writer,
                    event.into_owned(),
                    row,
                    row_updates,
                    style_ids,
                )?;
                last_row = row;
            }
            Event::Empty(event) if event.local_name().as_ref() == b"row" => {
                let row = required_attribute(reader, &event, "r")?
                    .parse::<u32>()
                    .context("XLSX row reference is not numeric")?;
                if row == 0 || row > MAX_XLSX_ROWS || row <= last_row {
                    return Err(anyhow!("XLSX worksheet rows are invalid or not ordered"));
                }
                write_missing_rows_before(writer, updates, row, style_ids)?;
                if let Some(row_updates) = updates.remove(&row) {
                    writer.write_event(Event::Start(event.into_owned()))?;
                    write_update_cells(writer, row, row_updates, style_ids)?;
                    writer.write_event(Event::End(BytesEnd::new("row")))?;
                } else {
                    writer.write_event(Event::Empty(event.into_owned()))?;
                }
                last_row = row;
            }
            Event::End(event) if event.local_name().as_ref() == b"sheetData" => {
                let remaining = std::mem::take(updates);
                for (row, cells) in remaining {
                    write_new_row(writer, row, cells, style_ids)?;
                }
                writer.write_event(Event::End(event.into_owned()))?;
                return Ok(());
            }
            Event::Eof => return Err(anyhow!("XLSX sheetData is not closed")),
            event => writer.write_event(event.into_owned())?,
        }
    }
}

fn write_missing_rows_before(
    writer: &mut Writer<Vec<u8>>,
    updates: &mut BTreeMap<u32, BTreeMap<u16, CellInput>>,
    before: u32,
    style_ids: &BTreeMap<NumberFormat, u32>,
) -> Result<()> {
    let rows = updates
        .range(..before)
        .map(|(row, _)| *row)
        .collect::<Vec<_>>();
    for row in rows {
        let cells = updates
            .remove(&row)
            .ok_or_else(|| anyhow!("XLSX update row disappeared"))?;
        write_new_row(writer, row, cells, style_ids)?;
    }
    Ok(())
}

fn write_new_row(
    writer: &mut Writer<Vec<u8>>,
    row: u32,
    cells: BTreeMap<u16, CellInput>,
    style_ids: &BTreeMap<NumberFormat, u32>,
) -> Result<()> {
    let mut start = BytesStart::new("row");
    let row_text = row.to_string();
    start.push_attribute(("r", row_text.as_str()));
    writer.write_event(Event::Start(start))?;
    write_update_cells(writer, row, cells, style_ids)?;
    writer.write_event(Event::End(BytesEnd::new("row")))?;
    Ok(())
}

fn rewrite_existing_row(
    reader: &mut Reader<&[u8]>,
    writer: &mut Writer<Vec<u8>>,
    row_start: BytesStart<'static>,
    row: u32,
    mut updates: BTreeMap<u16, CellInput>,
    style_ids: &BTreeMap<NumberFormat, u32>,
) -> Result<()> {
    writer.write_event(Event::Start(row_start))?;
    let mut last_column = 0u16;
    let mut flushed_remaining = false;
    loop {
        match reader.read_event().context("rewrite XLSX row")? {
            Event::Start(event) if event.local_name().as_ref() == b"c" => {
                let reference = required_attribute(reader, &event, "r")?;
                let (column, cell_row) = parse_cell_reference(reference.as_str())?;
                if cell_row != row || column <= last_column {
                    return Err(anyhow!("XLSX worksheet cells are invalid or not ordered"));
                }
                write_missing_cells_before(writer, row, &mut updates, column, style_ids)?;
                if let Some(update) = updates.remove(&column) {
                    let existing_style = optional_attribute(reader, &event, "s")?
                        .and_then(|value| value.parse::<u32>().ok());
                    write_one_update_cell(writer, column, row, &update, style_ids, existing_style)?;
                    skip_element(reader, b"c")?;
                } else {
                    writer.write_event(Event::Start(event.into_owned()))?;
                    copy_element_body(reader, writer, b"c")?;
                }
                last_column = column;
            }
            Event::Empty(event) if event.local_name().as_ref() == b"c" => {
                let reference = required_attribute(reader, &event, "r")?;
                let (column, cell_row) = parse_cell_reference(reference.as_str())?;
                if cell_row != row || column <= last_column {
                    return Err(anyhow!("XLSX worksheet cells are invalid or not ordered"));
                }
                write_missing_cells_before(writer, row, &mut updates, column, style_ids)?;
                if let Some(update) = updates.remove(&column) {
                    let existing_style = optional_attribute(reader, &event, "s")?
                        .and_then(|value| value.parse::<u32>().ok());
                    write_one_update_cell(writer, column, row, &update, style_ids, existing_style)?;
                } else {
                    writer.write_event(Event::Empty(event.into_owned()))?;
                }
                last_column = column;
            }
            Event::Start(event) if !flushed_remaining => {
                write_update_cells(writer, row, std::mem::take(&mut updates), style_ids)?;
                flushed_remaining = true;
                writer.write_event(Event::Start(event.into_owned()))?;
            }
            Event::Empty(event) if !flushed_remaining => {
                write_update_cells(writer, row, std::mem::take(&mut updates), style_ids)?;
                flushed_remaining = true;
                writer.write_event(Event::Empty(event.into_owned()))?;
            }
            Event::End(event) if event.local_name().as_ref() == b"row" => {
                if !flushed_remaining {
                    write_update_cells(writer, row, updates, style_ids)?;
                }
                writer.write_event(Event::End(event.into_owned()))?;
                return Ok(());
            }
            Event::Eof => return Err(anyhow!("XLSX row is not closed")),
            event => writer.write_event(event.into_owned())?,
        }
    }
}

fn write_missing_cells_before(
    writer: &mut Writer<Vec<u8>>,
    row: u32,
    updates: &mut BTreeMap<u16, CellInput>,
    before: u16,
    style_ids: &BTreeMap<NumberFormat, u32>,
) -> Result<()> {
    let columns = updates
        .range(..before)
        .map(|(column, _)| *column)
        .collect::<Vec<_>>();
    for column in columns {
        let cell = updates
            .remove(&column)
            .ok_or_else(|| anyhow!("XLSX update cell disappeared"))?;
        write_one_update_cell(writer, column, row, &cell, style_ids, None)?;
    }
    Ok(())
}

fn write_update_cells(
    writer: &mut Writer<Vec<u8>>,
    row: u32,
    cells: BTreeMap<u16, CellInput>,
    style_ids: &BTreeMap<NumberFormat, u32>,
) -> Result<()> {
    for (column, cell) in cells {
        write_one_update_cell(writer, column, row, &cell, style_ids, None)?;
    }
    Ok(())
}

fn write_one_update_cell(
    writer: &mut Writer<Vec<u8>>,
    column: u16,
    row: u32,
    cell: &CellInput,
    style_ids: &BTreeMap<NumberFormat, u32>,
    existing_style: Option<u32>,
) -> Result<()> {
    let style_id = match cell.number_format {
        Some(format) => Some(
            *style_ids
                .get(&format)
                .ok_or_else(|| anyhow!("XLSX update style mapping is missing"))?,
        ),
        None => existing_style,
    };
    writer
        .get_mut()
        .extend_from_slice(cell_xml(column, row, cell, style_id)?.as_bytes());
    Ok(())
}

fn skip_element(reader: &mut Reader<&[u8]>, local_name: &[u8]) -> Result<()> {
    let mut depth = 1usize;
    while depth > 0 {
        match reader
            .read_event()
            .context("skip replaced XLSX XML element")?
        {
            Event::Start(event) if event.local_name().as_ref() == local_name => depth += 1,
            Event::End(event) if event.local_name().as_ref() == local_name => depth -= 1,
            Event::Eof => return Err(anyhow!("XLSX XML element is not closed")),
            _ => {}
        }
    }
    Ok(())
}

fn copy_element_body(
    reader: &mut Reader<&[u8]>,
    writer: &mut Writer<Vec<u8>>,
    local_name: &[u8],
) -> Result<()> {
    let mut depth = 1usize;
    while depth > 0 {
        let event = reader.read_event().context("copy XLSX XML element")?;
        match &event {
            Event::Start(start) if start.local_name().as_ref() == local_name => depth += 1,
            Event::End(end) if end.local_name().as_ref() == local_name => depth -= 1,
            Event::Eof => return Err(anyhow!("XLSX XML element is not closed")),
            _ => {}
        }
        writer.write_event(event.into_owned())?;
    }
    Ok(())
}

fn rebuild_start_with_attribute(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    replaced_key: &str,
    replaced_value: &str,
) -> Result<BytesStart<'static>> {
    let mut rebuilt = BytesStart::new(event_name(event)?);
    for attribute in event.attributes().with_checks(false) {
        let attribute = attribute.context("parse XLSX XML attribute")?;
        let key = std::str::from_utf8(attribute.key.as_ref())?.to_string();
        if key.rsplit(':').next() == Some(replaced_key) {
            continue;
        }
        let value = attribute
            .decoded_and_normalized_value(XmlVersion::Explicit1_0, reader.decoder())?
            .into_owned();
        rebuilt.push_attribute((key.as_str(), value.as_str()));
    }
    rebuilt.push_attribute((replaced_key, replaced_value));
    Ok(rebuilt.into_owned())
}

pub(super) fn event_name(event: &BytesStart<'_>) -> Result<String> {
    Ok(std::str::from_utf8(event.name().as_ref())?.to_string())
}

pub(super) fn force_formula_recalculation(xml: &str) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(xml.len() + 128));
    let mut found = false;
    loop {
        match reader.read_event().context("parse XLSX workbook XML")? {
            Event::Empty(event) if event.local_name().as_ref() == b"calcPr" => {
                found = true;
                writer.write_event(Event::Empty(calc_pr_event()))?;
            }
            Event::Start(event) if event.local_name().as_ref() == b"calcPr" => {
                found = true;
                writer.write_event(Event::Empty(calc_pr_event()))?;
                skip_element(&mut reader, b"calcPr")?;
            }
            Event::End(event) if event.local_name().as_ref() == b"workbook" => {
                if std::str::from_utf8(event.name().as_ref())?.contains(':') {
                    return Err(anyhow!(
                        "formula XLSX updates do not support prefixed spreadsheet namespaces"
                    ));
                }
                if !found {
                    writer.write_event(Event::Empty(calc_pr_event()))?;
                }
                writer.write_event(Event::End(event.into_owned()))?;
            }
            Event::Eof => break,
            event => writer.write_event(event.into_owned())?,
        }
    }
    String::from_utf8(writer.into_inner()).context("encode updated XLSX workbook XML")
}

fn calc_pr_event() -> BytesStart<'static> {
    let mut event = BytesStart::new("calcPr");
    event.push_attribute(("calcId", "0"));
    event.push_attribute(("calcMode", "auto"));
    event.push_attribute(("fullCalcOnLoad", "1"));
    event.push_attribute(("forceFullCalc", "1"));
    event.into_owned()
}
