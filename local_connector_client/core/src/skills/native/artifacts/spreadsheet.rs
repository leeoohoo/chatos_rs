// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Component, Path};

use anyhow::{anyhow, Context, Result};
use quick_xml::escape::unescape;
use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::{Reader, Writer, XmlVersion};
use serde_json::{json, Value};
use tempfile::NamedTempFile;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::{
    file_size, input_file, optional_bool, optional_text, read_zip_text, require_extension,
    required_text, safe_workspace_path, MAX_ARTIFACT_BYTES, MAX_TABLE_CELLS, MAX_XML_BYTES,
};

const MAX_XLSX_SHEETS: usize = 64;
const MAX_XLSX_ROWS: u32 = 1_048_576;
const MAX_XLSX_COLUMNS: u16 = 16_384;
const MAX_XLSX_ZIP_ENTRIES: usize = 10_000;
const MAX_FORMULA_BYTES: usize = 4_096;
const MAX_CELL_TEXT_CHARS: usize = 32_767;
const MAX_COLUMN_WIDTH: f64 = 255.0;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum NumberFormat {
    Integer,
    Decimal2,
    Percent2,
    Date,
    DateTime,
}

impl NumberFormat {
    fn parse(value: &str) -> Result<Option<Self>> {
        match value {
            "general" => Ok(None),
            "integer" => Ok(Some(Self::Integer)),
            "decimal_2" => Ok(Some(Self::Decimal2)),
            "percent_2" => Ok(Some(Self::Percent2)),
            "date" => Ok(Some(Self::Date)),
            "datetime" => Ok(Some(Self::DateTime)),
            _ => Err(anyhow!("unsupported XLSX number_format: {value}")),
        }
    }

    fn built_in_id(self) -> u32 {
        match self {
            Self::Integer => 1,
            Self::Decimal2 => 2,
            Self::Percent2 => 10,
            Self::Date => 14,
            Self::DateTime => 22,
        }
    }

    fn generated_style_id(self) -> u32 {
        match self {
            Self::Integer => 1,
            Self::Decimal2 => 2,
            Self::Percent2 => 3,
            Self::Date => 4,
            Self::DateTime => 5,
        }
    }
}

#[derive(Clone, Debug)]
enum PrimitiveCellValue {
    Blank,
    Bool(bool),
    Number(String),
    Text(String),
}

#[derive(Clone, Debug)]
enum CellValue {
    Primitive(PrimitiveCellValue),
    Formula {
        expression: String,
        cached_value: Option<PrimitiveCellValue>,
    },
}

#[derive(Clone, Debug)]
struct CellInput {
    value: CellValue,
    number_format: Option<NumberFormat>,
}

#[derive(Debug)]
struct WorksheetInput {
    name: String,
    rows: Vec<Vec<CellInput>>,
    freeze_rows: u32,
    column_widths: BTreeMap<u16, f64>,
}

#[derive(Clone, Debug)]
struct SheetPart {
    name: String,
    path: String,
}

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

