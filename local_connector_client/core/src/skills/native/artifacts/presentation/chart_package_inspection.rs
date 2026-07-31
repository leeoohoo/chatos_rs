// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};
use std::fs::File;

use anyhow::{anyhow, Context, Result};
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use zip::ZipArchive;

use super::super::read_zip_text;
use super::limits::{MAX_PPTX_CHARTS_PER_SLIDE, MAX_PPTX_CHARTS_TOTAL};
use super::model::{
    PptxChartOwnership, PptxChartRelationshipInspection, ResolvedPptxChartReference,
};
use super::{
    ensure_all_slide_parts_are_referenced, optional_xml_attribute,
    ordered_presentation_slide_paths, parse_relationship_document, presentation_slide_metadata,
    relationships_part_path, required_xml_attribute, resolve_part_target,
};

fn standard_pptx_chart_parts(names: &HashSet<String>) -> Result<HashSet<String>> {
    let mut parts = HashSet::new();
    for name in names {
        let Some(suffix) = name
            .strip_prefix("ppt/charts/chart")
            .and_then(|value| value.strip_suffix(".xml"))
        else {
            continue;
        };
        if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(anyhow!(
                "PPTX contains a nonstandard chart part name: {name}"
            ));
        }
        parts.insert(name.clone());
    }
    Ok(parts)
}

fn resolve_standard_pptx_chart_references(
    slide_xml: &str,
    relationships_xml: Option<&str>,
    slide_path: &str,
    names: &HashSet<String>,
) -> Result<Vec<ResolvedPptxChartReference>> {
    let relationship_ids = standard_pptx_slide_chart_relationship_ids(slide_xml)?;
    let Some(relationships_xml) = relationships_xml else {
        if relationship_ids.is_empty() {
            return Ok(Vec::new());
        }
        return Err(anyhow!(
            "PPTX slide chart references require a relationship part"
        ));
    };
    let relationships = parse_relationship_document(relationships_xml, slide_path)?;
    let chart_relationships = relationships
        .relationships
        .iter()
        .filter(|relationship| relationship.relationship_type.ends_with("/chart"))
        .collect::<Vec<_>>();
    if chart_relationships.len() != relationship_ids.len() {
        return Err(anyhow!(
            "PPTX slide chart relationships do not exactly match standard chart references"
        ));
    }
    let relationships_by_id = relationships
        .relationships
        .iter()
        .map(|relationship| (relationship.id.as_str(), relationship))
        .collect::<HashMap<_, _>>();
    let expected_ids = relationship_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let actual_ids = chart_relationships
        .iter()
        .map(|relationship| relationship.id.as_str())
        .collect::<HashSet<_>>();
    if expected_ids != actual_ids {
        return Err(anyhow!(
            "PPTX slide chart relationship ids do not match visible chart references"
        ));
    }
    let mut parts = HashSet::new();
    let mut resolved = Vec::with_capacity(relationship_ids.len());
    for relationship_id in relationship_ids {
        let relationship = relationships_by_id
            .get(relationship_id.as_str())
            .ok_or_else(|| anyhow!("PPTX chart references a missing relationship"))?;
        if relationship.external || !relationship.relationship_type.ends_with("/chart") {
            return Err(anyhow!(
                "PPTX standard chart relationship must be internal and use the chart relationship type"
            ));
        }
        let part = resolve_part_target(slide_path, relationship.target.as_str())?;
        let standard_part = part
            .strip_prefix("ppt/charts/chart")
            .and_then(|value| value.strip_suffix(".xml"))
            .is_some_and(|value| {
                !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
            });
        if !standard_part || !names.contains(part.as_str()) || !parts.insert(part.clone()) {
            return Err(anyhow!(
                "PPTX slide chart relationship must resolve to one unique standard chart part"
            ));
        }
        resolved.push(ResolvedPptxChartReference {
            relationship_id,
            part,
        });
    }
    Ok(resolved)
}

