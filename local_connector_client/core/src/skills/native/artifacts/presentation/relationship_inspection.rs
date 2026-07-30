// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};
use std::fs::File;

use anyhow::{anyhow, Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use zip::ZipArchive;

use super::super::read_zip_text;
use super::inspection_common::slide_part_number;
use super::model::{OwnedNotesPart, SlideRelationshipInspection};
use super::package_metadata::{
    optional_xml_attribute, parse_relationship_document, required_xml_attribute,
    PresentationSlideMetadata, RelationshipDocument,
};
use super::package_paths::{relationships_part_path, resolve_part_target};

pub(super) fn ensure_all_slide_parts_are_referenced(
    ordered_slide_paths: &[String],
    names: &HashSet<String>,
) -> Result<()> {
    let package_slide_paths = names
        .iter()
        .filter(|name| slide_part_number(name.as_str()).is_some())
        .cloned()
        .collect::<HashSet<_>>();
    let referenced_slide_paths = ordered_slide_paths.iter().cloned().collect::<HashSet<_>>();
    if referenced_slide_paths != package_slide_paths {
        return Err(anyhow!(
            "PPTX contains unreferenced or missing slide parts; conservative editing was refused"
        ));
    }
    Ok(())
}

pub(super) fn ensure_presentation_slide_relationships_are_exact(
    metadata: &PresentationSlideMetadata,
    relationships: &RelationshipDocument,
) -> Result<()> {
    let expected = metadata
        .relationship_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut actual = HashSet::new();
    for relationship in relationships
        .relationships
        .iter()
        .filter(|relationship| relationship.relationship_type.ends_with("/slide"))
    {
        if relationship.external || !actual.insert(relationship.id.as_str()) {
            return Err(anyhow!(
                "PPTX presentation contains ambiguous or external slide relationships"
            ));
        }
    }
    if actual != expected {
        return Err(anyhow!(
            "PPTX presentation slide relationships do not exactly match the visible slide list"
        ));
    }
    Ok(())
}

pub(super) fn reject_unsupported_slide_deletion_references(xml: &str) -> Result<()> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut in_slide_list = false;
    loop {
        match reader
            .read_event()
            .context("inspect PPTX slide deletion references")?
        {
            Event::Start(event) if event.local_name().as_ref() == b"sldIdLst" => {
                if in_slide_list {
                    return Err(anyhow!("PPTX presentation contains nested slide lists"));
                }
                in_slide_list = true;
            }
            Event::End(event) if event.local_name().as_ref() == b"sldIdLst" => {
                if !in_slide_list {
                    return Err(anyhow!(
                        "PPTX presentation contains an unmatched slide list"
                    ));
                }
                in_slide_list = false;
            }
            Event::Start(event) | Event::Empty(event)
                if matches!(
                    event.local_name().as_ref(),
                    b"custShowLst" | b"custShow" | b"sectionLst" | b"section"
                ) =>
            {
                return Err(anyhow!(
                    "PPTX custom shows or presentation sections make slide deletion ambiguous"
                ));
            }
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"sldId" && !in_slide_list =>
            {
                return Err(anyhow!(
                    "PPTX contains slide-id references outside the visible slide list"
                ));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if in_slide_list {
        return Err(anyhow!("PPTX presentation slide list is not closed"));
    }
    Ok(())
}

pub(super) fn ordered_presentation_slide_paths(
    metadata: &PresentationSlideMetadata,
    relationships: &RelationshipDocument,
    names: &HashSet<String>,
) -> Result<Vec<String>> {
    let relationships_by_id = relationships
        .relationships
        .iter()
        .map(|relationship| (relationship.id.as_str(), relationship))
        .collect::<HashMap<_, _>>();
    let mut ordered = Vec::with_capacity(metadata.relationship_ids.len());
    let mut referenced = HashSet::new();
    for relationship_id in &metadata.relationship_ids {
        let relationship = relationships_by_id
            .get(relationship_id.as_str())
            .ok_or_else(|| {
                anyhow!("PPTX presentation references a missing relationship: {relationship_id}")
            })?;
        if relationship.external || !relationship.relationship_type.ends_with("/slide") {
            return Err(anyhow!(
                "PPTX presentation slide relationship is external or has an unexpected type"
            ));
        }
        let path = resolve_part_target("ppt/presentation.xml", relationship.target.as_str())?;
        if !names.contains(path.as_str()) || !referenced.insert(path.clone()) {
            return Err(anyhow!(
                "PPTX presentation contains a missing or duplicate slide reference"
            ));
        }
        ordered.push(path);
    }
    Ok(ordered)
}

pub(super) fn owned_notes_parts_by_slide(
    archive: &mut ZipArchive<File>,
    names: &HashSet<String>,
    ordered_slide_paths: &[String],
) -> Result<Vec<Option<OwnedNotesPart>>> {
    let mut notes_by_slide = Vec::with_capacity(ordered_slide_paths.len());
    let mut notes_owners = HashMap::<String, String>::new();
    for slide_path in ordered_slide_paths {
        let slide_relationships_path = relationships_part_path(slide_path.as_str())?;
        if !names.contains(slide_relationships_path.as_str()) {
            notes_by_slide.push(None);
            continue;
        }
        let slide_relationships_xml = read_zip_text(archive, slide_relationships_path.as_str())?;
        let slide_relationships =
            parse_relationship_document(slide_relationships_xml.as_str(), slide_path.as_str())?;
        let notes_relationships = slide_relationships
            .relationships
            .iter()
            .filter(|relationship| relationship.relationship_type.ends_with("/notesSlide"))
            .collect::<Vec<_>>();
        if notes_relationships.len() > 1
            || notes_relationships
                .first()
                .is_some_and(|item| item.external)
        {
            return Err(anyhow!(
                "PPTX slide contains ambiguous or external speaker-note relationships"
            ));
        }
        let Some(notes_relationship) = notes_relationships.first() else {
            notes_by_slide.push(None);
            continue;
        };
        let notes_path =
            resolve_part_target(slide_path.as_str(), notes_relationship.target.as_str())?;
        if !names.contains(notes_path.as_str()) {
            return Err(anyhow!(
                "PPTX is missing referenced notes part: {notes_path}"
            ));
        }
        if notes_owners
            .insert(notes_path.clone(), slide_path.clone())
            .is_some()
        {
            return Err(anyhow!(
                "PPTX speaker-note part is shared by multiple slides; conservative editing was refused"
            ));
        }
        let notes_relationships_path = relationships_part_path(notes_path.as_str())?;
        if !names.contains(notes_relationships_path.as_str()) {
            return Err(anyhow!(
                "PPTX notes part is missing its relationship part: {notes_relationships_path}"
            ));
        }
        let notes_relationships_xml = read_zip_text(archive, notes_relationships_path.as_str())?;
        let notes_part_relationships =
            parse_relationship_document(notes_relationships_xml.as_str(), notes_path.as_str())?;
        let slide_back_references = notes_part_relationships
            .relationships
            .iter()
            .filter(|relationship| relationship.relationship_type.ends_with("/slide"))
            .collect::<Vec<_>>();
        if slide_back_references.len() != 1 || slide_back_references[0].external {
            return Err(anyhow!(
                "PPTX notes part must contain exactly one internal owning-slide relationship"
            ));
        }
        let owner_path = resolve_part_target(
            notes_path.as_str(),
            slide_back_references[0].target.as_str(),
        )?;
        if owner_path != *slide_path {
            return Err(anyhow!(
                "PPTX notes part owning-slide relationship does not match its slide"
            ));
        }
        notes_by_slide.push(Some(OwnedNotesPart {
            path: notes_path,
            relationships_path: notes_relationships_path,
        }));
    }
    Ok(notes_by_slide)
}

pub(super) fn inspect_slide_relationships(
    xml: &str,
    slide_path: &str,
) -> Result<SlideRelationshipInspection> {
    if xml.is_empty() {
        return Ok(SlideRelationshipInspection::default());
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut inspection = SlideRelationshipInspection::default();
    loop {
        match reader
            .read_event()
            .context("parse PPTX slide relationships")?
        {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"Relationship" =>
            {
                let relationship_type = required_xml_attribute(&reader, &event, "Type")?;
                if optional_xml_attribute(&reader, &event, "TargetMode")?
                    .is_some_and(|value| value.eq_ignore_ascii_case("external"))
                {
                    continue;
                }
                if relationship_type.ends_with("/image") {
                    inspection.image_count = inspection.image_count.saturating_add(1);
                } else if relationship_type.ends_with("/notesSlide") {
                    if inspection.notes_path.is_some() {
                        return Err(anyhow!("PPTX slide contains duplicate notes relationships"));
                    }
                    inspection.notes_path = Some(resolve_part_target(
                        slide_path,
                        required_xml_attribute(&reader, &event, "Target")?.as_str(),
                    )?);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(inspection)
}
