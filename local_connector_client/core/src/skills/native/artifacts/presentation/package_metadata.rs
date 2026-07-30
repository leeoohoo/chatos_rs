// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Context, Result};
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use super::package_paths::sibling_qualified_name;
use super::resolve_part_target;

#[derive(Debug)]
pub(super) struct PresentationSlideMetadata {
    pub(super) slide_ids: Vec<u32>,
    pub(super) relationship_ids: Vec<String>,
    pub(super) max_slide_id: u32,
    pub(super) slide_tag_name: String,
    pub(super) relationship_attribute_name: String,
}

#[derive(Debug)]
pub(super) struct PackageRelationship {
    pub(super) id: String,
    pub(super) relationship_type: String,
    pub(super) target: String,
    pub(super) external: bool,
}

#[derive(Debug)]
pub(super) struct RelationshipDocument {
    pub(super) relationships: Vec<PackageRelationship>,
    pub(super) relationship_tag_name: String,
}

#[derive(Debug)]
pub(super) struct RelationshipAddition {
    pub(super) id: String,
    pub(super) relationship_type: &'static str,
    pub(super) target: String,
}

#[derive(Debug)]
pub(super) struct ContentTypesMetadata {
    pub(super) defaults: HashMap<String, String>,
    pub(super) overrides: HashSet<String>,
    pub(super) default_tag_name: String,
    pub(super) override_tag_name: String,
}

