// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::path::Path;

use anyhow::{anyhow, Result};

use super::{
    find_next_xml_tag_start, unescape_xml, DocumentRelationship, MAX_DOCX_ZIP_ENTRIES,
    MAX_XML_BYTES,
};

pub(super) fn next_package_part_name(
    names: &HashSet<String>,
    prefix: &str,
    suffix: &str,
) -> Result<String> {
    for index in 1..=MAX_DOCX_ZIP_ENTRIES {
        let candidate = format!("{prefix}{index}{suffix}");
        if !names.contains(candidate.as_str()) {
            return Ok(candidate);
        }
    }
    Err(anyhow!("DOCX has no available bounded package part name"))
}

pub(super) fn next_relationship_id(relationships_xml: &str) -> Result<String> {
    let existing = quoted_attribute_values(relationships_xml, "Id")
        .into_iter()
        .collect::<HashSet<_>>();
    for index in 1..=(MAX_DOCX_ZIP_ENTRIES * 2) {
        let candidate = format!("rId{index}");
        if !existing.contains(candidate.as_str()) {
            return Ok(candidate);
        }
    }
    Err(anyhow!("DOCX has no available bounded relationship ID"))
}

pub(super) fn next_drawing_property_id(document_xml: &str) -> Result<u32> {
    let highest = quoted_attribute_values(document_xml, "id")
        .into_iter()
        .filter_map(|value| value.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    highest
        .checked_add(1)
        .ok_or_else(|| anyhow!("DOCX drawing property IDs are exhausted"))
}

pub(super) fn quoted_attribute_values(xml: &str, attribute: &str) -> Vec<String> {
    let needle = format!(" {attribute}=");
    let mut values = Vec::new();
    let mut cursor = 0usize;
    while let Some(offset) = xml[cursor..].find(needle.as_str()) {
        let quote_index = cursor + offset + needle.len();
        let Some(quote) = xml.as_bytes().get(quote_index).copied() else {
            break;
        };
        if !matches!(quote, b'\'' | b'"') {
            cursor = quote_index;
            continue;
        }
        let value_start = quote_index + 1;
        let Some(end) = xml.as_bytes()[value_start..]
            .iter()
            .position(|byte| *byte == quote)
        else {
            break;
        };
        values.push(unescape_xml(&xml[value_start..value_start + end]));
        cursor = value_start + end + 1;
    }
    values
}

pub(super) fn append_package_child(xml: &str, root_name: &str, child: &str) -> Result<String> {
    let closing = format!("</{root_name}>");
    if let Some(index) = xml.rfind(closing.as_str()) {
        let mut output = String::with_capacity(xml.len().saturating_add(child.len()));
        output.push_str(&xml[..index]);
        output.push_str(child);
        output.push_str(&xml[index..]);
        if output.len() > MAX_XML_BYTES {
            return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
        }
        return Ok(output);
    }

    let opening = format!("<{root_name}");
    let opening_start = xml
        .find(opening.as_str())
        .ok_or_else(|| anyhow!("DOCX XML is missing {root_name} root"))?;
    let opening_end = xml[opening_start..]
        .find('>')
        .map(|offset| opening_start + offset)
        .ok_or_else(|| anyhow!("DOCX XML has an unterminated {root_name} root"))?;
    let slash = xml[opening_start..opening_end]
        .rfind('/')
        .map(|offset| opening_start + offset)
        .filter(|index| xml[index + 1..opening_end].trim().is_empty())
        .ok_or_else(|| anyhow!("DOCX XML {root_name} root is not closed"))?;
    let mut output = String::with_capacity(
        xml.len()
            .saturating_add(child.len())
            .saturating_add(closing.len()),
    );
    output.push_str(&xml[..slash]);
    output.push('>');
    output.push_str(child);
    output.push_str(closing.as_str());
    output.push_str(&xml[opening_end + 1..]);
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok(output)
}

pub(super) fn ensure_content_type_default(
    xml: &str,
    extension: &str,
    content_type: &str,
) -> Result<String> {
    let has_extension = xml.contains(format!("Extension=\"{extension}\"").as_str())
        || xml.contains(format!("Extension='{extension}'").as_str());
    if has_extension {
        return Ok(xml.to_string());
    }
    append_package_child(
        xml,
        "Types",
        format!("<Default Extension=\"{extension}\" ContentType=\"{content_type}\"/>").as_str(),
    )
}

pub(super) fn ensure_content_type_override(
    xml: &str,
    part_name: &str,
    content_type: &str,
) -> Result<String> {
    let has_part = xml.contains(format!("PartName=\"{part_name}\"").as_str())
        || xml.contains(format!("PartName='{part_name}'").as_str());
    if has_part {
        return Err(anyhow!("DOCX content types already contain {part_name}"));
    }
    append_package_child(
        xml,
        "Types",
        format!("<Override PartName=\"{part_name}\" ContentType=\"{content_type}\"/>").as_str(),
    )
}

pub(super) fn content_types_for_part(xml: &str, part_name: &str) -> Result<Vec<String>> {
    let mut content_types = Vec::new();
    let mut cursor = 0usize;
    while let Some(start) = find_next_xml_tag_start(xml, "<Override", cursor) {
        let end = xml[start..]
            .find('>')
            .map(|offset| start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX content type override is unterminated"))?;
        let entry = &xml[start..end];
        if quoted_attribute_values(entry, "PartName")
            .first()
            .is_some_and(|value| value == part_name)
        {
            let content_type = quoted_attribute_values(entry, "ContentType")
                .into_iter()
                .next()
                .ok_or_else(|| anyhow!("DOCX content type override is missing ContentType"))?;
            content_types.push(content_type);
        }
        cursor = end;
    }
    Ok(content_types)
}

pub(super) fn relationship_targets_for_type(
    xml: &str,
    relationship_type: &str,
) -> Result<Vec<String>> {
    let mut targets = Vec::new();
    let mut cursor = 0usize;
    while let Some(start) = find_next_xml_tag_start(xml, "<Relationship", cursor) {
        let end = xml[start..]
            .find('>')
            .map(|offset| start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX relationship entry is unterminated"))?;
        let entry = &xml[start..end];
        let types = quoted_attribute_values(entry, "Type");
        if types
            .first()
            .is_some_and(|value| value == relationship_type)
        {
            let target = quoted_attribute_values(entry, "Target")
                .into_iter()
                .next()
                .ok_or_else(|| anyhow!("DOCX comments relationship is missing Target"))?;
            targets.push(target.trim_start_matches("./").to_string());
        }
        cursor = end;
    }
    Ok(targets)
}

pub(super) fn document_relationships(xml: &str) -> Result<Vec<DocumentRelationship>> {
    let mut relationships = Vec::new();
    let mut ids = HashSet::new();
    let mut cursor = 0usize;
    while let Some(start) = find_next_xml_tag_start(xml, "<Relationship", cursor) {
        let end = xml[start..]
            .find('>')
            .map(|offset| start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX relationship entry is unterminated"))?;
        let entry = &xml[start..end];
        if !empty_xml_start_tag(entry) {
            return Err(anyhow!(
                "DOCX relationship entries must be empty XML elements"
            ));
        }
        let id = single_attribute_value(entry, "Id", "relationship")?;
        if !ids.insert(id.clone()) {
            return Err(anyhow!("DOCX contains a duplicate relationship ID: {id}"));
        }
        let relationship_type = single_attribute_value(entry, "Type", "relationship")?;
        let target = single_attribute_value(entry, "Target", "relationship")?;
        let target_modes = quoted_attribute_values(entry, "TargetMode");
        let external = match target_modes.as_slice() {
            [] => false,
            [mode] if mode == "External" => true,
            [_] => {
                return Err(anyhow!(
                    "DOCX relationship TargetMode must be External when present"
                ));
            }
            _ => {
                return Err(anyhow!(
                    "DOCX relationship contains duplicate TargetMode values"
                ))
            }
        };
        relationships.push(DocumentRelationship {
            id,
            relationship_type,
            target,
            external,
        });
        if relationships.len() > MAX_DOCX_ZIP_ENTRIES * 2 {
            return Err(anyhow!("DOCX relationship count exceeds the safety limit"));
        }
        cursor = end;
    }
    Ok(relationships)
}

pub(super) fn single_attribute_value(entry: &str, attribute: &str, label: &str) -> Result<String> {
    let values = quoted_attribute_values(entry, attribute);
    match values.as_slice() {
        [value] if !value.is_empty() => Ok(value.clone()),
        [_] => Err(anyhow!("DOCX {label} {attribute} must not be empty")),
        [] => Err(anyhow!("DOCX {label} is missing {attribute}")),
        _ => Err(anyhow!(
            "DOCX {label} contains duplicate {attribute} values"
        )),
    }
}

pub(super) fn empty_xml_start_tag(entry: &str) -> bool {
    entry
        .strip_suffix('>')
        .is_some_and(|value| value.trim_end().ends_with('/'))
}

pub(super) fn resolve_document_relationship_target(target: &str) -> Result<String> {
    if target.is_empty()
        || target.starts_with('/')
        || target.contains(['\\', '?', '#'])
        || target.contains(':')
    {
        return Err(anyhow!(
            "DOCX relationship target is not a safe relative part path"
        ));
    }
    let mut components = vec!["word".to_string()];
    for component in Path::new(target).components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(value) => {
                let value = value
                    .to_str()
                    .ok_or_else(|| anyhow!("DOCX relationship target is not valid UTF-8"))?;
                if value.is_empty() {
                    return Err(anyhow!(
                        "DOCX relationship target contains an empty segment"
                    ));
                }
                components.push(value.to_string());
            }
            std::path::Component::ParentDir => {
                if components.len() <= 1 {
                    return Err(anyhow!(
                        "DOCX relationship target escapes the word package root"
                    ));
                }
                components.pop();
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err(anyhow!("DOCX relationship target must be relative"));
            }
        }
    }
    if components.len() <= 1 {
        return Err(anyhow!(
            "DOCX relationship target resolves to the word package root"
        ));
    }
    Ok(components.join("/"))
}