fn parse_worksheets(arguments: &Value) -> Result<Vec<WorksheetInput>> {
    let worksheets = if let Some(items) = arguments.get("worksheets") {
        if arguments.get("rows").is_some() || arguments.get("sheet_name").is_some() {
            return Err(anyhow!(
                "create_xlsx accepts either worksheets or legacy rows/sheet_name, not both"
            ));
        }
        let items = items
            .as_array()
            .ok_or_else(|| anyhow!("worksheets must be an array"))?;
        if items.is_empty() || items.len() > MAX_XLSX_SHEETS {
            return Err(anyhow!(
                "worksheets must contain between 1 and {MAX_XLSX_SHEETS} items"
            ));
        }
        let mut output = Vec::with_capacity(items.len());
        for item in items {
            let object = item
                .as_object()
                .ok_or_else(|| anyhow!("each worksheet must be an object"))?;
            let name = object
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("worksheet name is required"))?
                .to_string();
            validate_sheet_name(name.as_str())?;
            let rows = parse_cell_rows(
                object
                    .get("rows")
                    .ok_or_else(|| anyhow!("worksheet rows are required"))?,
                "worksheet rows",
            )?;
            let freeze_rows = object
                .get("freeze_rows")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if freeze_rows > 1_000 || freeze_rows > u64::from(MAX_XLSX_ROWS - 1) {
                return Err(anyhow!("freeze_rows must be between 0 and 1000"));
            }
            let column_widths = parse_column_widths(object.get("column_widths"))?;
            output.push(WorksheetInput {
                name,
                rows,
                freeze_rows: freeze_rows as u32,
                column_widths,
            });
        }
        output
    } else {
        let name = optional_text(arguments, "sheet_name").unwrap_or_else(|| "Sheet1".to_string());
        validate_sheet_name(name.as_str())?;
        vec![WorksheetInput {
            name,
            rows: parse_cell_rows(
                arguments
                    .get("rows")
                    .ok_or_else(|| anyhow!("rows is required"))?,
                "rows",
            )?,
            freeze_rows: 0,
            column_widths: BTreeMap::new(),
        }]
    };
    let mut names = HashSet::new();
    for sheet in &worksheets {
        if !names.insert(sheet.name.to_lowercase()) {
            return Err(anyhow!(
                "XLSX worksheet names must be unique case-insensitively"
            ));
        }
    }
    let cells = worksheets
        .iter()
        .flat_map(|sheet| sheet.rows.iter())
        .map(Vec::len)
        .sum::<usize>();
    if cells > MAX_TABLE_CELLS {
        return Err(anyhow!(
            "workbook exceeds the {MAX_TABLE_CELLS} cell safety limit"
        ));
    }
    Ok(worksheets)
}

fn parse_cell_rows(value: &Value, label: &str) -> Result<Vec<Vec<CellInput>>> {
    let rows = value
        .as_array()
        .ok_or_else(|| anyhow!("{label} must be an array"))?;
    if rows.len() > MAX_XLSX_ROWS as usize {
        return Err(anyhow!("{label} exceeds the XLSX row limit"));
    }
    let mut cells = 0usize;
    let mut output = Vec::with_capacity(rows.len());
    for row in rows {
        let row = row
            .as_array()
            .ok_or_else(|| anyhow!("each {label} row must be an array"))?;
        if row.len() > MAX_XLSX_COLUMNS as usize {
            return Err(anyhow!("{label} row exceeds the XLSX column limit"));
        }
        cells = cells.saturating_add(row.len());
        if cells > MAX_TABLE_CELLS {
            return Err(anyhow!(
                "{label} exceeds the {MAX_TABLE_CELLS} cell safety limit"
            ));
        }
        output.push(
            row.iter()
                .map(parse_cell_input)
                .collect::<Result<Vec<_>>>()?,
        );
    }
    Ok(output)
}

