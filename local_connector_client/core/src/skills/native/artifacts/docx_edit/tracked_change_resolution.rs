// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, Result};

use super::super::MAX_XML_BYTES;
use super::tracked_change_model::{
    DocxTrackedRevisionAction, DocxTrackedRevisionKind, ResolvedTrackedRevisionStats,
    SimpleTrackedRevision,
};
use super::{
    count_exact_xml_tags, find_last_xml_tag_start, find_next_xml_tag_start, find_text_tag,
    quoted_attribute_values, MAX_DOCX_REVISION_IDS, MAX_DOCX_TRACKED_REVISIONS,
};

pub(super) fn resolve_tracked_revisions_xml(
    document_xml: &str,
    action: DocxTrackedRevisionAction,
    selected_ids: Option<&BTreeSet<u32>>,
) -> Result<(String, ResolvedTrackedRevisionStats)> {
    let revisions = scan_simple_tracked_revisions(document_xml)?;
    if revisions.is_empty() {
        return Err(anyhow!(
            "DOCX document body contains no supported tracked insertions or deletions"
        ));
    }

    if let Some(selected_ids) = selected_ids {
        let mut occurrences = BTreeMap::new();
        for revision in &revisions {
            *occurrences.entry(revision.id).or_insert(0usize) += 1;
        }
        for id in selected_ids {
            match occurrences.get(id).copied().unwrap_or(0) {
                1 => {}
                0 => return Err(anyhow!("requested DOCX revision ID {id} does not exist")),
                count => {
                    return Err(anyhow!(
                    "requested DOCX revision ID {id} is ambiguous because it occurs {count} times"
                ))
                }
            }
        }
    }

    let mut output = String::with_capacity(document_xml.len());
    let mut cursor = 0usize;
    let mut resolved_insertions = 0usize;
    let mut resolved_deletions = 0usize;
    let mut resolved_revision_ids = Vec::new();
    for revision in &revisions {
        output.push_str(&document_xml[cursor..revision.start]);
        let selected = selected_ids.is_none_or(|ids| ids.contains(&revision.id));
        if !selected {
            output.push_str(&document_xml[revision.start..revision.end]);
            cursor = revision.end;
            continue;
        }

        match (action, revision.kind) {
            (DocxTrackedRevisionAction::Accept, DocxTrackedRevisionKind::Insertion) => {
                output.push_str(revision.content);
            }
            (DocxTrackedRevisionAction::Reject, DocxTrackedRevisionKind::Deletion) => {
                output.push_str(restore_deleted_text(revision.content)?.as_str());
            }
            _ => {}
        }
        match revision.kind {
            DocxTrackedRevisionKind::Insertion => resolved_insertions += 1,
            DocxTrackedRevisionKind::Deletion => resolved_deletions += 1,
        }
        resolved_revision_ids.push(revision.id);
        cursor = revision.end;
    }
    output.push_str(&document_xml[cursor..]);
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    let remaining_revisions = revisions.len().saturating_sub(resolved_revision_ids.len());
    let actual_remaining = count_exact_xml_tags(output.as_str(), "<w:ins")
        .saturating_add(count_exact_xml_tags(output.as_str(), "<w:del"));
    if actual_remaining != remaining_revisions {
        return Err(anyhow!(
            "DOCX tracked changes could not be resolved without ambiguity"
        ));
    }
    Ok((
        output,
        ResolvedTrackedRevisionStats {
            insertions: resolved_insertions,
            deletions: resolved_deletions,
            resolved_revision_ids,
            total_revisions: revisions.len(),
            remaining_revisions,
        },
    ))
}

