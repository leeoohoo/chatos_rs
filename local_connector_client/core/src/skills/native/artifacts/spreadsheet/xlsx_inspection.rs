// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::File;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use quick_xml::escape::unescape;
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use serde_json::{json, Value};
use zip::ZipArchive;

use super::super::{file_size, read_zip_text};
use super::parse_cell_reference;
use super::xlsx_package::{
    optional_attribute, parse_relationships, read_workbook_parts, validate_xlsx_package,
    workbook_sheet_parts,
};

pub(super) fn inspect_xlsx(path: &Path, relative: &str) -> Result<Value> {
    let package_names = validate_xlsx_package(path)?;
    let (workbook_xml, relationships_xml) = read_workbook_parts(path)?;
    let sheets = workbook_sheet_parts(workbook_xml.as_str(), relationships_xml.as_str())?;
    let mut archive = ZipArchive::new(File::open(path)?)
        .with_context(|| format!("open XLSX {}", path.display()))?;
    let mut metadata = Vec::with_capacity(sheets.len());
    let mut total_cells = 0usize;
    let mut total_formulas = 0usize;
    for sheet in &sheets {
        if !package_names.contains(sheet.path.as_str()) {
            return Err(anyhow!("XLSX is missing worksheet part: {}", sheet.path));
        }
        let xml = read_zip_text(&mut archive, sheet.path.as_str())?;
        let inspection = inspect_worksheet(xml.as_str())?;
        total_cells = total_cells.saturating_add(inspection.cells);
        total_formulas = total_formulas.saturating_add(inspection.formulas);
        metadata.push(json!({
            "name": sheet.name,
            "rows": inspection.max_row,
            "columns": inspection.max_column,
            "cells": inspection.cells,
            "formula_cells": inspection.formulas,
            "frozen_rows": inspection.frozen_rows,
            "custom_column_widths": inspection.custom_column_widths,
        }));
    }
    Ok(json!({
        "path": relative,
        "format": "xlsx",
        "bytes": file_size(path)?,
        "worksheets": sheets.len(),
        "sheet_names": sheets.iter().map(|sheet| sheet.name.as_str()).collect::<Vec<_>>(),
        "sheets": metadata,
        "cells": total_cells,
        "formula_cells": total_formulas,
        "recalculation_on_open": workbook_requests_recalculation(workbook_xml.as_str())?,
    }))
}

pub(super) fn validate_xlsx_for_render(path: &Path) -> Result<()> {
    let package_names = validate_xlsx_package(path)?;
    for name in &package_names {
        let lower = name.to_ascii_lowercase();
        if lower == "xl/vbaproject.bin"
            || lower == "xl/connections.xml"
            || lower.starts_with("xl/externallinks/")
            || lower.starts_with("xl/activex/")
            || lower.starts_with("xl/embeddings/")
            || lower.starts_with("xl/model/")
            || lower.starts_with("xl/webextensions/")
            || lower.starts_with("customui/")
        {
            return Err(anyhow!(
                "XLSX rendering rejects active, embedded, connected, or external workbook content: {name}"
            ));
        }
    }

    let mut archive = ZipArchive::new(File::open(path)?)
        .with_context(|| format!("open XLSX {} for render validation", path.display()))?;
    let content_types = read_zip_text(&mut archive, "[Content_Types].xml")?;
    let normalized_content_types = content_types.to_ascii_lowercase();
    if [
        "macroenabled",
        "vbaproject",
        "activex",
        "oleobject",
        "externalconnection",
    ]
    .iter()
    .any(|token| normalized_content_types.contains(token))
    {
        return Err(anyhow!(
            "XLSX rendering rejects active or externally connected content types"
        ));
    }

    let mut names = package_names.into_iter().collect::<Vec<_>>();
    names.sort();
    for name in names.iter().filter(|name| name.ends_with(".rels")) {
        let relationships = read_zip_text(&mut archive, name.as_str())?;
        for (_, (_, relationship_type, external)) in parse_relationships(&relationships)? {
            if external && !relationship_type.ends_with("/hyperlink") {
                return Err(anyhow!(
                    "XLSX rendering rejects external non-hyperlink relationships"
                ));
            }
        }
    }
    for name in names.iter().filter(|name| {
        name.starts_with("xl/")
            && name.ends_with(".xml")
            && !name.ends_with("styles.xml")
            && !name.ends_with("sharedStrings.xml")
    }) {
        let xml = read_zip_text(&mut archive, name.as_str())?;
        reject_unsafe_render_formulas(xml.as_str())?;
    }
    Ok(())
}

