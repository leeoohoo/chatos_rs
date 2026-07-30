// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Context, Result};
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer};

use super::escape_xml;
use super::package_metadata::{
    required_xml_attribute, ContentTypesMetadata, PresentationSlideMetadata, RelationshipAddition,
};

pub(super) fn appended_slide_relationships(
    layout_target: &str,
    image_target: Option<&str>,
    chart_target: Option<&str>,
    notes_target: Option<&str>,
) -> String {
    let mut relationships = format!(
        "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout\" Target=\"{}\"/>",
        escape_xml(layout_target)
    );
    let mut next_id = 2usize;
    if let Some(image_target) = image_target {
        relationships.push_str(
            format!("<Relationship Id=\"rId{next_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"{}\"/>", escape_xml(image_target)).as_str(),
        );
        next_id += 1;
    }
    if let Some(chart_target) = chart_target {
        relationships.push_str(
            format!("<Relationship Id=\"rId{next_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart\" Target=\"{}\"/>", escape_xml(chart_target)).as_str(),
        );
        next_id += 1;
    }
    if let Some(notes_target) = notes_target {
        relationships.push_str(
            format!("<Relationship Id=\"rId{next_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide\" Target=\"{}\"/>", escape_xml(notes_target)).as_str(),
        );
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{relationships}</Relationships>"#
    )
}

pub(super) fn appended_notes_slide_relationships(
    notes_master_target: &str,
    slide_target: &str,
) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster" Target="{}"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="{}"/></Relationships>"#,
        escape_xml(notes_master_target),
        escape_xml(slide_target)
    )
}

pub(super) fn append_presentation_slide_ids(
    xml: &str,
    metadata: &PresentationSlideMetadata,
    additions: &[(u32, String)],
) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(
        xml.len().saturating_add(additions.len().saturating_mul(64)),
    ));
    let mut inserted = false;
    loop {
        let event = reader
            .read_event()
            .context("rewrite PPTX presentation slide list")?;
        match &event {
            Event::End(end) if end.local_name().as_ref() == b"sldIdLst" => {
                if inserted {
                    return Err(anyhow!("PPTX presentation contains duplicate slide lists"));
                }
                for (slide_id, relationship_id) in additions {
                    let slide_id_text = slide_id.to_string();
                    let mut slide = BytesStart::new(metadata.slide_tag_name.as_str());
                    slide.push_attribute(("id", slide_id_text.as_str()));
                    slide.push_attribute((
                        metadata.relationship_attribute_name.as_str(),
                        relationship_id.as_str(),
                    ));
                    writer.write_event(Event::Empty(slide))?;
                }
                inserted = true;
            }
            Event::Eof => {
                writer.write_event(event.into_owned())?;
                break;
            }
            _ => {}
        }
        writer.write_event(event.into_owned())?;
    }
    if !inserted {
        return Err(anyhow!(
            "PPTX presentation is missing a writable slide list"
        ));
    }
    xml_output(writer, "updated PPTX presentation XML")
}