pub(super) fn presentation_slide_metadata(xml: &str) -> Result<PresentationSlideMetadata> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut slide_ids = Vec::new();
    let mut used_slide_ids = HashSet::new();
    let mut relationship_ids = Vec::new();
    let mut used_relationship_ids = HashSet::new();
    let mut max_slide_id = 0u32;
    let mut slide_tag_name = None;
    let mut relationship_attribute_name = None;
    let mut slide_list_count = 0usize;
    loop {
        match reader
            .read_event()
            .context("parse PPTX presentation slide list")?
        {
            Event::Start(event) if event.local_name().as_ref() == b"sldIdLst" => {
                slide_list_count = slide_list_count.saturating_add(1);
            }
            Event::Empty(event) if event.local_name().as_ref() == b"sldIdLst" => {
                return Err(anyhow!("PPTX presentation slide list is empty"));
            }
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"sldId" =>
            {
                let mut numeric_id = None;
                let mut relationship_id = None;
                let mut relationship_key = None;
                for attribute in event.attributes().with_checks(false) {
                    let attribute = attribute.context("parse PPTX slide id attribute")?;
                    let key = attribute.key.as_ref();
                    let value = attribute
                        .decoded_and_normalized_value(XmlVersion::Explicit1_0, reader.decoder())?
                        .into_owned();
                    if key == b"id" {
                        numeric_id = Some(value.parse::<u32>().context("parse PPTX slide id")?);
                    } else if key.ends_with(b":id") {
                        relationship_id = Some(value);
                        relationship_key = Some(String::from_utf8_lossy(key).into_owned());
                    }
                }
                let numeric_id = numeric_id
                    .ok_or_else(|| anyhow!("PPTX slide id is missing numeric id attribute"))?;
                let relationship_id = relationship_id
                    .ok_or_else(|| anyhow!("PPTX slide id is missing relationship id attribute"))?;
                if !used_slide_ids.insert(numeric_id) {
                    return Err(anyhow!(
                        "PPTX presentation contains duplicate numeric slide ids"
                    ));
                }
                if !used_relationship_ids.insert(relationship_id.clone()) {
                    return Err(anyhow!(
                        "PPTX presentation contains duplicate slide relationship ids"
                    ));
                }
                max_slide_id = max_slide_id.max(numeric_id);
                slide_ids.push(numeric_id);
                relationship_ids.push(relationship_id);
                let current_tag = String::from_utf8_lossy(event.name().as_ref()).into_owned();
                if slide_tag_name
                    .as_ref()
                    .is_some_and(|existing| existing != &current_tag)
                {
                    return Err(anyhow!("PPTX presentation mixes slide id namespaces"));
                }
                slide_tag_name = Some(current_tag);
                let relationship_key = relationship_key
                    .ok_or_else(|| anyhow!("PPTX slide id is missing relationship id attribute"))?;
                if relationship_attribute_name
                    .as_ref()
                    .is_some_and(|existing| existing != &relationship_key)
                {
                    return Err(anyhow!(
                        "PPTX presentation mixes relationship attribute namespaces"
                    ));
                }
                relationship_attribute_name = Some(relationship_key);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if slide_list_count != 1 || relationship_ids.is_empty() {
        return Err(anyhow!(
            "PPTX presentation must contain exactly one non-empty slide list"
        ));
    }
    Ok(PresentationSlideMetadata {
        slide_ids,
        relationship_ids,
        max_slide_id,
        slide_tag_name: slide_tag_name
            .ok_or_else(|| anyhow!("PPTX presentation slide list is missing a tag name"))?,
        relationship_attribute_name: relationship_attribute_name.ok_or_else(|| {
            anyhow!("PPTX presentation slide list is missing a relationship attribute")
        })?,
    })
}

pub(super) fn parse_relationship_document(
    xml: &str,
    source_part: &str,
) -> Result<RelationshipDocument> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut relationships = Vec::new();
    let mut ids = HashSet::new();
    let mut relationship_tag_name = None;
    let mut root_count = 0usize;
    loop {
        match reader
            .read_event()
            .with_context(|| format!("parse PPTX relationships for {source_part}"))?
        {
            Event::Start(event) if event.local_name().as_ref() == b"Relationships" => {
                root_count = root_count.saturating_add(1);
            }
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"Relationship" =>
            {
                let id = required_xml_attribute(&reader, &event, "Id")?;
                if !ids.insert(id.clone()) {
                    return Err(anyhow!(
                        "PPTX relationship document contains duplicate Id: {id}"
                    ));
                }
                let relationship_type = required_xml_attribute(&reader, &event, "Type")?;
                let target = required_xml_attribute(&reader, &event, "Target")?;
                let external = optional_xml_attribute(&reader, &event, "TargetMode")?
                    .is_some_and(|value| value.eq_ignore_ascii_case("external"));
                if !external {
                    resolve_part_target(source_part, target.as_str())?;
                }
                let current_tag = String::from_utf8_lossy(event.name().as_ref()).into_owned();
                if relationship_tag_name
                    .as_ref()
                    .is_some_and(|existing| existing != &current_tag)
                {
                    return Err(anyhow!("PPTX relationship document mixes namespaces"));
                }
                relationship_tag_name = Some(current_tag);
                relationships.push(PackageRelationship {
                    id,
                    relationship_type,
                    target,
                    external,
                });
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if root_count != 1 {
        return Err(anyhow!(
            "PPTX relationship document must contain exactly one Relationships root"
        ));
    }
    Ok(RelationshipDocument {
        relationships,
        relationship_tag_name: relationship_tag_name.unwrap_or_else(|| "Relationship".to_string()),
    })
}

pub(super) fn content_types_metadata(xml: &str) -> Result<ContentTypesMetadata> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut defaults = HashMap::new();
    let mut overrides = HashSet::new();
    let mut default_tag_name = None;
    let mut override_tag_name = None;
    let mut root_count = 0usize;
    loop {
        match reader.read_event().context("parse PPTX content types")? {
            Event::Start(event) if event.local_name().as_ref() == b"Types" => {
                root_count = root_count.saturating_add(1);
            }
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"Default" =>
            {
                let extension =
                    required_xml_attribute(&reader, &event, "Extension")?.to_ascii_lowercase();
                let content_type = required_xml_attribute(&reader, &event, "ContentType")?;
                if defaults.insert(extension, content_type).is_some() {
                    return Err(anyhow!(
                        "PPTX content types contain a duplicate Default extension"
                    ));
                }
                default_tag_name =
                    Some(String::from_utf8_lossy(event.name().as_ref()).into_owned());
            }
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"Override" =>
            {
                let part_name = required_xml_attribute(&reader, &event, "PartName")?;
                if !part_name.starts_with('/') || !overrides.insert(part_name) {
                    return Err(anyhow!(
                        "PPTX content types contain an invalid or duplicate Override"
                    ));
                }
                override_tag_name =
                    Some(String::from_utf8_lossy(event.name().as_ref()).into_owned());
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if root_count != 1 {
        return Err(anyhow!(
            "PPTX content types must contain exactly one Types root"
        ));
    }
    let default_tag_name = default_tag_name.unwrap_or_else(|| "Default".to_string());
    let override_tag_name = override_tag_name
        .unwrap_or_else(|| sibling_qualified_name(default_tag_name.as_str(), "Override"));
    Ok(ContentTypesMetadata {
        defaults,
        overrides,
        default_tag_name,
        override_tag_name,
    })
}

pub(super) fn required_xml_attribute(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    name: &str,
) -> Result<String> {
    optional_xml_attribute(reader, event, name)?
        .ok_or_else(|| anyhow!("PPTX XML element is missing required {name} attribute"))
}

pub(super) fn optional_xml_attribute(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    name: &str,
) -> Result<Option<String>> {
    for attribute in event.attributes().with_checks(false) {
        let attribute = attribute.context("parse PPTX XML attribute")?;
        let local = attribute.key.as_ref().rsplit(|byte| *byte == b':').next();
        if attribute.key.as_ref() == name.as_bytes() || local == Some(name.as_bytes()) {
            return Ok(Some(
                attribute
                    .decoded_and_normalized_value(XmlVersion::Explicit1_0, reader.decoder())?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}