fn parse_cell_input(value: &Value) -> Result<CellInput> {
    if let Some(object) = value.as_object() {
        if object.keys().any(|key| {
            !matches!(
                key.as_str(),
                "value" | "formula" | "cached_value" | "number_format"
            )
        }) {
            return Err(anyhow!("XLSX cell object contains an unsupported field"));
        }
        let number_format = object
            .get("number_format")
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| anyhow!("number_format must be a string"))
                    .and_then(NumberFormat::parse)
            })
            .transpose()?
            .flatten();
        let cell_value = if let Some(formula) = object.get("formula") {
            if object.contains_key("value") {
                return Err(anyhow!("formula cells cannot also contain value"));
            }
            let expression = validate_formula(
                formula
                    .as_str()
                    .ok_or_else(|| anyhow!("formula must be a string"))?,
            )?;
            let cached_value = object
                .get("cached_value")
                .map(parse_primitive_cell_value)
                .transpose()?;
            CellValue::Formula {
                expression,
                cached_value,
            }
        } else {
            if object.contains_key("cached_value") {
                return Err(anyhow!("cached_value is only valid for formula cells"));
            }
            CellValue::Primitive(parse_primitive_cell_value(
                object
                    .get("value")
                    .ok_or_else(|| anyhow!("XLSX cell object requires value or formula"))?,
            )?)
        };
        if number_format.is_some() {
            let incompatible = matches!(
                &cell_value,
                CellValue::Primitive(PrimitiveCellValue::Text(_) | PrimitiveCellValue::Bool(_))
                    | CellValue::Formula {
                        cached_value: Some(
                            PrimitiveCellValue::Text(_) | PrimitiveCellValue::Bool(_)
                        ),
                        ..
                    }
            );
            if incompatible {
                return Err(anyhow!(
                    "XLSX number_format requires a numeric value, blank value, or formula with a numeric cached_value"
                ));
            }
        }
        Ok(CellInput {
            value: cell_value,
            number_format,
        })
    } else {
        Ok(CellInput {
            value: CellValue::Primitive(parse_primitive_cell_value(value)?),
            number_format: None,
        })
    }
}

fn parse_primitive_cell_value(value: &Value) -> Result<PrimitiveCellValue> {
    match value {
        Value::Null => Ok(PrimitiveCellValue::Blank),
        Value::Bool(value) => Ok(PrimitiveCellValue::Bool(*value)),
        Value::Number(value) => Ok(PrimitiveCellValue::Number(value.to_string())),
        Value::String(value) => {
            if value.chars().count() > MAX_CELL_TEXT_CHARS {
                return Err(anyhow!(
                    "XLSX cell text exceeds the {MAX_CELL_TEXT_CHARS} character limit"
                ));
            }
            Ok(PrimitiveCellValue::Text(value.clone()))
        }
        _ => Err(anyhow!(
            "XLSX cell values must be null, boolean, number, string, or a supported cell object"
        )),
    }
}

fn parse_column_widths(value: Option<&Value>) -> Result<BTreeMap<u16, f64>> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let entries = value
        .as_array()
        .ok_or_else(|| anyhow!("column_widths must be an array"))?;
    if entries.len() > 256 {
        return Err(anyhow!("column_widths exceeds the 256 item limit"));
    }
    let mut widths = BTreeMap::new();
    for entry in entries {
        let object = entry
            .as_object()
            .ok_or_else(|| anyhow!("each column_widths item must be an object"))?;
        let column = parse_column_reference(
            object
                .get("column")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("column_widths column is required"))?,
        )?;
        let width = object
            .get("width")
            .and_then(Value::as_f64)
            .ok_or_else(|| anyhow!("column_widths width must be a number"))?;
        if !width.is_finite() || !(0.1..=MAX_COLUMN_WIDTH).contains(&width) {
            return Err(anyhow!(
                "column_widths width must be between 0.1 and {MAX_COLUMN_WIDTH}"
            ));
        }
        if widths.insert(column, width).is_some() {
            return Err(anyhow!("column_widths contains a duplicate column"));
        }
    }
    Ok(widths)
}

fn validate_sheet_name(value: &str) -> Result<()> {
    let chars = value.chars().count();
    if chars == 0 || chars > 31 || value.trim().is_empty() {
        return Err(anyhow!(
            "worksheet name must contain between 1 and 31 characters"
        ));
    }
    if value.starts_with('\'')
        || value.ends_with('\'')
        || value.chars().any(|character| {
            character.is_control() || matches!(character, ':' | '\\' | '/' | '?' | '*' | '[' | ']')
        })
    {
        return Err(anyhow!("worksheet name contains an unsupported character"));
    }
    Ok(())
}

