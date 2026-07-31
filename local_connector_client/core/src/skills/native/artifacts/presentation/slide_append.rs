// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use zip::ZipArchive;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::chart_xml::presentation_chart_xml;
use super::image::PresentationImageFormat;
use super::inspection_common::slide_part_number;
use super::limits::MAX_PPTX_SLIDES;
use super::package_edit::{
    append_content_type_entries, append_presentation_slide_ids, append_relationship_entries,
    appended_notes_slide_relationships, appended_slide_relationships,
};
use super::package_io::{ensure_distinct_pptx_paths, rewrite_pptx_package, validate_pptx_package};
use super::package_metadata::{
    content_types_metadata, parse_relationship_document, presentation_slide_metadata,
    RelationshipAddition,
};
use super::package_paths::{
    next_relationship_id, numbered_part, relationships_part_path, relative_part_target,
    resolve_part_target,
};
use super::slide_parse::parse_slides;
use super::slide_xml::{notes_slide_xml, slide_xml};
use super::{
    input_file, optional_bool, read_zip_text, require_extension, required_text, safe_workspace_path,
};

pub(super) fn append_pptx_slides(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (source, source_relative) =
        input_file(state, request, required_text(arguments, "path")?, ".pptx")?;
    let target_requested = required_text(arguments, "target_path")?;
    require_extension(target_requested, ".pptx")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_pptx_paths(source.as_path(), target.as_path())?;
    let slides = parse_slides(arguments, state, request)?;
    let names = validate_pptx_package(source.as_path())?;
    for required in [
        "[Content_Types].xml",
        "ppt/presentation.xml",
        "ppt/_rels/presentation.xml.rels",
    ] {
        if !names.contains(required) {
            return Err(anyhow!("PPTX is missing required package part: {required}"));
        }
    }

    let existing_slide_parts = names
        .iter()
        .filter_map(|name| slide_part_number(name.as_str()).map(|number| (number, name.clone())))
        .collect::<Vec<_>>();
    if existing_slide_parts.is_empty()
        || existing_slide_parts.len().saturating_add(slides.len()) > MAX_PPTX_SLIDES
    {
        return Err(anyhow!(
            "appended PPTX must contain between 1 and {MAX_PPTX_SLIDES} slides"
        ));
    }

    let mut archive = ZipArchive::new(File::open(source.as_path())?)
        .with_context(|| format!("open PPTX {}", source.display()))?;
    let content_types_xml = read_zip_text(&mut archive, "[Content_Types].xml")?;
    let presentation_xml = read_zip_text(&mut archive, "ppt/presentation.xml")?;
    let presentation_relationships_xml =
        read_zip_text(&mut archive, "ppt/_rels/presentation.xml.rels")?;
    let slide_metadata = presentation_slide_metadata(presentation_xml.as_str())?;
    if slide_metadata.relationship_ids.len() != existing_slide_parts.len() {
        return Err(anyhow!(
            "PPTX contains unreferenced or missing slide parts; conservative append was refused"
        ));
    }
    let presentation_relationships = parse_relationship_document(
        presentation_relationships_xml.as_str(),
        "ppt/presentation.xml",
    )?;
    let relationships_by_id = presentation_relationships
        .relationships
        .iter()
        .map(|relationship| (relationship.id.as_str(), relationship))
        .collect::<HashMap<_, _>>();
    let mut ordered_slide_paths = Vec::with_capacity(slide_metadata.relationship_ids.len());
    let mut referenced_slide_paths = HashSet::new();
    for relationship_id in &slide_metadata.relationship_ids {
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
        if !names.contains(path.as_str()) || !referenced_slide_paths.insert(path.clone()) {
            return Err(anyhow!(
                "PPTX presentation contains a missing or duplicate slide reference"
            ));
        }
        ordered_slide_paths.push(path);
    }
    let package_slide_paths = existing_slide_parts
        .iter()
        .map(|(_, path)| path.clone())
        .collect::<HashSet<_>>();
    if referenced_slide_paths != package_slide_paths {
        return Err(anyhow!(
            "PPTX contains unreferenced or missing slide parts; conservative append was refused"
        ));
    }

    let reference_slide = ordered_slide_paths
        .last()
        .ok_or_else(|| anyhow!("PPTX has no slide available for layout inheritance"))?;
    let reference_slide_relationships_path = relationships_part_path(reference_slide.as_str())?;
    if !names.contains(reference_slide_relationships_path.as_str()) {
        return Err(anyhow!(
            "PPTX reference slide is missing its relationship part"
        ));
    }
    let reference_slide_relationships_xml =
        read_zip_text(&mut archive, reference_slide_relationships_path.as_str())?;
    let reference_slide_relationships = parse_relationship_document(
        reference_slide_relationships_xml.as_str(),
        reference_slide.as_str(),
    )?;
    let layout_relationships = reference_slide_relationships
        .relationships
        .iter()
        .filter(|relationship| relationship.relationship_type.ends_with("/slideLayout"))
        .collect::<Vec<_>>();
    if layout_relationships.len() != 1 || layout_relationships[0].external {
        return Err(anyhow!(
            "PPTX reference slide must contain exactly one internal slide layout relationship"
        ));
    }
    let layout_path = resolve_part_target(
        reference_slide.as_str(),
        layout_relationships[0].target.as_str(),
    )?;
    if !names.contains(layout_path.as_str()) {
        return Err(anyhow!(
            "PPTX is missing inherited slide layout: {layout_path}"
        ));
    }

    let notes_requested = slides.iter().any(|slide| !slide.notes.is_empty());
    let notes_master_path = if notes_requested {
        let notes_master_relationships = presentation_relationships
            .relationships
            .iter()
            .filter(|relationship| relationship.relationship_type.ends_with("/notesMaster"))
            .collect::<Vec<_>>();
        if notes_master_relationships.len() != 1 || notes_master_relationships[0].external {
            return Err(anyhow!(
                "appending speaker notes requires exactly one existing internal notes master"
            ));
        }
        let path = resolve_part_target(
            "ppt/presentation.xml",
            notes_master_relationships[0].target.as_str(),
        )?;
        if !names.contains(path.as_str()) {
            return Err(anyhow!("PPTX is missing referenced notes master: {path}"));
        }
        Some(path)
    } else {
        None
    };

    let content_types = content_types_metadata(content_types_xml.as_str())?;
    let mut used_relationship_ids = presentation_relationships
        .relationships
        .iter()
        .map(|relationship| relationship.id.clone())
        .collect::<HashSet<_>>();
    let mut next_slide_number = existing_slide_parts
        .iter()
        .map(|(number, _)| *number)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| anyhow!("PPTX slide part number overflow"))?;
    let mut next_notes_number = names
        .iter()
        .filter_map(|name| numbered_part(name, "ppt/notesSlides/notesSlide", ".xml"))
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| anyhow!("PPTX notes part number overflow"))?;
    let mut next_media_number = 1usize;
    let mut next_chart_number = names
        .iter()
        .filter_map(|name| numbered_part(name, "ppt/charts/chart", ".xml"))
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| anyhow!("PPTX chart part number overflow"))?;
    let mut next_slide_id = slide_metadata
        .max_slide_id
        .max(255)
        .checked_add(1)
        .ok_or_else(|| anyhow!("PPTX slide identifier overflow"))?;
    let mut additions = Vec::<(String, Vec<u8>)>::new();
    let mut addition_names = HashSet::new();
    let mut content_type_overrides = Vec::<(String, &'static str)>::new();
    let mut required_image_defaults = HashMap::<String, &'static str>::new();
    let mut presentation_relationship_additions = Vec::with_capacity(slides.len());
    let mut presentation_slide_additions = Vec::with_capacity(slides.len());
    let mut appended_images = 0usize;
    let mut appended_charts = 0usize;
    let mut appended_notes = 0usize;

    for slide in &slides {
        while names.contains(format!("ppt/slides/slide{next_slide_number}.xml").as_str())
            || names
                .contains(format!("ppt/slides/_rels/slide{next_slide_number}.xml.rels").as_str())
        {
            next_slide_number = next_slide_number
                .checked_add(1)
                .ok_or_else(|| anyhow!("PPTX slide part number overflow"))?;
        }
        let slide_number = next_slide_number;
        next_slide_number = next_slide_number
            .checked_add(1)
            .ok_or_else(|| anyhow!("PPTX slide part number overflow"))?;
        let slide_path = format!("ppt/slides/slide{slide_number}.xml");
        let slide_relationships_path = relationships_part_path(slide_path.as_str())?;
        let layout_target = relative_part_target(slide_path.as_str(), layout_path.as_str())?;

        let image = if let Some(image) = &slide.image {
            let extension = image.format.extension();
            let media_path = loop {
                let candidate = format!("ppt/media/chatosImage{next_media_number}.{extension}");
                next_media_number = next_media_number
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("PPTX media part number overflow"))?;
                if !names.contains(candidate.as_str())
                    && !addition_names.contains(candidate.as_str())
                {
                    break candidate;
                }
            };
            addition_names.insert(media_path.clone());
            additions.push((media_path.clone(), image.bytes.clone()));
            required_image_defaults.insert(
                extension.to_string(),
                match image.format {
                    PresentationImageFormat::Png => "image/png",
                    PresentationImageFormat::Jpeg => "image/jpeg",
                },
            );
            appended_images = appended_images.saturating_add(1);
            Some(relative_part_target(
                slide_path.as_str(),
                media_path.as_str(),
            )?)
        } else {
            None
        };

        let chart = if let Some(chart) = &slide.chart {
            let chart_path = loop {
                let candidate = format!("ppt/charts/chart{next_chart_number}.xml");
                next_chart_number = next_chart_number
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("PPTX chart part number overflow"))?;
                if !names.contains(candidate.as_str())
                    && !addition_names.contains(candidate.as_str())
                {
                    break candidate;
                }
            };
            addition_names.insert(chart_path.clone());
            additions.push((
                chart_path.clone(),
                presentation_chart_xml(chart)?.into_bytes(),
            ));
            content_type_overrides.push((
                format!("/{chart_path}"),
                "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
            ));
            appended_charts = appended_charts.saturating_add(1);
            Some(relative_part_target(
                slide_path.as_str(),
                chart_path.as_str(),
            )?)
        } else {
            None
        };

        let notes = if slide.notes.is_empty() {
            None
        } else {
            let notes_master_path = notes_master_path.as_ref().ok_or_else(|| {
                anyhow!("appending speaker notes requires an existing notes master")
            })?;
            while names
                .contains(format!("ppt/notesSlides/notesSlide{next_notes_number}.xml").as_str())
                || names.contains(
                    format!("ppt/notesSlides/_rels/notesSlide{next_notes_number}.xml.rels")
                        .as_str(),
                )
            {
                next_notes_number = next_notes_number
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("PPTX notes part number overflow"))?;
            }
            let notes_number = next_notes_number;
            next_notes_number = next_notes_number
                .checked_add(1)
                .ok_or_else(|| anyhow!("PPTX notes part number overflow"))?;
            let notes_path = format!("ppt/notesSlides/notesSlide{notes_number}.xml");
            let notes_relationships_path = relationships_part_path(notes_path.as_str())?;
            let notes_master_target =
                relative_part_target(notes_path.as_str(), notes_master_path.as_str())?;
            let slide_target = relative_part_target(notes_path.as_str(), slide_path.as_str())?;
            addition_names.insert(notes_path.clone());
            addition_names.insert(notes_relationships_path.clone());
            additions.push((
                notes_path.clone(),
                notes_slide_xml(slide.notes.as_str(), slide_number)?.into_bytes(),
            ));
            additions.push((
                notes_relationships_path,
                appended_notes_slide_relationships(
                    notes_master_target.as_str(),
                    slide_target.as_str(),
                )
                .into_bytes(),
            ));
            content_type_overrides.push((
                format!("/{notes_path}"),
                "application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml",
            ));
            appended_notes = appended_notes.saturating_add(1);
            Some(relative_part_target(
                slide_path.as_str(),
                notes_path.as_str(),
            )?)
        };

        let image_relationship_id = image.as_ref().map(|_| "rId2");
        let chart_relationship_id =
            chart
                .as_ref()
                .map(|_| if image.is_some() { "rId3" } else { "rId2" });
        addition_names.insert(slide_path.clone());
        addition_names.insert(slide_relationships_path.clone());
        additions.push((
            slide_path.clone(),
            slide_xml(slide, image_relationship_id, chart_relationship_id)?.into_bytes(),
        ));
        additions.push((
            slide_relationships_path,
            appended_slide_relationships(
                layout_target.as_str(),
                image.as_deref(),
                chart.as_deref(),
                notes.as_deref(),
            )
            .into_bytes(),
        ));
        content_type_overrides.push((
            format!("/{slide_path}"),
            "application/vnd.openxmlformats-officedocument.presentationml.slide+xml",
        ));

        let relationship_id = next_relationship_id(&mut used_relationship_ids)?;
        presentation_relationship_additions.push(RelationshipAddition {
            id: relationship_id.clone(),
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide",
            target: relative_part_target("ppt/presentation.xml", slide_path.as_str())?,
        });
        presentation_slide_additions.push((next_slide_id, relationship_id));
        next_slide_id = next_slide_id
            .checked_add(1)
            .ok_or_else(|| anyhow!("PPTX slide identifier overflow"))?;
    }
    drop(archive);

    let updated_presentation = append_presentation_slide_ids(
        presentation_xml.as_str(),
        &slide_metadata,
        presentation_slide_additions.as_slice(),
    )?;
    let updated_presentation_relationships = append_relationship_entries(
        presentation_relationships_xml.as_str(),
        presentation_relationships.relationship_tag_name.as_str(),
        presentation_relationship_additions.as_slice(),
    )?;
    let updated_content_types = append_content_type_entries(
        content_types_xml.as_str(),
        &content_types,
        &required_image_defaults,
        content_type_overrides.as_slice(),
    )?;
    let replacements = BTreeMap::from([
        (
            "[Content_Types].xml".to_string(),
            updated_content_types.into_bytes(),
        ),
        (
            "ppt/presentation.xml".to_string(),
            updated_presentation.into_bytes(),
        ),
        (
            "ppt/_rels/presentation.xml.rels".to_string(),
            updated_presentation_relationships.into_bytes(),
        ),
    ]);
    let bytes = rewrite_pptx_package(
        source.as_path(),
        target.as_path(),
        &replacements,
        additions,
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "updated": true,
        "source_path": source_relative,
        "path": target_relative,
        "source_unchanged": true,
        "previous_slides": existing_slide_parts.len(),
        "appended_slides": slides.len(),
        "slides": existing_slide_parts.len().saturating_add(slides.len()),
        "appended_images": appended_images,
        "appended_charts": appended_charts,
        "appended_chart_types": slides.iter().filter_map(|slide| slide.chart.as_ref().map(|chart| chart.chart_type.as_str())).collect::<Vec<_>>(),
        "appended_speaker_notes": appended_notes,
        "inherited_slide_layout": layout_path,
        "bytes": bytes,
    }))
}
