// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use zip::ZipArchive;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::{input_file, optional_bool, read_zip_text, required_text, MAX_ARTIFACT_BYTES};
use super::package_write::{docx_output_path, rewrite_docx_package};
use super::{
    append_package_child, document_relationships, docx_core_property_value, docx_metadata_request,
    docx_metadata_xml_tag, empty_docx_core_properties, ensure_content_type_override,
    next_relationship_id, set_docx_core_property, strict_content_types_for_part,
    validate_docx_core_properties_xml, xml_element_ranges, MAX_DOCX_ZIP_ENTRIES,
};

const DOCX_CORE_PROPERTIES_PART: &str = "docProps/core.xml";
const DOCX_CORE_PROPERTIES_RELATIONSHIP_TYPE: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
const DOCX_CORE_PROPERTIES_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-package.core-properties+xml";

struct DocxMetadataPackage {
    names: HashSet<String>,
    root_relationships_xml: String,
    content_types_xml: String,
    core_properties_xml: Option<String>,
}

pub(super) fn inspect_docx_metadata(core_properties_xml: Option<&str>) -> Result<Value> {
    let Some(xml) = core_properties_xml else {
        return Ok(json!({
            "present": false,
            "title": Value::Null,
            "author": Value::Null,
            "subject": Value::Null,
            "keywords": Value::Null,
        }));
    };
    validate_docx_core_properties_xml(xml)?;
    Ok(json!({
        "present": true,
        "title": docx_core_property_value(xml, "dc:title")?,
        "author": docx_core_property_value(xml, "dc:creator")?,
        "subject": docx_core_property_value(xml, "dc:subject")?,
        "keywords": docx_core_property_value(xml, "cp:keywords")?,
    }))
}

pub(super) fn update_docx_metadata(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".docx")?;
    let (updates, removals) = docx_metadata_request(arguments)?;
    if updates.is_empty() && removals.is_empty() {
        return Err(anyhow!(
            "DOCX metadata update requires at least one field value or remove_fields entry"
        ));
    }
    if updates.keys().any(|field| removals.contains(field)) {
        return Err(anyhow!(
            "DOCX metadata fields cannot be set and removed in the same request"
        ));
    }
    let target_requested = required_text(arguments, "target_path")?;
    let (target, target_relative) = docx_output_path(
        state,
        request,
        target_requested,
        std::slice::from_ref(&source),
    )?;

    let package = read_docx_metadata_package(source.as_path())?;
    let mut root_relationships_xml = package.root_relationships_xml.clone();
    let mut content_types_xml = package.content_types_xml.clone();
    validate_docx_metadata_package(&package)?;
    let metadata_part_created = package.core_properties_xml.is_none();
    let mut core_properties_xml = package
        .core_properties_xml
        .clone()
        .unwrap_or_else(empty_docx_core_properties);
    let original_core_properties_xml = core_properties_xml.clone();

    for field in &removals {
        core_properties_xml = set_docx_core_property(
            core_properties_xml.as_str(),
            docx_metadata_xml_tag(field)?,
            None,
        )?;
    }
    for (field, value) in &updates {
        core_properties_xml = set_docx_core_property(
            core_properties_xml.as_str(),
            docx_metadata_xml_tag(field)?,
            Some(value.as_str()),
        )?;
    }
    if core_properties_xml == original_core_properties_xml {
        return Err(anyhow!(
            "DOCX metadata update would not change any requested field"
        ));
    }

    let mut replacements = BTreeMap::new();
    let mut additions = Vec::new();
    if metadata_part_created {
        let relationship_id = next_relationship_id(root_relationships_xml.as_str())?;
        root_relationships_xml = append_package_child(
            root_relationships_xml.as_str(),
            "Relationships",
            format!(
                "<Relationship Id=\"{relationship_id}\" Type=\"{DOCX_CORE_PROPERTIES_RELATIONSHIP_TYPE}\" Target=\"{DOCX_CORE_PROPERTIES_PART}\"/>"
            )
            .as_str(),
        )?;
        content_types_xml = ensure_content_type_override(
            content_types_xml.as_str(),
            "/docProps/core.xml",
            DOCX_CORE_PROPERTIES_CONTENT_TYPE,
        )?;
        replacements.insert(
            "_rels/.rels".to_string(),
            root_relationships_xml.into_bytes(),
        );
        replacements.insert(
            "[Content_Types].xml".to_string(),
            content_types_xml.into_bytes(),
        );
        additions.push((
            DOCX_CORE_PROPERTIES_PART.to_string(),
            core_properties_xml.as_bytes().to_vec(),
        ));
    } else {
        replacements.insert(
            DOCX_CORE_PROPERTIES_PART.to_string(),
            core_properties_xml.as_bytes().to_vec(),
        );
    }
    let metadata = inspect_docx_metadata(Some(core_properties_xml.as_str()))?;
    let bytes = rewrite_docx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        additions,
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "operation": "update_metadata",
        "source_path": source_relative,
        "path": target_relative,
        "metadata_part_created": metadata_part_created,
        "updated_fields": updates.keys().copied().collect::<Vec<_>>(),
        "removed_fields": removals.iter().copied().collect::<Vec<_>>(),
        "metadata": metadata,
        "bytes": bytes,
    }))
}

