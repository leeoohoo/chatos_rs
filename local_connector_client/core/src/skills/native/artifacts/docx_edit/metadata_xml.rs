// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, Result};
use serde_json::Value;

use super::super::format_helpers::{escape_xml, unescape_xml};
use super::{
    append_package_child, empty_xml_start_tag, find_next_xml_tag_start, single_attribute_value,
    validate_xml_text, xml_element_ranges, MAX_XML_BYTES,
};

#[derive(Clone, Copy)]
struct SimpleXmlTextElementRange {
    start: usize,
    end: usize,
    text_range: Option<(usize, usize)>,
}

pub(super) fn docx_metadata_request(
    arguments: &Value,
) -> Result<(BTreeMap<&'static str, String>, BTreeSet<&'static str>)> {
    let mut updates = BTreeMap::new();
    for (field, maximum) in [
        ("title", 1_000usize),
        ("author", 256usize),
        ("subject", 1_000usize),
        ("keywords", 1_000usize),
    ] {
        if let Some(value) = arguments.get(field) {
            let value = value
                .as_str()
                .ok_or_else(|| anyhow!("{field} must be a string when provided"))?;
            if value.chars().count() > maximum {
                return Err(anyhow!(
                    "{field} exceeds the {maximum} character safety limit"
                ));
            }
            validate_xml_text(value, field)?;
            updates.insert(field, value.to_string());
        }
    }
    let mut removals = BTreeSet::new();
    if let Some(value) = arguments.get("remove_fields") {
        let fields = value
            .as_array()
            .filter(|values| values.len() <= 4)
            .ok_or_else(|| anyhow!("remove_fields must be an array of at most 4 field names"))?;
        for value in fields {
            let field = value
                .as_str()
                .and_then(docx_metadata_field)
                .ok_or_else(|| {
                    anyhow!("remove_fields entries must be title, author, subject, or keywords")
                })?;
            if !removals.insert(field) {
                return Err(anyhow!("remove_fields must not contain duplicates"));
            }
        }
    }
    Ok((updates, removals))
}

fn docx_metadata_field(value: &str) -> Option<&'static str> {
    match value {
        "title" => Some("title"),
        "author" => Some("author"),
        "subject" => Some("subject"),
        "keywords" => Some("keywords"),
        _ => None,
    }
}

pub(super) fn docx_metadata_xml_tag(field: &str) -> Result<&'static str> {
    match field {
        "title" => Ok("dc:title"),
        "author" => Ok("dc:creator"),
        "subject" => Ok("dc:subject"),
        "keywords" => Ok("cp:keywords"),
        _ => Err(anyhow!("unsupported DOCX metadata field: {field}")),
    }
}

pub(super) fn empty_docx_core_properties() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"></cp:coreProperties>"#.to_string()
}