pub(super) fn scan_simple_tracked_revisions(
    document_xml: &str,
) -> Result<Vec<SimpleTrackedRevision<'_>>> {
    let insertion_count = count_exact_xml_tags(document_xml, "<w:ins");
    let deletion_count = count_exact_xml_tags(document_xml, "<w:del");
    let revision_count = insertion_count.saturating_add(deletion_count);
    if revision_count > MAX_DOCX_TRACKED_REVISIONS {
        return Err(anyhow!(
            "DOCX tracked changes exceed the {MAX_DOCX_TRACKED_REVISIONS} revision safety limit"
        ));
    }
    if insertion_count != document_xml.matches("</w:ins>").count()
        || deletion_count != document_xml.matches("</w:del>").count()
    {
        return Err(anyhow!(
            "DOCX tracked insertion/deletion markup is malformed or self-closing"
        ));
    }
    reject_unsupported_tracked_revision_markup(document_xml)?;

    let mut revisions = Vec::with_capacity(revision_count);
    let mut cursor = 0usize;
    while let Some((start, kind)) = next_tracked_revision_start(document_xml, cursor) {
        let opening_end = document_xml[start..]
            .find('>')
            .map(|offset| start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX tracked {} is unterminated", kind.label()))?;
        let opening = &document_xml[start..opening_end];
        let id = validate_tracked_revision_opening(opening, kind)?;
        let closing = kind.closing();
        let content_end = document_xml[opening_end..]
            .find(closing)
            .map(|offset| opening_end + offset)
            .ok_or_else(|| anyhow!("DOCX tracked {} has no closing tag", kind.label()))?;
        let revision_end = content_end + closing.len();
        let content = &document_xml[opening_end..content_end];
        if next_tracked_revision_start(content, 0).is_some() {
            return Err(anyhow!(
                "nested DOCX tracked insertion/deletion revisions are not supported"
            ));
        }
        if revision_intersects_comment_range(document_xml, start, revision_end) {
            return Err(anyhow!(
                "DOCX tracked revision intersects an existing comment range"
            ));
        }
        validate_simple_tracked_revision_content(content, kind)?;
        revisions.push(SimpleTrackedRevision {
            start,
            end: revision_end,
            id,
            kind,
            opening,
            content,
        });
        cursor = revision_end;
    }
    if revisions.len() != revision_count {
        return Err(anyhow!(
            "DOCX tracked insertion/deletion markup is malformed or ambiguous"
        ));
    }
    Ok(revisions)
}

fn next_tracked_revision_start(
    xml: &str,
    cursor: usize,
) -> Option<(usize, DocxTrackedRevisionKind)> {
    let insertion = find_next_xml_tag_start(xml, "<w:ins", cursor);
    let deletion = find_next_xml_tag_start(xml, "<w:del", cursor);
    match (insertion, deletion) {
        (Some(insertion), Some(deletion)) if insertion <= deletion => {
            Some((insertion, DocxTrackedRevisionKind::Insertion))
        }
        (Some(_), Some(deletion)) => Some((deletion, DocxTrackedRevisionKind::Deletion)),
        (Some(insertion), None) => Some((insertion, DocxTrackedRevisionKind::Insertion)),
        (None, Some(deletion)) => Some((deletion, DocxTrackedRevisionKind::Deletion)),
        (None, None) => None,
    }
}

fn validate_tracked_revision_opening(opening: &str, kind: DocxTrackedRevisionKind) -> Result<u32> {
    if opening
        .strip_suffix('>')
        .is_some_and(|value| value.trim_end().ends_with('/'))
    {
        return Err(anyhow!(
            "self-closing DOCX tracked {} is not supported",
            kind.label()
        ));
    }
    let ids = quoted_attribute_values(opening, "w:id");
    let id = ids
        .first()
        .filter(|_| ids.len() == 1)
        .and_then(|id| id.parse::<u32>().ok())
        .filter(|id| *id <= MAX_DOCX_REVISION_IDS)
        .ok_or_else(|| {
            anyhow!(
                "DOCX tracked {} requires one bounded numeric w:id",
                kind.label()
            )
        })?;
    Ok(id)
}

fn reject_unsupported_tracked_revision_markup(document_xml: &str) -> Result<()> {
    const UNSUPPORTED_REVISION_MARKERS: &[&str] = &[
        "<w:moveFrom",
        "<w:moveTo",
        "<w:rPrChange",
        "<w:pPrChange",
        "<w:sectPrChange",
        "<w:tblPrChange",
        "<w:tblGridChange",
        "<w:trPrChange",
        "<w:tcPrChange",
        "<w:cellIns",
        "<w:cellDel",
        "<w:cellMerge",
        "<w:customXmlInsRange",
        "<w:customXmlDelRange",
        "<w:customXmlMoveFromRange",
        "<w:customXmlMoveToRange",
    ];
    if let Some(marker) = UNSUPPORTED_REVISION_MARKERS
        .iter()
        .find(|marker| document_xml.contains(**marker))
    {
        return Err(anyhow!(
            "DOCX contains unsupported tracked revision markup: {marker}"
        ));
    }
    Ok(())
}