pub(super) fn rewrite_presentation_slide_ids(
    xml: &str,
    metadata: &PresentationSlideMetadata,
    slide_positions: &[usize],
) -> Result<String> {
    if slide_positions.is_empty() {
        return Err(anyhow!("PPTX slide list must retain at least one slide"));
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(xml.len()));
    let mut in_slide_list = false;
    let mut replaced = false;
    let mut existing_slides = 0usize;
    loop {
        let event = reader
            .read_event()
            .context("reorder PPTX presentation slide list")?;
        match event {
            Event::Start(start) if start.local_name().as_ref() == b"sldIdLst" => {
                if in_slide_list || replaced {
                    return Err(anyhow!("PPTX presentation contains duplicate slide lists"));
                }
                in_slide_list = true;
                writer.write_event(Event::Start(start.into_owned()))?;
            }
            Event::Empty(empty) if empty.local_name().as_ref() == b"sldIdLst" => {
                return Err(anyhow!("PPTX presentation slide list is empty"));
            }
            Event::Empty(slide) if in_slide_list && slide.local_name().as_ref() == b"sldId" => {
                existing_slides = existing_slides.saturating_add(1);
            }
            Event::Start(slide) if in_slide_list && slide.local_name().as_ref() == b"sldId" => {
                return Err(anyhow!(
                    "PPTX slide reordering requires empty slide-id elements without extension content"
                ));
            }
            Event::End(end) if end.local_name().as_ref() == b"sldIdLst" => {
                if !in_slide_list || existing_slides != metadata.relationship_ids.len() {
                    return Err(anyhow!(
                        "PPTX presentation slide list changed during validation"
                    ));
                }
                for position in slide_positions {
                    let index = position - 1;
                    if index >= metadata.relationship_ids.len() {
                        return Err(anyhow!(
                            "PPTX rewritten slide position is outside the visible slide list"
                        ));
                    }
                    let slide_id_text = metadata.slide_ids[index].to_string();
                    let mut slide = BytesStart::new(metadata.slide_tag_name.as_str());
                    slide.push_attribute(("id", slide_id_text.as_str()));
                    slide.push_attribute((
                        metadata.relationship_attribute_name.as_str(),
                        metadata.relationship_ids[index].as_str(),
                    ));
                    writer.write_event(Event::Empty(slide))?;
                }
                writer.write_event(Event::End(end.into_owned()))?;
                in_slide_list = false;
                replaced = true;
            }
            Event::Text(text)
                if in_slide_list && String::from_utf8_lossy(text.as_ref()).trim().is_empty() =>
            {
                writer.write_event(Event::Text(text.into_owned()))?;
            }
            Event::Eof => {
                writer.write_event(Event::Eof)?;
                break;
            }
            event => {
                if in_slide_list {
                    return Err(anyhow!(
                        "PPTX slide list contains unsupported extension or mixed content"
                    ));
                }
                writer.write_event(event.into_owned())?;
            }
        }
    }
    if in_slide_list || !replaced {
        return Err(anyhow!(
            "PPTX presentation is missing a writable slide list"
        ));
    }
    xml_output(writer, "reordered PPTX presentation XML")
}

pub(super) fn append_relationship_entries(
    xml: &str,
    relationship_tag_name: &str,
    additions: &[RelationshipAddition],
) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(
        xml.len()
            .saturating_add(additions.len().saturating_mul(180)),
    ));
    let mut inserted = false;
    loop {
        let event = reader
            .read_event()
            .context("rewrite PPTX relationship document")?;
        match &event {
            Event::End(end) if end.local_name().as_ref() == b"Relationships" => {
                if inserted {
                    return Err(anyhow!("PPTX relationship document has duplicate roots"));
                }
                for addition in additions {
                    let mut relationship = BytesStart::new(relationship_tag_name);
                    relationship.push_attribute(("Id", addition.id.as_str()));
                    relationship.push_attribute(("Type", addition.relationship_type));
                    relationship.push_attribute(("Target", addition.target.as_str()));
                    writer.write_event(Event::Empty(relationship))?;
                }
                inserted = true;
            }
            Event::Eof => {
                writer.write_event(event.into_owned())?;
                break;
            }
            _ => {}
        }
        writer.write_event(event.into_owned())?;
    }
    if !inserted {
        return Err(anyhow!(
            "PPTX relationship document is missing a writable root"
        ));
    }
    xml_output(writer, "updated PPTX relationship XML")
}

pub(super) fn remove_relationship_entries(
    xml: &str,
    removed_ids: &HashSet<String>,
) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(xml.len()));
    let mut removed = HashSet::new();
    loop {
        let event = reader
            .read_event()
            .context("remove PPTX relationship entries")?;
        match event {
            Event::Empty(relationship) if relationship.local_name().as_ref() == b"Relationship" => {
                let id = required_xml_attribute(&reader, &relationship, "Id")?;
                if removed_ids.contains(id.as_str()) {
                    removed.insert(id);
                } else {
                    writer.write_event(Event::Empty(relationship.into_owned()))?;
                }
            }
            Event::Start(relationship) if relationship.local_name().as_ref() == b"Relationship" => {
                return Err(anyhow!(
                    "PPTX relationship removal requires empty Relationship elements"
                ));
            }
            Event::Eof => {
                writer.write_event(Event::Eof)?;
                break;
            }
            event => writer.write_event(event.into_owned())?,
        }
    }
    if &removed != removed_ids {
        return Err(anyhow!(
            "PPTX relationship document is missing a relationship selected for removal"
        ));
    }
    xml_output(writer, "updated PPTX relationship XML")
}