fn validate_formula(value: &str) -> Result<String> {
    let expression = value
        .trim()
        .strip_prefix('=')
        .unwrap_or(value.trim())
        .trim();
    if expression.is_empty() || expression.len() > MAX_FORMULA_BYTES {
        return Err(anyhow!(
            "formula must contain between 1 and {MAX_FORMULA_BYTES} bytes"
        ));
    }
    if !expression.is_ascii()
        || expression.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'_' | b'.'
                        | b'$'
                        | b':'
                        | b','
                        | b'+'
                        | b'-'
                        | b'*'
                        | b'/'
                        | b'%'
                        | b'^'
                        | b'<'
                        | b'>'
                        | b'='
                        | b'('
                        | b')'
                        | b'!'
                        | b'\''
                        | b' '
                ))
        })
    {
        return Err(anyhow!(
            "formula contains unsupported characters; strings, external workbooks, and dynamic links are disabled"
        ));
    }
    let allowed_functions = [
        "ABS", "AND", "AVERAGE", "COUNT", "COUNTA", "IF", "MAX", "MIN", "NOT", "OR", "ROUND", "SUM",
    ];
    let bytes = expression.as_bytes();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        if bytes[cursor] == b'\'' {
            let end = expression[cursor + 1..]
                .find('\'')
                .map(|offset| cursor + 1 + offset)
                .ok_or_else(|| anyhow!("formula contains an unterminated worksheet name"))?;
            let sheet_name = &expression[cursor + 1..end];
            validate_sheet_name(sheet_name)?;
            if bytes.get(end + 1) != Some(&b'!') {
                return Err(anyhow!(
                    "quoted formula identifiers are only allowed as worksheet references"
                ));
            }
            cursor = end + 2;
            continue;
        }
        if bytes[cursor].is_ascii_alphabetic()
            || bytes[cursor] == b'_'
            || (bytes[cursor] == b'$' && bytes.get(cursor + 1).is_some_and(u8::is_ascii_alphabetic))
        {
            let start = cursor;
            cursor += 1;
            while cursor < bytes.len()
                && (bytes[cursor].is_ascii_alphanumeric()
                    || matches!(bytes[cursor], b'_' | b'.' | b'$'))
            {
                cursor += 1;
            }
            let mut lookahead = cursor;
            while lookahead < bytes.len() && bytes[lookahead] == b' ' {
                lookahead += 1;
            }
            if lookahead < bytes.len() && bytes[lookahead] == b'(' {
                let function = expression[start..cursor].to_ascii_uppercase();
                if !allowed_functions.contains(&function.as_str()) {
                    return Err(anyhow!(
                        "formula function is not in the local safety allowlist: {function}"
                    ));
                }
            } else {
                let identifier = &expression[start..cursor];
                let is_sheet_reference = bytes.get(lookahead) == Some(&b'!');
                let is_boolean =
                    matches!(identifier.to_ascii_uppercase().as_str(), "TRUE" | "FALSE");
                let is_cell_reference =
                    parse_cell_reference(identifier.replace('$', "").as_str()).is_ok();
                let exponent_digit_index = if matches!(bytes.get(lookahead), Some(b'+' | b'-')) {
                    lookahead + 1
                } else {
                    lookahead
                };
                let is_numeric_exponent = matches!(identifier, "E" | "e")
                    && start > 0
                    && bytes[start - 1].is_ascii_digit()
                    && bytes
                        .get(exponent_digit_index)
                        .is_some_and(u8::is_ascii_digit);
                if is_sheet_reference {
                    validate_sheet_name(identifier)?;
                }
                if !is_sheet_reference && !is_boolean && !is_cell_reference && !is_numeric_exponent
                {
                    return Err(anyhow!(
                        "formula named ranges are disabled; identifiers must be cells, booleans, safe functions, or worksheet references"
                    ));
                }
            }
        } else {
            cursor += 1;
        }
    }
    Ok(expression.to_string())
}

