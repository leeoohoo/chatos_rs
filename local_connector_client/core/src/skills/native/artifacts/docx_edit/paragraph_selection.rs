// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

use super::{
    count_exact_xml_tags, docx_visible_text, find_next_xml_tag_start,
    paragraph_has_unsupported_cross_run_content, simple_docx_text_runs, xml_element_ranges,
    xml_open_element_stack_at, xml_tag_end, XmlElementRange, MAX_DOCX_BLOCKS,
    MAX_DOCX_REPLACEMENTS,
};

pub(super) fn direct_top_level_docx_paragraphs(document_xml: &str) -> Result<Vec<XmlElementRange>> {
    if document_xml.contains("<!--")
        || document_xml.contains("<![CDATA[")
        || document_xml.contains("<!DOCTYPE")
    {
        return Err(anyhow!(
            "DOCX indexed paragraph editing does not support comments, CDATA, or DTD markup"
        ));
    }
    if !xml_open_element_stack_at(document_xml, document_xml.len())?.is_empty() {
        return Err(anyhow!(
            "DOCX indexed paragraph editing requires structurally complete document XML"
        ));
    }
    let bodies = xml_element_ranges(document_xml, "<w:body", "</w:body>", 1, "DOCX bodies")?;
    if bodies.len() != 1 {
        return Err(anyhow!("DOCX must contain exactly one standard w:body"));
    }
    let body = bodies[0];
    let mut paragraphs = Vec::new();
    let mut cursor = body.open_end;
    while let Some(start) = find_next_xml_tag_start(document_xml, "<w:p", cursor) {
        if start >= body.close_start {
            break;
        }
        let open_end = xml_tag_end(document_xml, start, body.close_start)?;
        if xml_open_element_stack_at(document_xml, start)? != ["w:document", "w:body"] {
            cursor = open_end;
            continue;
        }
        let self_closing = document_xml[start..open_end - 1].trim_end().ends_with('/');
        let paragraph = if self_closing {
            XmlElementRange {
                start,
                open_end,
                close_start: open_end,
                end: open_end,
            }
        } else {
            let close_start = document_xml[open_end..body.close_start]
                .find("</w:p>")
                .map(|offset| open_end + offset)
                .ok_or_else(|| anyhow!("DOCX top-level paragraph has no closing tag"))?;
            let end = close_start + "</w:p>".len();
            let paragraph = XmlElementRange {
                start,
                open_end,
                close_start,
                end,
            };
            if count_exact_xml_tags(&document_xml[start..end], "<w:p") != 1 {
                return Err(anyhow!(
                    "DOCX top-level paragraph is structurally ambiguous"
                ));
            }
            paragraph
        };
        paragraphs.push(paragraph);
        if paragraphs.len() > MAX_DOCX_BLOCKS {
            return Err(anyhow!(
                "DOCX contains more than the {MAX_DOCX_BLOCKS} direct top-level paragraph safety limit"
            ));
        }
        cursor = paragraph.end;
    }
    Ok(paragraphs)
}

pub(super) fn validate_indexed_docx_paragraph(
    document_xml: &str,
    paragraph: XmlElementRange,
    expected_text: &str,
) -> Result<()> {
    let paragraph_xml = &document_xml[paragraph.start..paragraph.end];
    if paragraph_xml.contains("<w:sectPr") {
        return Err(anyhow!(
            "selected DOCX paragraph contains section properties and cannot be edited safely"
        ));
    }
    if paragraph_has_unsupported_cross_run_content(paragraph_xml) {
        return Err(anyhow!(
            "selected DOCX paragraph contains a hyperlink, field, comment, revision, bookmark, drawing, control, or wrapper element"
        ));
    }
    let visible_text = docx_visible_text(paragraph_xml)?;
    if visible_text != expected_text {
        return Err(anyhow!(
            "selected DOCX paragraph text does not match expected_text"
        ));
    }
    if count_exact_xml_tags(paragraph_xml, "<w:r") == 0 {
        if !visible_text.is_empty() {
            return Err(anyhow!(
                "selected DOCX paragraph text is not represented by direct simple runs"
            ));
        }
        if paragraph.open_end == paragraph.close_start {
            return Ok(());
        }
        let inner = document_xml[paragraph.open_end..paragraph.close_start].trim();
        if inner.is_empty() {
            return Ok(());
        }
        let properties_start = find_next_xml_tag_start(inner, "<w:pPr", 0);
        if properties_start != Some(0) {
            return Err(anyhow!(
                "selected empty DOCX paragraph contains unsupported content"
            ));
        }
        let properties_open_end = xml_tag_end(inner, 0, inner.len())?;
        let properties_end = if inner[..properties_open_end - 1].trim_end().ends_with('/') {
            properties_open_end
        } else {
            inner[properties_open_end..]
                .find("</w:pPr>")
                .map(|offset| properties_open_end + offset + "</w:pPr>".len())
                .ok_or_else(|| anyhow!("DOCX paragraph properties have no closing tag"))?
        };
        if !inner[properties_end..].trim().is_empty()
            || count_exact_xml_tags(&inner[..properties_end], "<w:pPr") != 1
        {
            return Err(anyhow!(
                "selected empty DOCX paragraph contains unsupported content"
            ));
        }
        return Ok(());
    }
    let runs = simple_docx_text_runs(document_xml, paragraph)?;
    if runs
        .iter()
        .map(|run| run.decoded.as_str())
        .collect::<String>()
        != visible_text
    {
        return Err(anyhow!(
            "selected DOCX paragraph text is not represented by direct simple runs"
        ));
    }
    Ok(())
}