pub(super) fn strict_content_types_for_part(xml: &str, part_name: &str) -> Result<Vec<String>> {
    if xml.contains("<!--") || xml.contains("<![CDATA[") || xml.contains("<!DOCTYPE") {
        return Err(anyhow!(
            "DOCX content types do not support comments, CDATA, or DTD markup"
        ));
    }
    let roots = xml_element_ranges(xml, "<Types", "</Types>", 1, "DOCX content types")?;
    if roots.len() != 1 {
        return Err(anyhow!(
            "DOCX content types must contain exactly one Types root"
        ));
    }
    let root = &xml[roots[0].start..roots[0].end];
    let mut content_types = Vec::new();
    let mut cursor = 0usize;
    while let Some(start) = find_next_xml_tag_start(root, "<Override", cursor) {
        let end = root[start..]
            .find('>')
            .map(|offset| start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX content type override is unterminated"))?;
        let entry = &root[start..end];
        if !empty_xml_start_tag(entry) {
            return Err(anyhow!(
                "DOCX content type overrides must be empty XML elements"
            ));
        }
        let entry_part_name = single_attribute_value(entry, "PartName", "content type override")?;
        let content_type = single_attribute_value(entry, "ContentType", "content type override")?;
        if entry_part_name == part_name {
            content_types.push(content_type);
        }
        cursor = end;
    }
    Ok(content_types)
}

pub(super) fn validate_docx_core_properties_xml(xml: &str) -> Result<()> {
    if xml.len() > MAX_XML_BYTES {
        return Err(anyhow!(
            "DOCX core properties XML exceeds the local size limit"
        ));
    }
    if xml.contains("<!--") || xml.contains("<![CDATA[") || xml.contains("<!DOCTYPE") {
        return Err(anyhow!(
            "DOCX core properties do not support comments, CDATA, or DTD markup"
        ));
    }
    let roots = xml_element_ranges(
        xml,
        "<cp:coreProperties",
        "</cp:coreProperties>",
        1,
        "DOCX core properties",
    )?;
    if roots.len() != 1 {
        return Err(anyhow!(
            "DOCX core properties must contain exactly one cp:coreProperties root"
        ));
    }
    let opening = &xml[roots[0].start..roots[0].open_end];
    for namespace in [
        "xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\"",
        "xmlns:dc=\"http://purl.org/dc/elements/1.1/\"",
    ] {
        if !opening.contains(namespace) {
            return Err(anyhow!(
                "DOCX core properties are missing a required standard namespace"
            ));
        }
    }
    for tag in ["dc:title", "dc:creator", "dc:subject", "cp:keywords"] {
        if let Some(range) = docx_core_property_range(xml, tag)? {
            if range.start < roots[0].open_end || range.end > roots[0].close_start {
                return Err(anyhow!(
                    "DOCX managed core property {tag} is outside cp:coreProperties"
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn docx_core_property_value(xml: &str, tag: &str) -> Result<Option<String>> {
    Ok(docx_core_property_range(xml, tag)?.map(|range| {
        range
            .text_range
            .map(|(start, end)| unescape_xml(&xml[start..end]))
            .unwrap_or_default()
    }))
}

fn docx_core_property_range(xml: &str, tag: &str) -> Result<Option<SimpleXmlTextElementRange>> {
    let Some(start) = find_next_xml_tag_start(xml, format!("<{tag}").as_str(), 0) else {
        if xml.contains(format!("</{tag}>").as_str()) {
            return Err(anyhow!(
                "DOCX core property {tag} has an unmatched closing tag"
            ));
        }
        return Ok(None);
    };
    let open_end = xml[start..]
        .find('>')
        .map(|offset| start + offset + 1)
        .ok_or_else(|| anyhow!("DOCX core property {tag} has an unterminated opening tag"))?;
    let opening = &xml[start..open_end];
    let expected_prefix = format!("<{tag}");
    let attributes = opening
        .strip_prefix(expected_prefix.as_str())
        .and_then(|value| value.strip_suffix('>'))
        .ok_or_else(|| anyhow!("DOCX core property {tag} has an invalid opening tag"))?;
    let self_closing = attributes.trim_end().ends_with('/');
    let attributes = if self_closing {
        attributes.trim_end().trim_end_matches('/').trim()
    } else {
        attributes.trim()
    };
    if !attributes.is_empty() {
        return Err(anyhow!(
            "DOCX managed core property {tag} must not contain attributes"
        ));
    }
    let (end, text_range) = if self_closing {
        (open_end, None)
    } else {
        let closing = format!("</{tag}>");
        let close_start = xml[open_end..]
            .find(closing.as_str())
            .map(|offset| open_end + offset)
            .ok_or_else(|| anyhow!("DOCX core property {tag} has no closing tag"))?;
        let raw = &xml[open_end..close_start];
        if raw.contains('<') {
            return Err(anyhow!(
                "DOCX managed core property {tag} contains unsupported nested XML"
            ));
        }
        (close_start + closing.len(), Some((open_end, close_start)))
    };
    if find_next_xml_tag_start(xml, format!("<{tag}").as_str(), end).is_some() {
        return Err(anyhow!("DOCX core property {tag} appears more than once"));
    }
    let closing_count = xml.matches(format!("</{tag}>").as_str()).count();
    if closing_count != usize::from(!self_closing) {
        return Err(anyhow!(
            "DOCX core property {tag} has mismatched element boundaries"
        ));
    }
    Ok(Some(SimpleXmlTextElementRange {
        start,
        end,
        text_range,
    }))
}

pub(super) fn set_docx_core_property(xml: &str, tag: &str, value: Option<&str>) -> Result<String> {
    validate_docx_core_properties_xml(xml)?;
    let existing = docx_core_property_range(xml, tag)?;
    match (existing, value) {
        (Some(range), Some(value)) => {
            let replacement = format!("<{tag}>{}</{tag}>", escape_xml(value));
            let mut output = String::with_capacity(
                xml.len()
                    .saturating_sub(range.end - range.start)
                    .saturating_add(replacement.len()),
            );
            output.push_str(&xml[..range.start]);
            output.push_str(replacement.as_str());
            output.push_str(&xml[range.end..]);
            Ok(output)
        }
        (Some(range), None) => {
            let mut output =
                String::with_capacity(xml.len().saturating_sub(range.end - range.start));
            output.push_str(&xml[..range.start]);
            output.push_str(&xml[range.end..]);
            Ok(output)
        }
        (None, Some(value)) => append_package_child(
            xml,
            "cp:coreProperties",
            format!("<{tag}>{}</{tag}>", escape_xml(value)).as_str(),
        ),
        (None, None) => Ok(xml.to_string()),
    }
}