fn workbook_entries(worksheets: &[WorksheetInput]) -> Result<Vec<(String, Vec<u8>)>> {
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

impl NumberFormat {
    fn all() -> [Self; 5] {
        [
            Self::Integer,
            Self::Decimal2,
            Self::Percent2,
            Self::Date,
            Self::DateTime,
        ]
    }
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

fn cell_xml(column: u16, row: u32, cell: &CellInput, style_id: Option<u32>) -> Result<String> {
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

fn validate_xlsx_package(path: &Path) -> Result<HashSet<String>> {
    let mut archive = ZipArchive::new(File::open(path)?)
        .with_context(|| format!("open XLSX {}", path.display()))?;
    if archive.is_empty() || archive.len() > MAX_XLSX_ZIP_ENTRIES {
        return Err(anyhow!(
            "XLSX ZIP must contain between 1 and {MAX_XLSX_ZIP_ENTRIES} entries"
        ));
    }
    let mut names = HashSet::new();
    let mut expanded = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if entry.enclosed_name().is_none() || entry.is_symlink() || !names.insert(name) {
            return Err(anyhow!("XLSX ZIP contains an unsafe or duplicate entry"));
        }
        expanded = expanded.saturating_add(entry.size());
        if expanded > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "XLSX ZIP exceeds the 100 MiB expanded safety limit"
            ));
        }
    }
    Ok(names)
}

fn read_workbook_parts(path: &Path) -> Result<(String, String)> {
    let mut archive = ZipArchive::new(File::open(path)?)
        .with_context(|| format!("open XLSX {}", path.display()))?;
    Ok((
        read_zip_text(&mut archive, "xl/workbook.xml")?,
        read_zip_text(&mut archive, "xl/_rels/workbook.xml.rels")?,
    ))
}