fn standard_pptx_slide_chart_relationship_ids(xml: &str) -> Result<Vec<String>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut stack = Vec::<(String, Option<String>)>::new();
    let mut relationship_ids = Vec::new();
    let mut seen = HashSet::new();
    let mut root_count = 0usize;
    loop {
        match reader
            .read_event()
            .context("inspect PPTX slide chart references")?
        {
            Event::Start(event) => {
                let qualified = String::from_utf8_lossy(event.name().as_ref()).into_owned();
                if stack.is_empty() {
                    if qualified != "p:sld" {
                        return Err(anyhow!(
                            "PPTX chart inspection requires a standard p:sld root"
                        ));
                    }
                    root_count = root_count.saturating_add(1);
                }
                if event.local_name().as_ref() == b"chart" {
                    return Err(anyhow!(
                        "PPTX chart references must use an empty standard c:chart element"
                    ));
                }
                let chart_uri = if event.local_name().as_ref() == b"graphicData" {
                    if qualified != "a:graphicData" {
                        return Err(anyhow!(
                            "PPTX chart graphicData must use the standard a namespace"
                        ));
                    }
                    optional_xml_attribute(&reader, &event, "uri")?
                } else {
                    None
                };
                stack.push((qualified, chart_uri));
                if stack.len() > 256 {
                    return Err(anyhow!(
                        "PPTX slide chart reference nesting exceeds the safety limit"
                    ));
                }
            }
            Event::Empty(event) if event.local_name().as_ref() == b"chart" => {
                if event.name().as_ref() != b"c:chart" {
                    return Err(anyhow!(
                        "PPTX chart references must use the standard c namespace"
                    ));
                }
                let in_standard_graphic_data = stack.iter().rev().any(|(name, uri)| {
                    name == "a:graphicData"
                        && uri.as_deref()
                            == Some("http://schemas.openxmlformats.org/drawingml/2006/chart")
                });
                if !in_standard_graphic_data {
                    return Err(anyhow!(
                        "PPTX c:chart must be inside standard chart graphicData"
                    ));
                }
                let mut relationship_id = None;
                let mut relationship_attribute_count = 0usize;
                for attribute in event.attributes().with_checks(false) {
                    let attribute = attribute.context("parse PPTX chart reference attribute")?;
                    let value = attribute
                        .decoded_and_normalized_value(XmlVersion::Explicit1_0, reader.decoder())?
                        .into_owned();
                    if attribute.key.as_ref() == b"r:id" {
                        relationship_attribute_count =
                            relationship_attribute_count.saturating_add(1);
                        relationship_id = Some(value);
                    } else if (attribute.key.as_ref() == b"xmlns:c"
                        && value
                            == "http://schemas.openxmlformats.org/drawingml/2006/chart")
                        || (attribute.key.as_ref() == b"xmlns:r"
                            && value
                                == "http://schemas.openxmlformats.org/officeDocument/2006/relationships")
                    {
                    } else {
                        return Err(anyhow!(
                            "PPTX chart reference contains an unsupported attribute"
                        ));
                    }
                }
                if relationship_attribute_count != 1 {
                    return Err(anyhow!(
                        "PPTX chart reference must contain exactly one r:id attribute"
                    ));
                }
                let relationship_id = relationship_id
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| anyhow!("PPTX chart reference is missing r:id"))?;
                if !seen.insert(relationship_id.clone()) {
                    return Err(anyhow!(
                        "PPTX slide contains duplicate standard chart references"
                    ));
                }
                relationship_ids.push(relationship_id);
                if relationship_ids.len() > MAX_PPTX_CHARTS_PER_SLIDE {
                    return Err(anyhow!(
                        "PPTX slide charts exceed the {MAX_PPTX_CHARTS_PER_SLIDE} item safety limit"
                    ));
                }
            }
            Event::Empty(event) => {
                if event.local_name().as_ref() == b"graphicData"
                    && event.name().as_ref() != b"a:graphicData"
                {
                    return Err(anyhow!(
                        "PPTX chart graphicData must use the standard a namespace"
                    ));
                }
            }
            Event::End(event) => {
                let expected = stack
                    .pop()
                    .ok_or_else(|| anyhow!("PPTX slide contains an unmatched closing tag"))?;
                if expected.0.as_bytes() != event.name().as_ref() {
                    return Err(anyhow!("PPTX slide contains mismatched element boundaries"));
                }
            }
            Event::DocType(_) | Event::PI(_) | Event::CData(_) => {
                return Err(anyhow!(
                    "PPTX chart inspection does not support slide declarations, processing instructions, or CDATA"
                ));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if root_count != 1 || !stack.is_empty() {
        return Err(anyhow!(
            "PPTX chart inspection requires one complete standard slide root"
        ));
    }
    Ok(relationship_ids)
}

pub(super) fn ensure_standard_pptx_chart_content_type(xml: &str, chart_part: &str) -> Result<()> {
    let expected_part_name = format!("/{chart_part}");
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut matches = 0usize;
    loop {
        match reader
            .read_event()
            .context("inspect PPTX chart content type")?
        {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"Override" =>
            {
                if required_xml_attribute(&reader, &event, "PartName")? == expected_part_name {
                    matches = matches.saturating_add(1);
                    if required_xml_attribute(&reader, &event, "ContentType")?
                        != "application/vnd.openxmlformats-officedocument.drawingml.chart+xml"
                    {
                        return Err(anyhow!("PPTX chart part has an unexpected content type"));
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if matches != 1 {
        return Err(anyhow!(
            "PPTX standard chart part must have exactly one chart content-type override"
        ));
    }
    Ok(())
}

pub(super) fn inspect_pptx_chart_ownership(
    archive: &mut ZipArchive<File>,
    names: &HashSet<String>,
) -> Result<PptxChartOwnership> {
    if names
        .iter()
        .any(|name| name.starts_with("ppt/charts/chartEx") && name.ends_with(".xml"))
    {
        return Err(anyhow!(
            "PPTX chart inspection does not support chartEx parts"
        ));
    }
    let presentation_xml = read_zip_text(archive, "ppt/presentation.xml")?;
    let presentation_relationships_xml = read_zip_text(archive, "ppt/_rels/presentation.xml.rels")?;
    let slide_metadata = presentation_slide_metadata(presentation_xml.as_str())?;
    let presentation_relationships = parse_relationship_document(
        presentation_relationships_xml.as_str(),
        "ppt/presentation.xml",
    )?;
    let ordered_slide_paths =
        ordered_presentation_slide_paths(&slide_metadata, &presentation_relationships, names)?;
    ensure_all_slide_parts_are_referenced(&ordered_slide_paths, names)?;
    if ordered_slide_paths.is_empty() || ordered_slide_paths.len() > 1_000 {
        return Err(anyhow!(
            "PPTX slide count is outside the chart inspection safety limit"
        ));
    }

    let mut charts_by_slide = Vec::with_capacity(ordered_slide_paths.len());
    let mut chart_owners = HashMap::<String, (usize, String)>::new();
    let mut chart_count = 0usize;
    for (slide_index, slide_path) in ordered_slide_paths.iter().enumerate() {
        let slide_xml = read_zip_text(archive, slide_path.as_str())?;
        let relationships_path = relationships_part_path(slide_path.as_str())?;
        let relationships_xml = if names.contains(relationships_path.as_str()) {
            Some(read_zip_text(archive, relationships_path.as_str())?)
        } else {
            None
        };
        let references = resolve_standard_pptx_chart_references(
            slide_xml.as_str(),
            relationships_xml.as_deref(),
            slide_path.as_str(),
            names,
        )?;
        chart_count = chart_count
            .checked_add(references.len())
            .ok_or_else(|| anyhow!("PPTX chart count overflow"))?;
        if chart_count > MAX_PPTX_CHARTS_TOTAL {
            return Err(anyhow!(
                "PPTX charts exceed the {MAX_PPTX_CHARTS_TOTAL} item safety limit"
            ));
        }
        for reference in &references {
            if let Some((owner_index, owner_relationship)) = chart_owners.insert(
                reference.part.clone(),
                (slide_index + 1, reference.relationship_id.clone()),
            ) {
                return Err(anyhow!(
                    "PPTX chart part is shared by slide {owner_index} relationship {owner_relationship} and another visible slide"
                ));
            }
        }
        charts_by_slide.push(references);
    }
    let package_chart_parts = standard_pptx_chart_parts(names)?;
    let referenced_chart_parts = chart_owners.keys().cloned().collect::<HashSet<_>>();
    if package_chart_parts != referenced_chart_parts {
        return Err(anyhow!(
            "PPTX contains unreferenced or missing standard chart parts"
        ));
    }
    Ok(PptxChartOwnership {
        ordered_slide_paths,
        charts_by_slide,
        chart_count,
    })
}

pub(super) fn inspect_pptx_chart_relationships(
    archive: &mut ZipArchive<File>,
    names: &HashSet<String>,
    chart_part: &str,
) -> Result<PptxChartRelationshipInspection> {
    let relationships_path = relationships_part_path(chart_part)?;
    if !names.contains(relationships_path.as_str()) {
        return Ok(PptxChartRelationshipInspection {
            data_source: "cached_only",
            relationship_count: 0,
            embedded_workbook: None,
            relationships_part_present: false,
        });
    }
    let relationships_xml = read_zip_text(archive, relationships_path.as_str())?;
    let relationships = parse_relationship_document(relationships_xml.as_str(), chart_part)?;
    let mut embedded_workbook = None;
    for relationship in &relationships.relationships {
        if relationship.external {
            return Err(anyhow!(
                "PPTX chart inspection refuses external chart relationships"
            ));
        }
        let target = resolve_part_target(chart_part, relationship.target.as_str())?;
        if !names.contains(target.as_str()) {
            return Err(anyhow!(
                "PPTX chart relationship references a missing package part"
            ));
        }
        if relationship.relationship_type.ends_with("/package")
            && (!target.starts_with("ppt/embeddings/")
                || embedded_workbook.replace(target).is_some())
        {
            return Err(anyhow!(
                "PPTX chart must reference at most one internal embedded workbook"
            ));
        }
    }
    Ok(PptxChartRelationshipInspection {
        data_source: if embedded_workbook.is_some() {
            "cached_with_embedded_workbook"
        } else {
            "cached_only"
        },
        relationship_count: relationships.relationships.len(),
        embedded_workbook,
        relationships_part_present: true,
    })
}