fn reject_unsafe_render_formulas(xml: &str) -> Result<()> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut formula_tag = None::<Vec<u8>>;
    let mut formula = String::new();
    loop {
        match reader
            .read_event()
            .context("parse XLSX render-safety XML")?
        {
            Event::Start(event) if is_formula_element(event.local_name().as_ref()) => {
                if formula_tag.is_some() {
                    return Err(anyhow!("XLSX formula markup is unexpectedly nested"));
                }
                formula_tag = Some(event.local_name().as_ref().to_vec());
                formula.clear();
            }
            Event::Text(text) if formula_tag.is_some() => {
                let decoded = text
                    .xml_content(XmlVersion::Explicit1_0)
                    .context("decode XLSX formula text")?;
                let decoded = unescape(decoded.as_ref()).context("unescape XLSX formula text")?;
                formula.push_str(decoded.as_ref());
            }
            Event::GeneralRef(_) | Event::CData(_) if formula_tag.is_some() => {
                return Err(anyhow!(
                    "XLSX rendering rejects formula character references and CDATA"
                ));
            }
            Event::End(event)
                if formula_tag
                    .as_deref()
                    .is_some_and(|tag| tag == event.local_name().as_ref()) =>
            {
                reject_unsafe_render_formula(formula.as_str())?;
                formula_tag = None;
                formula.clear();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if formula_tag.is_some() {
        return Err(anyhow!("XLSX formula markup is incomplete"));
    }
    Ok(())
}

fn is_formula_element(name: &[u8]) -> bool {
    name == b"f"
        || name == b"definedName"
        || String::from_utf8_lossy(name)
            .to_ascii_lowercase()
            .contains("formula")
}

fn reject_unsafe_render_formula(formula: &str) -> Result<()> {
    let normalized = formula.to_ascii_uppercase();
    let forbidden = [
        "WEBSERVICE(",
        "RTD(",
        "DDE(",
        "EXEC(",
        "CALL(",
        "REGISTER.ID(",
        "IMAGE(",
        "HTTP://",
        "HTTPS://",
        "FTP://",
        "FILE://",
    ];
    if forbidden.iter().any(|token| normalized.contains(token)) || normalized.contains('|') {
        return Err(anyhow!(
            "XLSX rendering rejects network, dynamic-data, external-file, or executable formulas"
        ));
    }
    Ok(())
}

#[derive(Default)]
pub(super) struct WorksheetInspection {
    cells: usize,
    formulas: usize,
    pub(super) max_row: u32,
    pub(super) max_column: u16,
    frozen_rows: u32,
    custom_column_widths: usize,
}

pub(super) fn inspect_worksheet(xml: &str) -> Result<WorksheetInspection> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut inspection = WorksheetInspection::default();
    loop {
        match reader.read_event().context("parse XLSX worksheet XML")? {
            Event::Start(event) | Event::Empty(event) if event.local_name().as_ref() == b"c" => {
                inspection.cells = inspection.cells.saturating_add(1);
                if let Some(reference) = optional_attribute(&reader, &event, "r")? {
                    let (column, row) = parse_cell_reference(reference.as_str())?;
                    inspection.max_column = inspection.max_column.max(column);
                    inspection.max_row = inspection.max_row.max(row);
                }
            }
            Event::Start(event) | Event::Empty(event) if event.local_name().as_ref() == b"f" => {
                inspection.formulas = inspection.formulas.saturating_add(1);
            }
            Event::Start(event) | Event::Empty(event) if event.local_name().as_ref() == b"pane" => {
                if optional_attribute(&reader, &event, "state")?
                    .is_some_and(|state| state == "frozen" || state == "frozenSplit")
                {
                    inspection.frozen_rows = optional_attribute(&reader, &event, "ySplit")?
                        .and_then(|value| value.parse::<f64>().ok())
                        .map(|value| value.floor().max(0.0) as u32)
                        .unwrap_or(0)
                        .min(1_000);
                }
            }
            Event::Start(event) | Event::Empty(event) if event.local_name().as_ref() == b"col" => {
                if optional_attribute(&reader, &event, "customWidth")?
                    .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
                {
                    inspection.custom_column_widths =
                        inspection.custom_column_widths.saturating_add(1);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(inspection)
}

fn workbook_requests_recalculation(xml: &str) -> Result<bool> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    loop {
        match reader
            .read_event()
            .context("parse XLSX workbook recalculation metadata")?
        {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"calcPr" =>
            {
                return Ok(optional_attribute(&reader, &event, "fullCalcOnLoad")?
                    .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
                    || optional_attribute(&reader, &event, "forceFullCalc")?
                        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true")));
            }
            Event::Eof => return Ok(false),
            _ => {}
        }
    }
}