fn revision_intersects_comment_range(document_xml: &str, start: usize, end: usize) -> bool {
    let prefix = &document_xml[..start];
    let last_start = find_last_xml_tag_start(prefix, "<w:commentRangeStart");
    let last_end = find_last_xml_tag_start(prefix, "<w:commentRangeEnd");
    let active_at_start = last_start.is_some_and(|start| last_end.is_none_or(|end| start > end));
    let content = &document_xml[start..end];
    active_at_start
        || content.contains("<w:commentRangeStart")
        || content.contains("<w:commentRangeEnd")
        || content.contains("<w:commentReference")
}

fn validate_simple_tracked_revision_content(
    content: &str,
    kind: DocxTrackedRevisionKind,
) -> Result<()> {
    const UNSUPPORTED_CONTENT: &[&str] = &[
        "<w:p",
        "<w:tbl",
        "<w:tr",
        "<w:tc",
        "<w:sectPr",
        "<w:drawing",
        "<w:object",
        "<w:fldChar",
        "<w:instrText",
        "<w:delInstrText",
        "<w:tab",
        "<w:br",
        "<w:footnoteReference",
        "<w:endnoteReference",
        "<w:sym",
        "<w:bookmarkStart",
        "<w:bookmarkEnd",
        "<w:permStart",
        "<w:permEnd",
    ];
    if count_exact_xml_tags(content, "<w:r") == 0 {
        return Err(anyhow!(
            "DOCX tracked {} must contain at least one text run",
            kind.label()
        ));
    }
    if let Some(marker) = UNSUPPORTED_CONTENT
        .iter()
        .find(|marker| find_next_xml_tag_start(content, marker, 0).is_some())
    {
        return Err(anyhow!(
            "DOCX tracked {} contains unsupported complex content: {marker}",
            kind.label()
        ));
    }
    match kind {
        DocxTrackedRevisionKind::Insertion => {
            let texts = count_exact_xml_tags(content, "<w:t");
            if texts == 0
                || texts != content.matches("</w:t>").count()
                || count_exact_xml_tags(content, "<w:delText") != 0
            {
                return Err(anyhow!(
                    "DOCX tracked insertion must contain only well-formed active text runs"
                ));
            }
        }
        DocxTrackedRevisionKind::Deletion => {
            let deleted_texts = count_exact_xml_tags(content, "<w:delText");
            if deleted_texts == 0
                || deleted_texts != content.matches("</w:delText>").count()
                || find_text_tag(content, 0).is_some()
            {
                return Err(anyhow!(
                    "DOCX tracked deletion must contain only well-formed deleted text runs"
                ));
            }
        }
    }
    Ok(())
}

fn restore_deleted_text(content: &str) -> Result<String> {
    let mut restored = String::with_capacity(content.len());
    let mut cursor = 0usize;
    while let Some(start) = find_next_xml_tag_start(content, "<w:delText", cursor) {
        restored.push_str(&content[cursor..start]);
        let opening_end = content[start..]
            .find('>')
            .map(|offset| start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX deleted text has an unterminated opening tag"))?;
        let closing_start = content[opening_end..]
            .find("</w:delText>")
            .map(|offset| opening_end + offset)
            .ok_or_else(|| anyhow!("DOCX deleted text has no closing tag"))?;
        restored.push_str("<w:t");
        restored.push_str(&content[start + "<w:delText".len()..opening_end]);
        restored.push_str(&content[opening_end..closing_start]);
        restored.push_str("</w:t>");
        cursor = closing_start + "</w:delText>".len();
    }
    restored.push_str(&content[cursor..]);
    if restored.len() > MAX_XML_BYTES {
        return Err(anyhow!(
            "restored DOCX revision XML exceeds the local size limit"
        ));
    }
    Ok(restored)
}
