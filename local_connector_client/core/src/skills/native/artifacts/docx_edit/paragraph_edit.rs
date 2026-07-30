// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

use super::super::MAX_XML_BYTES;
use super::paragraph_selection::{
    direct_top_level_docx_paragraphs, ensure_docx_paragraph_operation_has_no_range_markup,
    unique_eligible_top_level_paragraph, validate_indexed_docx_paragraph,
};

pub(super) fn insert_blocks_at_unique_top_level_paragraph(
    document_xml: &str,
    anchor_text: &str,
    position: &str,
    inserted_xml: &str,
) -> Result<(String, usize)> {
    let (paragraph, paragraph_number) =
        unique_eligible_top_level_paragraph(document_xml, anchor_text, "anchor_text")?;
    let insertion_point = match position {
        "before" => paragraph.start,
        "after" => paragraph.end,
        _ => return Err(anyhow!("position must be before or after")),
    };
    let mut output = String::with_capacity(document_xml.len().saturating_add(inserted_xml.len()));
    output.push_str(&document_xml[..insertion_point]);
    output.push_str(inserted_xml);
    output.push_str(&document_xml[insertion_point..]);
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok((output, paragraph_number))
}

pub(super) fn insert_blocks_at_top_level_paragraph_index(
    document_xml: &str,
    paragraph_index: usize,
    expected_text: &str,
    position: &str,
    inserted_xml: &str,
) -> Result<(String, usize)> {
    ensure_docx_paragraph_operation_has_no_range_markup(document_xml, "indexed insertion")?;
    let paragraphs = direct_top_level_docx_paragraphs(document_xml)?;
    let paragraph = *paragraphs.get(paragraph_index - 1).ok_or_else(|| {
        anyhow!(
            "paragraph index {paragraph_index} is outside the available direct top-level 1..={} range",
            paragraphs.len()
        )
    })?;
    validate_indexed_docx_paragraph(document_xml, paragraph, expected_text)?;
    let insertion_point = match position {
        "before" => paragraph.start,
        "after" => paragraph.end,
        _ => return Err(anyhow!("position must be before or after")),
    };
    let mut output = String::with_capacity(document_xml.len().saturating_add(inserted_xml.len()));
    output.push_str(&document_xml[..insertion_point]);
    output.push_str(inserted_xml);
    output.push_str(&document_xml[insertion_point..]);
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok((output, paragraphs.len()))
}

pub(super) fn delete_unique_top_level_paragraph(
    document_xml: &str,
    anchor_text: &str,
) -> Result<(String, usize)> {
    let (paragraph, paragraph_number) =
        unique_eligible_top_level_paragraph(document_xml, anchor_text, "anchor_text")?;
    let mut output = String::with_capacity(
        document_xml
            .len()
            .saturating_sub(paragraph.end.saturating_sub(paragraph.start)),
    );
    output.push_str(&document_xml[..paragraph.start]);
    output.push_str(&document_xml[paragraph.end..]);
    Ok((output, paragraph_number))
}

pub(super) fn delete_top_level_paragraph_at_index(
    document_xml: &str,
    paragraph_index: usize,
    expected_text: &str,
) -> Result<(String, usize)> {
    ensure_docx_paragraph_operation_has_no_range_markup(document_xml, "indexed deletion")?;
    let paragraphs = direct_top_level_docx_paragraphs(document_xml)?;
    let paragraph = *paragraphs.get(paragraph_index - 1).ok_or_else(|| {
        anyhow!(
            "paragraph index {paragraph_index} is outside the available direct top-level 1..={} range",
            paragraphs.len()
        )
    })?;
    validate_indexed_docx_paragraph(document_xml, paragraph, expected_text)?;
    let mut output = String::with_capacity(
        document_xml
            .len()
            .saturating_sub(paragraph.end.saturating_sub(paragraph.start)),
    );
    output.push_str(&document_xml[..paragraph.start]);
    output.push_str(&document_xml[paragraph.end..]);
    Ok((output, paragraphs.len()))
}

pub(super) fn move_unique_top_level_paragraph(
    document_xml: &str,
    anchor_text: &str,
    reference_text: &str,
    position: &str,
) -> Result<(String, usize, usize)> {
    ensure_docx_paragraph_operation_has_no_range_markup(document_xml, "movement")?;
    let (anchor, anchor_number) =
        unique_eligible_top_level_paragraph(document_xml, anchor_text, "anchor_text")?;
    let (reference, reference_number) =
        unique_eligible_top_level_paragraph(document_xml, reference_text, "reference_text")?;
    if anchor.start == reference.start && anchor.end == reference.end {
        return Err(anyhow!(
            "anchor_text and reference_text must select distinct paragraphs"
        ));
    }
    let already_positioned = match position {
        "before" if anchor.end <= reference.start => {
            document_xml[anchor.end..reference.start].trim().is_empty()
        }
        "after" if reference.end <= anchor.start => {
            document_xml[reference.end..anchor.start].trim().is_empty()
        }
        _ => false,
    };
    if already_positioned {
        return Err(anyhow!(
            "paragraph is already in the requested position relative to reference_text"
        ));
    }
    let anchor_xml = &document_xml[anchor.start..anchor.end];
    let anchor_len = anchor.end - anchor.start;
    let mut without_anchor = String::with_capacity(document_xml.len() - anchor_len);
    without_anchor.push_str(&document_xml[..anchor.start]);
    without_anchor.push_str(&document_xml[anchor.end..]);
    let reference_start = if reference.start > anchor.start {
        reference.start - anchor_len
    } else {
        reference.start
    };
    let reference_end = if reference.end > anchor.end {
        reference.end - anchor_len
    } else {
        reference.end
    };
    let insertion_point = match position {
        "before" => reference_start,
        "after" => reference_end,
        _ => return Err(anyhow!("position must be before or after")),
    };
    without_anchor.insert_str(insertion_point, anchor_xml);
    if without_anchor == document_xml {
        return Err(anyhow!(
            "paragraph is already in the requested position relative to reference_text"
        ));
    }
    Ok((without_anchor, anchor_number, reference_number))
}

