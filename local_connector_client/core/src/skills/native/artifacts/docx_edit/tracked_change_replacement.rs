// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use anyhow::{anyhow, Result};

use super::super::format_helpers::{escape_xml, unescape_xml};
use super::super::MAX_XML_BYTES;
use super::{
    count_exact_xml_tags, find_last_xml_tag_start, find_text_tag, inside_open_xml_wrapper,
    quoted_attribute_values, run_has_unsupported_complex_content, MAX_DOCX_REVISION_IDS,
};

pub(super) struct ExactTextRun {
    run_start: usize,
    run_end: usize,
    text_start: usize,
    text_open_end: usize,
    text_close_start: usize,
    text_close_end: usize,
}

pub(super) fn next_revision_ids(document_xml: &str, count: usize) -> Result<Vec<u32>> {
    if !(1..=2).contains(&count) {
        return Err(anyhow!(
            "tracked replacement requires one or two revision IDs"
        ));
    }
    let mut existing = quoted_attribute_values(document_xml, "w:id")
        .into_iter()
        .filter_map(|value| value.parse::<u32>().ok())
        .collect::<HashSet<_>>();
    let mut ids = Vec::with_capacity(count);
    for candidate in 0..=MAX_DOCX_REVISION_IDS {
        if existing.insert(candidate) {
            ids.push(candidate);
            if ids.len() == count {
                return Ok(ids);
            }
        }
    }
    Err(anyhow!("DOCX revision IDs exceed the local safety limit"))
}

pub(super) fn find_exact_trackable_run(
    document_xml: &str,
    selection: &str,
) -> Result<ExactTextRun> {
    let mut cursor = 0usize;
    while let Some(text_start) = find_text_tag(document_xml, cursor) {
        let text_open_end = document_xml[text_start..]
            .find('>')
            .map(|offset| text_start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX text run has an unterminated opening tag"))?;
        let text_close_start = document_xml[text_open_end..]
            .find("</w:t>")
            .map(|offset| text_open_end + offset)
            .ok_or_else(|| anyhow!("DOCX text run has no closing tag"))?;
        let text_close_end = text_close_start + "</w:t>".len();
        if unescape_xml(&document_xml[text_open_end..text_close_start]) != selection {
            cursor = text_close_end;
            continue;
        }

        let run_start = find_last_xml_tag_start(&document_xml[..text_start], "<w:r")
            .ok_or_else(|| anyhow!("DOCX text selection is not inside a Word run"))?;
        let run_open_end = document_xml[run_start..]
            .find('>')
            .map(|offset| run_start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX run has an unterminated opening tag"))?;
        if document_xml[run_open_end..text_start].contains("</w:r>") {
            cursor = text_close_end;
            continue;
        }
        let run_close_start = document_xml[text_close_end..]
            .find("</w:r>")
            .map(|offset| text_close_end + offset)
            .ok_or_else(|| anyhow!("DOCX text selection has no closing run"))?;
        let run_end = run_close_start + "</w:r>".len();
        let run_xml = &document_xml[run_start..run_end];
        if count_exact_xml_tags(run_xml, "<w:t") != 1
            || run_has_unsupported_complex_content(run_xml)
        {
            cursor = text_close_end;
            continue;
        }

        let paragraph_start = find_last_xml_tag_start(&document_xml[..run_start], "<w:p")
            .ok_or_else(|| anyhow!("DOCX text selection is not inside a paragraph"))?;
        let paragraph_prefix = &document_xml[paragraph_start..run_start];
        if paragraph_prefix.contains("</w:p>") {
            return Err(anyhow!("DOCX text selection is outside a valid paragraph"));
        }
        let last_comment_start = find_last_xml_tag_start(paragraph_prefix, "<w:commentRangeStart");
        let last_comment_end = find_last_xml_tag_start(paragraph_prefix, "<w:commentRangeEnd");
        if last_comment_start.is_some_and(|start| last_comment_end.is_none_or(|end| start > end)) {
            return Err(anyhow!(
                "selection is already inside an existing comment range"
            ));
        }
        let document_prefix = &document_xml[..run_start];
        if [
            ("<w:ins", "</w:ins>"),
            ("<w:del", "</w:del>"),
            ("<w:moveFrom", "</w:moveFrom>"),
            ("<w:moveTo", "</w:moveTo>"),
        ]
        .iter()
        .any(|(opening, closing)| inside_open_xml_wrapper(document_prefix, opening, closing))
        {
            return Err(anyhow!(
                "selection is already inside an existing tracked revision"
            ));
        }
        return Ok(ExactTextRun {
            run_start,
            run_end,
            text_start,
            text_open_end,
            text_close_start,
            text_close_end,
        });
    }
    Err(anyhow!(
        "selection was not present as the complete text of one eligible DOCX run"
    ))
}

pub(super) fn tracked_replacement_xml(
    document_xml: &str,
    matched: &ExactTextRun,
    replacement: &str,
    author: &str,
    date: &str,
    deletion_id: u32,
    insertion_id: Option<u32>,
) -> Result<String> {
    let run_xml = &document_xml[matched.run_start..matched.run_end];
    let text_start = matched.text_start - matched.run_start;
    let text_open_end = matched.text_open_end - matched.run_start;
    let text_close_start = matched.text_close_start - matched.run_start;
    let text_close_end = matched.text_close_end - matched.run_start;

    let mut deletion_run = String::with_capacity(run_xml.len().saturating_add(14));
    deletion_run.push_str(&run_xml[..text_start]);
    deletion_run.push_str("<w:delText");
    deletion_run.push_str(&run_xml[text_start + "<w:t".len()..text_open_end]);
    deletion_run.push_str(&run_xml[text_open_end..text_close_start]);
    deletion_run.push_str("</w:delText>");
    deletion_run.push_str(&run_xml[text_close_end..]);

    let escaped_author = escape_xml(author);
    let escaped_date = escape_xml(date);
    let mut revisions = format!(
        "<w:del w:id=\"{deletion_id}\" w:author=\"{escaped_author}\" w:date=\"{escaped_date}\">{deletion_run}</w:del>"
    );
    if let Some(insertion_id) = insertion_id {
        let mut insertion_run =
            String::with_capacity(run_xml.len().saturating_add(replacement.len()));
        insertion_run.push_str(&run_xml[..text_open_end]);
        insertion_run.push_str(escape_xml(replacement).as_str());
        insertion_run.push_str(&run_xml[text_close_start..]);
        revisions.push_str(
            format!(
                "<w:ins w:id=\"{insertion_id}\" w:author=\"{escaped_author}\" w:date=\"{escaped_date}\">{insertion_run}</w:ins>"
            )
            .as_str(),
        );
    }

    let mut output = String::with_capacity(document_xml.len().saturating_add(revisions.len()));
    output.push_str(&document_xml[..matched.run_start]);
    output.push_str(revisions.as_str());
    output.push_str(&document_xml[matched.run_end..]);
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok(output)
}