fn workbook_sheet_parts(workbook_xml: &str, relationships_xml: &str) -> Result<Vec<SheetPart>> {
    let relationships = parse_relationships(relationships_xml)?;
    let mut reader = Reader::from_str(workbook_xml);
    reader.config_mut().trim_text(false);
    let mut sheets = Vec::new();
    loop {
        match reader.read_event().context("parse XLSX workbook XML")? {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"sheet" =>
            {
                let name = required_attribute(&reader, &event, "name")?;
                validate_sheet_name(name.as_str())?;
                let relationship_id = required_attribute(&reader, &event, "r:id")?;
                let (target, relationship_type, external) =
                    relationships.get(relationship_id.as_str()).ok_or_else(|| {
                        anyhow!("XLSX worksheet relationship is missing: {relationship_id}")
                    })?;
                if *external || !relationship_type.ends_with("/worksheet") {
                    return Err(anyhow!(
                        "XLSX worksheet relationship is not a local worksheet"
                    ));
                }
                sheets.push(SheetPart {
                    name,
                    path: resolve_workbook_target(target.as_str())?,
                });
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if sheets.is_empty() || sheets.len() > MAX_XLSX_SHEETS {
        return Err(anyhow!(
            "XLSX must contain between 1 and {MAX_XLSX_SHEETS} worksheets"
        ));
    }
    let mut names = HashSet::new();
    let mut paths = HashSet::new();
    for sheet in &sheets {
        if !names.insert(sheet.name.to_lowercase()) || !paths.insert(sheet.path.clone()) {
            return Err(anyhow!("XLSX contains duplicate worksheet names or parts"));
        }
    }
    Ok(sheets)
}

type RelationshipMap = HashMap<String, (String, String, bool)>;

fn parse_relationships(xml: &str) -> Result<RelationshipMap> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut relationships = HashMap::new();
    loop {
        match reader
            .read_event()
            .context("parse XLSX relationships XML")?
        {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"Relationship" =>
            {
                let id = required_attribute(&reader, &event, "Id")?;
                let target = required_attribute(&reader, &event, "Target")?;
                let relationship_type = required_attribute(&reader, &event, "Type")?;
                let external = optional_attribute(&reader, &event, "TargetMode")?
                    .is_some_and(|value| value.eq_ignore_ascii_case("external"));
                if relationships
                    .insert(id, (target, relationship_type, external))
                    .is_some()
                {
                    return Err(anyhow!("XLSX contains duplicate relationship IDs"));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(relationships)
}

fn workbook_styles_part(relationships_xml: &str) -> Result<Option<String>> {
    for (_, (target, relationship_type, external)) in parse_relationships(relationships_xml)? {
        if relationship_type.ends_with("/styles") {
            if external {
                return Err(anyhow!("XLSX styles relationship cannot be external"));
            }
            return Ok(Some(resolve_workbook_target(target.as_str())?));
        }
    }
    Ok(None)
}

fn resolve_workbook_target(target: &str) -> Result<String> {
    if target.is_empty() || target.contains('\\') || target.contains('\0') {
        return Err(anyhow!("XLSX relationship target is invalid"));
    }
    let normalized_target = target.strip_prefix('/').unwrap_or(target);
    let candidate = if normalized_target.starts_with("xl/") {
        Path::new(normalized_target).to_path_buf()
    } else {
        Path::new("xl").join(normalized_target)
    };
    let mut parts = Vec::new();
    for component in candidate.components() {
        match component {
            Component::Normal(value) => parts.push(
                value
                    .to_str()
                    .ok_or_else(|| anyhow!("XLSX relationship target is not UTF-8"))?,
            ),
            _ => return Err(anyhow!("XLSX relationship target escapes the package")),
        }
    }
    if parts.is_empty() {
        return Err(anyhow!("XLSX relationship target is empty"));
    }
    Ok(parts.join("/"))
}

fn required_attribute(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    name: &str,
) -> Result<String> {
    optional_attribute(reader, event, name)?
        .ok_or_else(|| anyhow!("XLSX XML element is missing required {name} attribute"))
}

fn optional_attribute(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    name: &str,
) -> Result<Option<String>> {
    for attribute in event.attributes().with_checks(false) {
        let attribute = attribute.context("parse XLSX XML attribute")?;
        let expected_local = name.rsplit(':').next().unwrap_or(name).as_bytes();
        if attribute.key.as_ref() == name.as_bytes()
            || attribute.key.as_ref().rsplit(|byte| *byte == b':').next() == Some(expected_local)
        {
            return Ok(Some(
                attribute
                    .decoded_and_normalized_value(XmlVersion::Explicit1_0, reader.decoder())
                    .context("decode XLSX XML attribute")?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

#[derive(Default)]
struct WorksheetInspection {
    cells: usize,
    formulas: usize,
    max_row: u32,
    max_column: u16,
    frozen_rows: u32,
    custom_column_widths: usize,
}

fn inspect_worksheet(xml: &str) -> Result<WorksheetInspection> {
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

fn rewrite_worksheet(
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

fn event_name(event: &BytesStart<'_>) -> Result<String> {
    Ok(std::str::from_utf8(event.name().as_ref())?.to_string())
}

fn force_formula_recalculation(xml: &str) -> Result<String> {
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

fn ensure_distinct_xlsx_paths(source: &Path, target: &Path) -> Result<()> {
    if source == target {
        return Err(anyhow!(
            "XLSX editing requires a distinct target_path; source files are never modified in place"
        ));
    }
    if target.exists() {
        let metadata = fs::symlink_metadata(target)
            .with_context(|| format!("inspect XLSX target {}", target.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(anyhow!(
                "XLSX target exists and is not a regular non-symlink file"
            ));
        }
        if source.canonicalize()? == target.canonicalize()? {
            return Err(anyhow!(
                "XLSX editing requires a distinct target_path; source files are never modified in place"
            ));
        }
    }
    Ok(())
}

fn rewrite_xlsx_package(
    source: &Path,
    target: &Path,
    replacements: &BTreeMap<String, Vec<u8>>,
    overwrite: bool,
) -> Result<u64> {
    if target.exists() && !overwrite {
        return Err(anyhow!(
            "refusing to overwrite existing XLSX without overwrite=true"
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("XLSX output path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create XLSX output directory {}", parent.display()))?;
    let mut archive = ZipArchive::new(File::open(source)?)
        .with_context(|| format!("open XLSX {}", source.display()))?;
    if archive.is_empty() || archive.len() > MAX_XLSX_ZIP_ENTRIES {
        return Err(anyhow!("XLSX ZIP entry count is outside the safety limit"));
    }
    let temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary XLSX in {}", parent.display()))?;
    let mut writer = ZipWriter::new(temporary);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut names = HashSet::new();
    let mut replaced = HashSet::new();
    let mut expanded = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if entry.enclosed_name().is_none() || entry.is_symlink() || !names.insert(name.clone()) {
            return Err(anyhow!("XLSX ZIP contains an unsafe or duplicate entry"));
        }
        expanded = expanded.saturating_add(
            replacements
                .get(name.as_str())
                .map_or(entry.size(), |content| content.len() as u64),
        );
        if expanded > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "edited XLSX exceeds the 100 MiB expanded safety limit"
            ));
        }
        if let Some(content) = replacements.get(name.as_str()) {
            writer.start_file(name.as_str(), options)?;
            writer.write_all(content)?;
            replaced.insert(name);
        } else if entry.is_dir() {
            writer.add_directory(name.as_str(), entry.options())?;
        } else {
            writer.raw_copy_file(entry)?;
        }
    }
    for name in replacements.keys() {
        if !replaced.contains(name) {
            return Err(anyhow!(
                "XLSX ZIP is missing required replacement entry: {name}"
            ));
        }
    }
    let temporary = writer.finish().context("finalize edited XLSX")?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("sync temporary XLSX for {}", target.display()))?;
    let bytes = temporary.as_file().metadata()?.len();
    if bytes > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("edited XLSX exceeds the 100 MiB safety limit"));
    }
    if target.exists() {
        fs::remove_file(target)
            .with_context(|| format!("replace existing XLSX {}", target.display()))?;
    }
    temporary
        .persist(target)
        .map_err(|error| anyhow!("persist XLSX {}: {}", target.display(), error.error))?;
    Ok(bytes)
}

fn write_new_xlsx(target: &Path, entries: Vec<(String, Vec<u8>)>, overwrite: bool) -> Result<u64> {
    if target.exists() {
        let metadata = fs::symlink_metadata(target)
            .with_context(|| format!("inspect XLSX target {}", target.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(anyhow!(
                "XLSX target exists and is not a regular non-symlink file"
            ));
        }
        if !overwrite {
            return Err(anyhow!(
                "refusing to overwrite existing XLSX without overwrite=true"
            ));
        }
    }
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("XLSX output path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create XLSX output directory {}", parent.display()))?;
    let temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary XLSX in {}", parent.display()))?;
    let mut writer = ZipWriter::new(temporary);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut names = HashSet::new();
    let mut expanded = 0u64;
    for (name, content) in entries {
        if name.is_empty() || !names.insert(name.clone()) {
            return Err(anyhow!(
                "generated XLSX contains an invalid or duplicate ZIP entry"
            ));
        }
        expanded = expanded.saturating_add(content.len() as u64);
        if expanded > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "generated XLSX exceeds the 100 MiB expanded safety limit"
            ));
        }
        writer.start_file(name.as_str(), options)?;
        writer.write_all(content.as_slice())?;
    }
    let temporary = writer.finish().context("finalize generated XLSX")?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("sync temporary XLSX for {}", target.display()))?;
    let bytes = temporary.as_file().metadata()?.len();
    if bytes > MAX_ARTIFACT_BYTES {
        return Err(anyhow!("generated XLSX exceeds the 100 MiB safety limit"));
    }
    if target.exists() {
        fs::remove_file(target)
            .with_context(|| format!("replace existing XLSX {}", target.display()))?;
    }
    temporary
        .persist(target)
        .map_err(|error| anyhow!("persist XLSX {}: {}", target.display(), error.error))?;
    Ok(bytes)
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