pub(super) fn move_top_level_paragraph_at_indices(
    document_xml: &str,
    paragraph_index: usize,
    expected_text: &str,
    reference_paragraph_index: usize,
    reference_expected_text: &str,
    position: &str,
) -> Result<(String, usize, usize)> {
    ensure_docx_paragraph_operation_has_no_range_markup(document_xml, "indexed movement")?;
    let paragraphs = direct_top_level_docx_paragraphs(document_xml)?;
    let paragraph = *paragraphs.get(paragraph_index - 1).ok_or_else(|| {
        anyhow!(
            "paragraph index {paragraph_index} is outside the available direct top-level 1..={} range",
            paragraphs.len()
        )
    })?;
    let reference = *paragraphs
        .get(reference_paragraph_index - 1)
        .ok_or_else(|| {
            anyhow!(
                "reference_paragraph index {reference_paragraph_index} is outside the available direct top-level 1..={} range",
                paragraphs.len()
            )
        })?;
    if paragraph.start == reference.start && paragraph.end == reference.end {
        return Err(anyhow!(
            "paragraph and reference_paragraph must select distinct paragraphs"
        ));
    }
    validate_indexed_docx_paragraph(document_xml, paragraph, expected_text)?;
    validate_indexed_docx_paragraph(document_xml, reference, reference_expected_text)?;
    let already_positioned = match position {
        "before" if paragraph.end <= reference.start => document_xml
            [paragraph.end..reference.start]
            .trim()
            .is_empty(),
        "after" if reference.end <= paragraph.start => document_xml[reference.end..paragraph.start]
            .trim()
            .is_empty(),
        _ => false,
    };
    if already_positioned {
        return Err(anyhow!(
            "paragraph is already in the requested position relative to reference_paragraph"
        ));
    }

    let paragraph_xml = &document_xml[paragraph.start..paragraph.end];
    let paragraph_len = paragraph.end - paragraph.start;
    let mut without_paragraph = String::with_capacity(document_xml.len() - paragraph_len);
    without_paragraph.push_str(&document_xml[..paragraph.start]);
    without_paragraph.push_str(&document_xml[paragraph.end..]);
    let reference_start = if reference.start > paragraph.start {
        reference.start - paragraph_len
    } else {
        reference.start
    };
    let reference_end = if reference.end > paragraph.end {
        reference.end - paragraph_len
    } else {
        reference.end
    };
    let insertion_point = match position {
        "before" => reference_start,
        "after" => reference_end,
        _ => return Err(anyhow!("position must be before or after")),
    };
    without_paragraph.insert_str(insertion_point, paragraph_xml);
    if without_paragraph == document_xml {
        return Err(anyhow!(
            "paragraph is already in the requested position relative to reference_paragraph"
        ));
    }
    let moved_paragraph = match (paragraph_index < reference_paragraph_index, position) {
        (true, "before") => reference_paragraph_index - 1,
        (true, "after") => reference_paragraph_index,
        (false, "before") => reference_paragraph_index,
        (false, "after") => reference_paragraph_index + 1,
        (_, _) => return Err(anyhow!("position must be before or after")),
    };
    Ok((without_paragraph, paragraphs.len(), moved_paragraph))
}

pub(super) fn replace_unique_top_level_paragraph_with_blocks(
    document_xml: &str,
    anchor_text: &str,
    replacement_xml: &str,
) -> Result<(String, usize)> {
    ensure_docx_paragraph_operation_has_no_range_markup(document_xml, "replacement")?;
    let (paragraph, paragraph_number) =
        unique_eligible_top_level_paragraph(document_xml, anchor_text, "anchor_text")?;
    let mut output = String::with_capacity(
        document_xml
            .len()
            .saturating_sub(paragraph.end - paragraph.start)
            .saturating_add(replacement_xml.len()),
    );
    output.push_str(&document_xml[..paragraph.start]);
    output.push_str(replacement_xml);
    output.push_str(&document_xml[paragraph.end..]);
    if output == document_xml {
        return Err(anyhow!(
            "replacement blocks are identical to the selected DOCX paragraph"
        ));
    }
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok((output, paragraph_number))
}

pub(super) fn replace_top_level_paragraph_at_index_with_blocks(
    document_xml: &str,
    paragraph_index: usize,
    expected_text: &str,
    replacement_xml: &str,
) -> Result<(String, usize)> {
    ensure_docx_paragraph_operation_has_no_range_markup(document_xml, "indexed replacement")?;
    let paragraphs = direct_top_level_docx_paragraphs(document_xml)?;
    let paragraph = *paragraphs.get(paragraph_index - 1).ok_or_else(|| {
        anyhow!(
            "paragraph index {paragraph_index} is outside the available direct top-level 1..={} range",
            paragraphs.len()
        )
    })?;
    validate_indexed_docx_paragraph(document_xml, paragraph, expected_text)?;
    let mut output = String::with_capacity(
        document_xml
            .len()
            .saturating_sub(paragraph.end - paragraph.start)
            .saturating_add(replacement_xml.len()),
    );
    output.push_str(&document_xml[..paragraph.start]);
    output.push_str(replacement_xml);
    output.push_str(&document_xml[paragraph.end..]);
    if output == document_xml {
        return Err(anyhow!(
            "replacement blocks are identical to the selected DOCX paragraph"
        ));
    }
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok((output, paragraphs.len()))
}