fn read_docx_metadata_package(source: &Path) -> Result<DocxMetadataPackage> {
    let mut archive = ZipArchive::new(File::open(source)?)
        .with_context(|| format!("open DOCX {}", source.display()))?;
    if archive.is_empty() || archive.len() > MAX_DOCX_ZIP_ENTRIES {
        return Err(anyhow!(
            "DOCX ZIP must contain between 1 and {MAX_DOCX_ZIP_ENTRIES} entries"
        ));
    }
    let mut names = HashSet::new();
    let mut total_uncompressed = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if entry.enclosed_name().is_none() || entry.is_symlink() || !names.insert(name) {
            return Err(anyhow!("DOCX ZIP contains an unsafe or duplicate entry"));
        }
        total_uncompressed = total_uncompressed.saturating_add(entry.size());
        if total_uncompressed > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "DOCX ZIP exceeds the 100 MiB expanded safety limit"
            ));
        }
    }
    if !names.contains("word/document.xml") {
        return Err(anyhow!("DOCX ZIP is missing word/document.xml"));
    }
    let root_relationships_xml = read_zip_text(&mut archive, "_rels/.rels")?;
    let content_types_xml = read_zip_text(&mut archive, "[Content_Types].xml")?;
    let core_properties_xml = if names.contains(DOCX_CORE_PROPERTIES_PART) {
        Some(read_zip_text(&mut archive, DOCX_CORE_PROPERTIES_PART)?)
    } else {
        None
    };
    Ok(DocxMetadataPackage {
        names,
        root_relationships_xml,
        content_types_xml,
        core_properties_xml,
    })
}

fn validate_docx_metadata_package(package: &DocxMetadataPackage) -> Result<()> {
    if package.root_relationships_xml.contains("<!--")
        || package.root_relationships_xml.contains("<![CDATA[")
        || package.root_relationships_xml.contains("<!DOCTYPE")
    {
        return Err(anyhow!(
            "DOCX root relationships do not support comments, CDATA, or DTD markup"
        ));
    }
    let relationship_roots = xml_element_ranges(
        package.root_relationships_xml.as_str(),
        "<Relationships",
        "</Relationships>",
        1,
        "DOCX root relationships",
    )?;
    if relationship_roots.len() != 1 {
        return Err(anyhow!(
            "DOCX root relationships must contain exactly one Relationships root"
        ));
    }
    let root = relationship_roots[0];
    let relationships =
        document_relationships(&package.root_relationships_xml[root.start..root.end])?;
    let office_relationships = relationships
        .iter()
        .filter(|relationship| {
            relationship.relationship_type
                == "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"
        })
        .collect::<Vec<_>>();
    if office_relationships.len() != 1
        || office_relationships[0].external
        || !matches!(
            office_relationships[0].target.as_str(),
            "word/document.xml" | "./word/document.xml"
        )
    {
        return Err(anyhow!(
            "DOCX requires exactly one internal root officeDocument relationship to word/document.xml"
        ));
    }
    let core_relationships = relationships
        .iter()
        .filter(|relationship| {
            relationship.relationship_type == DOCX_CORE_PROPERTIES_RELATIONSHIP_TYPE
        })
        .collect::<Vec<_>>();
    let content_types =
        strict_content_types_for_part(package.content_types_xml.as_str(), "/docProps/core.xml")?;
    if package.names.contains(DOCX_CORE_PROPERTIES_PART) {
        if core_relationships.len() != 1
            || core_relationships[0].external
            || !matches!(
                core_relationships[0].target.as_str(),
                "docProps/core.xml" | "./docProps/core.xml"
            )
        {
            return Err(anyhow!(
                "existing DOCX core properties require exactly one standard internal relationship"
            ));
        }
        if content_types.as_slice() != [DOCX_CORE_PROPERTIES_CONTENT_TYPE] {
            return Err(anyhow!(
                "existing DOCX core properties require exactly one standard content type override"
            ));
        }
        validate_docx_core_properties_xml(
            package
                .core_properties_xml
                .as_deref()
                .ok_or_else(|| anyhow!("DOCX core properties part could not be read"))?,
        )?;
    } else if !core_relationships.is_empty() || !content_types.is_empty() {
        return Err(anyhow!(
            "DOCX contains partial core-properties relationship or content-type metadata without docProps/core.xml"
        ));
    }
    Ok(())
}
