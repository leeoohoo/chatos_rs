// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use anyhow::Result;

use super::chart_xml::presentation_chart_xml;
use super::model::SlideDefinition;
use super::slide_xml::{notes_slide_xml, slide_xml};
use super::templates::{
    content_types, notes_master_relationships, notes_master_xml, notes_slide_relationships,
    presentation_relationships, presentation_xml, root_relationships, slide_layout_relationships,
    slide_layout_xml, slide_master_relationships, slide_master_xml, slide_relationships, theme_xml,
};

pub(super) fn presentation_entries(slides: &[SlideDefinition]) -> Result<Vec<(String, Vec<u8>)>> {
    let notes_present = slides.iter().any(|slide| !slide.notes.is_empty());
    let image_formats = slides
        .iter()
        .filter_map(|slide| slide.image.as_ref().map(|image| image.format))
        .collect::<HashSet<_>>();
    let mut entries = vec![
        (
            "[Content_Types].xml".to_string(),
            content_types(slides, notes_present, &image_formats).into_bytes(),
        ),
        ("_rels/.rels".to_string(), root_relationships().into_bytes()),
        (
            "ppt/presentation.xml".to_string(),
            presentation_xml(slides.len(), notes_present).into_bytes(),
        ),
        (
            "ppt/_rels/presentation.xml.rels".to_string(),
            presentation_relationships(slides.len(), notes_present).into_bytes(),
        ),
        (
            "ppt/slideMasters/slideMaster1.xml".to_string(),
            slide_master_xml().into_bytes(),
        ),
        (
            "ppt/slideMasters/_rels/slideMaster1.xml.rels".to_string(),
            slide_master_relationships().into_bytes(),
        ),
        (
            "ppt/slideLayouts/slideLayout1.xml".to_string(),
            slide_layout_xml().into_bytes(),
        ),
        (
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels".to_string(),
            slide_layout_relationships().into_bytes(),
        ),
        ("ppt/theme/theme1.xml".to_string(), theme_xml().into_bytes()),
    ];
    if notes_present {
        entries.push((
            "ppt/notesMasters/notesMaster1.xml".to_string(),
            notes_master_xml().into_bytes(),
        ));
        entries.push((
            "ppt/notesMasters/_rels/notesMaster1.xml.rels".to_string(),
            notes_master_relationships().into_bytes(),
        ));
    }
    let mut media_number = 0usize;
    let mut notes_number = 0usize;
    let mut chart_number = 0usize;
    for (index, slide) in slides.iter().enumerate() {
        let slide_number = index + 1;
        let media = slide.image.as_ref().map(|image| {
            media_number += 1;
            (media_number, image)
        });
        let note = if slide.notes.is_empty() {
            None
        } else {
            notes_number += 1;
            Some(notes_number)
        };
        let chart = slide.chart.as_ref().map(|chart| {
            chart_number += 1;
            (chart_number, chart)
        });
        entries.push((
            format!("ppt/slides/slide{slide_number}.xml"),
            slide_xml(slide, media.map(|_| "rId2"), chart.map(|_| "rId2"))?.into_bytes(),
        ));
        entries.push((
            format!("ppt/slides/_rels/slide{slide_number}.xml.rels"),
            slide_relationships(
                media.map(|(number, image)| (number, image.format)),
                chart.map(|(number, _)| number),
                note,
            )
            .into_bytes(),
        ));
        if let Some((number, image)) = media {
            entries.push((
                format!("ppt/media/image{number}.{}", image.format.extension()),
                image.bytes.clone(),
            ));
        }
        if let Some(note_number) = note {
            entries.push((
                format!("ppt/notesSlides/notesSlide{note_number}.xml"),
                notes_slide_xml(slide.notes.as_str(), slide_number)?.into_bytes(),
            ));
            entries.push((
                format!("ppt/notesSlides/_rels/notesSlide{note_number}.xml.rels"),
                notes_slide_relationships(slide_number).into_bytes(),
            ));
        }
        if let Some((chart_number, chart)) = chart {
            entries.push((
                format!("ppt/charts/chart{chart_number}.xml"),
                presentation_chart_xml(chart)?.into_bytes(),
            ));
        }
    }
    Ok(entries)
}
