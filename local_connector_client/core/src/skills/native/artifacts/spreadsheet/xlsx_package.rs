// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::{Component, Path};

use anyhow::{anyhow, Context, Result};
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use zip::ZipArchive;

use super::super::{read_zip_text, MAX_ARTIFACT_BYTES};
use super::xlsx_input::validate_sheet_name;
use super::{MAX_XLSX_SHEETS, MAX_XLSX_ZIP_ENTRIES};

#[derive(Clone, Debug)]
pub(super) struct SheetPart {
    pub(super) name: String,
    pub(super) path: String,
}

pub(super) fn validate_xlsx_package(path: &Path) -> Result<HashSet<String>> {
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

pub(super) fn read_workbook_parts(path: &Path) -> Result<(String, String)> {
    let mut archive = ZipArchive::new(File::open(path)?)
        .with_context(|| format!("open XLSX {}", path.display()))?;
    Ok((
        read_zip_text(&mut archive, "xl/workbook.xml")?,
        read_zip_text(&mut archive, "xl/_rels/workbook.xml.rels")?,
    ))
}

pub(super) fn workbook_sheet_parts(
    workbook_xml: &str,
    relationships_xml: &str,
) -> Result<Vec<SheetPart>> {
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

pub(super) type RelationshipMap = HashMap<String, (String, String, bool)>;

pub(super) fn parse_relationships(xml: &str) -> Result<RelationshipMap> {
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

pub(super) fn workbook_styles_part(relationships_xml: &str) -> Result<Option<String>> {
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

pub(super) fn required_attribute(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    name: &str,
) -> Result<String> {
    optional_attribute(reader, event, name)?
        .ok_or_else(|| anyhow!("XLSX XML element is missing required {name} attribute"))
}

pub(super) fn optional_attribute(
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