pub(super) fn remove_content_type_overrides(
    xml: &str,
    removed_part_names: &HashSet<String>,
) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(xml.len()));
    let mut removed = HashSet::new();
    loop {
        let event = reader
            .read_event()
            .context("remove PPTX content-type overrides")?;
        match event {
            Event::Empty(override_entry) if override_entry.local_name().as_ref() == b"Override" => {
                let part_name = required_xml_attribute(&reader, &override_entry, "PartName")?;
                if removed_part_names.contains(part_name.as_str()) {
                    removed.insert(part_name);
                } else {
                    writer.write_event(Event::Empty(override_entry.into_owned()))?;
                }
            }
            Event::Start(override_entry) if override_entry.local_name().as_ref() == b"Override" => {
                return Err(anyhow!(
                    "PPTX content-type removal requires empty Override elements"
                ));
            }
            Event::Eof => {
                writer.write_event(Event::Eof)?;
                break;
            }
            event => writer.write_event(event.into_owned())?,
        }
    }
    if &removed != removed_part_names {
        return Err(anyhow!(
            "PPTX content types are missing an override selected for removal"
        ));
    }
    xml_output(writer, "updated PPTX content types")
}

pub(super) fn append_content_type_entries(
    xml: &str,
    metadata: &ContentTypesMetadata,
    required_defaults: &HashMap<String, &'static str>,
    overrides: &[(String, &'static str)],
) -> Result<String> {
    let mut missing_defaults = required_defaults
        .iter()
        .filter_map(|(extension, content_type)| {
            if let Some(existing) = metadata.defaults.get(extension) {
                if existing != content_type {
                    return Some(Err(anyhow!(
                        "PPTX content type for .{extension} conflicts with appended image data"
                    )));
                }
                None
            } else {
                Some(Ok((extension.as_str(), *content_type)))
            }
        })
        .collect::<Result<Vec<_>>>()?;
    missing_defaults.sort_by_key(|(extension, _)| *extension);
    for (part_name, _) in overrides {
        if metadata.overrides.contains(part_name) {
            return Err(anyhow!(
                "PPTX content types already contain appended part name: {part_name}"
            ));
        }
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(
        xml.len()
            .saturating_add(missing_defaults.len().saturating_mul(96))
            .saturating_add(overrides.len().saturating_mul(180)),
    ));
    let mut defaults_inserted = missing_defaults.is_empty();
    let mut overrides_inserted = false;
    loop {
        let event = reader.read_event().context("rewrite PPTX content types")?;
        if !defaults_inserted
            && matches!(
                &event,
                Event::Start(start) | Event::Empty(start)
                    if start.local_name().as_ref() == b"Override"
            )
        {
            write_content_type_defaults(
                &mut writer,
                metadata.default_tag_name.as_str(),
                missing_defaults.as_slice(),
            )?;
            defaults_inserted = true;
        }
        match &event {
            Event::End(end) if end.local_name().as_ref() == b"Types" => {
                if !defaults_inserted {
                    write_content_type_defaults(
                        &mut writer,
                        metadata.default_tag_name.as_str(),
                        missing_defaults.as_slice(),
                    )?;
                    defaults_inserted = true;
                }
                if overrides_inserted {
                    return Err(anyhow!("PPTX content types contain duplicate roots"));
                }
                for (part_name, content_type) in overrides {
                    let mut entry = BytesStart::new(metadata.override_tag_name.as_str());
                    entry.push_attribute(("PartName", part_name.as_str()));
                    entry.push_attribute(("ContentType", *content_type));
                    writer.write_event(Event::Empty(entry))?;
                }
                overrides_inserted = true;
            }
            Event::Eof => {
                writer.write_event(event.into_owned())?;
                break;
            }
            _ => {}
        }
        writer.write_event(event.into_owned())?;
    }
    if !defaults_inserted || !overrides_inserted {
        return Err(anyhow!("PPTX content types are missing a writable root"));
    }
    xml_output(writer, "updated PPTX content types XML")
}

fn write_content_type_defaults(
    writer: &mut Writer<Vec<u8>>,
    tag_name: &str,
    defaults: &[(&str, &'static str)],
) -> Result<()> {
    for (extension, content_type) in defaults {
        let mut entry = BytesStart::new(tag_name);
        entry.push_attribute(("Extension", *extension));
        entry.push_attribute(("ContentType", *content_type));
        writer.write_event(Event::Empty(entry))?;
    }
    Ok(())
}

pub(super) fn xml_output(writer: Writer<Vec<u8>>, label: &str) -> Result<String> {
    let bytes = writer.into_inner();
    if bytes.len() > super::super::MAX_XML_BYTES {
        return Err(anyhow!("{label} exceeds the local XML size limit"));
    }
    String::from_utf8(bytes).with_context(|| format!("encode {label}"))
}