pub(super) fn ensure_docx_paragraph_operation_has_no_range_markup(
    document_xml: &str,
    operation: &str,
) -> Result<()> {
    const RANGE_MARKERS: &[&str] = &[
        "<w:commentRange",
        "<w:commentReference",
        "<w:bookmark",
        "<w:perm",
        "<w:proofErr",
        "<w:moveFromRange",
        "<w:moveToRange",
        "<w:customXmlInsRange",
        "<w:customXmlDelRange",
        "<w:customXmlMoveFromRange",
        "<w:customXmlMoveToRange",
    ];
    if let Some(marker) = RANGE_MARKERS
        .iter()
        .find(|marker| document_xml.contains(**marker))
    {
        return Err(anyhow!(
            "DOCX paragraph {operation} does not support document range markup: {marker}"
        ));
    }
    Ok(())
}

pub(super) fn unique_eligible_top_level_paragraph(
    document_xml: &str,
    paragraph_text: &str,
    field: &str,
) -> Result<(XmlElementRange, usize)> {
    if document_xml.contains("<!--")
        || document_xml.contains("<![CDATA[")
        || document_xml.contains("<!DOCTYPE")
    {
        return Err(anyhow!(
            "DOCX paragraph-anchor editing does not support comments, CDATA, or DTD markup"
        ));
    }
    let bodies = xml_element_ranges(document_xml, "<w:body", "</w:body>", 1, "DOCX bodies")?;
    if bodies.len() != 1 {
        return Err(anyhow!("DOCX must contain exactly one standard w:body"));
    }
    let body = bodies[0];
    let paragraph_ranges = xml_element_ranges(
        &document_xml[body.open_end..body.close_start],
        "<w:p",
        "</w:p>",
        MAX_DOCX_REPLACEMENTS,
        "DOCX paragraphs",
    )?;
    let mut occurrences = 0usize;
    let mut top_level_paragraph = 0usize;
    let mut matched = None::<(XmlElementRange, usize)>;
    let mut unsupported_reason = None::<String>;
    for relative in paragraph_ranges {
        let paragraph = XmlElementRange {
            start: body.open_end + relative.start,
            open_end: body.open_end + relative.open_end,
            close_start: body.open_end + relative.close_start,
            end: body.open_end + relative.end,
        };
        let direct =
            xml_open_element_stack_at(document_xml, paragraph.start)? == ["w:document", "w:body"];
        if direct {
            top_level_paragraph += 1;
        }
        let paragraph_xml = &document_xml[paragraph.start..paragraph.end];
        let visible_text = docx_visible_text(paragraph_xml)?;
        if visible_text != paragraph_text {
            continue;
        }
        occurrences += 1;
        if occurrences > 1 {
            return Err(anyhow!(
                "{field} must match exactly one visible DOCX paragraph"
            ));
        }
        if !direct {
            unsupported_reason =
                Some("matched paragraph is not a direct top-level child of w:body".to_string());
            continue;
        }
        if paragraph_xml.contains("<w:sectPr") {
            unsupported_reason = Some(
                "matched paragraph contains section properties that cannot be moved safely"
                    .to_string(),
            );
            continue;
        }
        if paragraph_has_unsupported_cross_run_content(paragraph_xml) {
            unsupported_reason = Some(
                "matched paragraph contains a hyperlink, field, comment, revision, bookmark, drawing, control, or wrapper element"
                    .to_string(),
            );
            continue;
        }
        let runs = simple_docx_text_runs(document_xml, paragraph)?;
        if runs
            .iter()
            .map(|run| run.decoded.as_str())
            .collect::<String>()
            != visible_text
        {
            unsupported_reason = Some(
                "matched paragraph visible text is not represented by direct simple runs"
                    .to_string(),
            );
            continue;
        }
        matched = Some((paragraph, top_level_paragraph));
    }
    if occurrences == 0 {
        return Err(anyhow!(
            "{field} was not present as the complete visible text of a DOCX paragraph"
        ));
    }
    matched.ok_or_else(|| {
        anyhow!(
            "{field} is not an eligible top-level DOCX paragraph: {}",
            unsupported_reason.unwrap_or_else(|| "unsupported DOCX structure".to_string())
        )
    })
}
