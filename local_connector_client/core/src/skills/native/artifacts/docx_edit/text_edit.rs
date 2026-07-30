// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

use super::super::format_helpers::{escape_xml, unescape_xml};
use super::{
    count_exact_xml_tags, run_has_unsupported_complex_content, xml_element_ranges, XmlElementRange,
    MAX_DOCX_CROSS_RUNS, MAX_DOCX_REPLACEMENTS, MAX_XML_BYTES,
};

#[derive(Clone)]
pub(super) struct SimpleDocxTextRun {
    run_start: usize,
    run_end: usize,
    text_start: usize,
    text_open_end: usize,
    text_close_end: usize,
    formatting: String,
    pub(super) decoded: String,
}

struct CrossRunTextMatch {
    runs: Vec<SimpleDocxTextRun>,
    first_offset: usize,
    last_offset: usize,
}

pub(super) fn replace_text_runs(
    document_xml: &str,
    find: &str,
    replacement: &str,
    max_replacements: usize,
) -> Result<(String, usize, bool)> {
    let mut output = String::with_capacity(document_xml.len());
    let mut cursor = 0usize;
    let mut replacements = 0usize;
    let mut replacement_limit_reached = false;
    while let Some(tag_start) = find_text_tag(document_xml, cursor) {
        let tag_end = document_xml[tag_start..]
            .find('>')
            .map(|offset| tag_start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX text run has an unterminated opening tag"))?;
        let close_start = document_xml[tag_end..]
            .find("</w:t>")
            .map(|offset| tag_end + offset)
            .ok_or_else(|| anyhow!("DOCX text run has no closing tag"))?;
        output.push_str(&document_xml[cursor..tag_end]);
        let decoded = unescape_xml(&document_xml[tag_end..close_start]);
        let remaining = max_replacements.saturating_sub(replacements);
        let matches = decoded.matches(find).count();
        let count = matches.min(remaining);
        replacement_limit_reached |= matches > count;
        if count > 0 {
            output
                .push_str(escape_xml(decoded.replacen(find, replacement, count).as_str()).as_str());
            replacements += count;
        } else {
            output.push_str(&document_xml[tag_end..close_start]);
        }
        cursor = close_start;
    }
    output.push_str(&document_xml[cursor..]);
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok((output, replacements, replacement_limit_reached))
}

pub(super) fn replace_one_text_across_runs(
    document_xml: &str,
    selection: &str,
    replacement: &str,
) -> Result<(String, usize, usize)> {
    if document_xml.contains("<!--")
        || document_xml.contains("<![CDATA[")
        || document_xml.contains("<!DOCTYPE")
    {
        return Err(anyhow!(
            "DOCX cross-run replacement does not support comments, CDATA, or DTD markup"
        ));
    }
    let paragraphs = xml_element_ranges(
        document_xml,
        "<w:p",
        "</w:p>",
        MAX_DOCX_REPLACEMENTS,
        "DOCX paragraphs",
    )?;
    let mut occurrences = 0usize;
    let mut matched = None::<CrossRunTextMatch>;
    let mut unsupported_reason = None::<String>;
    for paragraph in paragraphs {
        let paragraph_xml = &document_xml[paragraph.start..paragraph.end];
        let visible_text = docx_visible_text(paragraph_xml)?;
        for start in overlapping_text_match_starts(visible_text.as_str(), selection) {
            occurrences += 1;
            if occurrences > 1 {
                return Err(anyhow!(
                    "selection must appear exactly once in visible DOCX paragraph text"
                ));
            }
            let candidate = cross_run_match_in_paragraph(
                document_xml,
                paragraph,
                visible_text.as_str(),
                start,
                start + selection.len(),
            );
            match candidate {
                Ok(candidate) => matched = Some(candidate),
                Err(error) => unsupported_reason = Some(error.to_string()),
            }
        }
    }
    if occurrences == 0 {
        return Err(anyhow!(
            "selection was not present in visible DOCX paragraph text"
        ));
    }
    let matched = matched.ok_or_else(|| {
        anyhow!(
            "selection is not an eligible same-format adjacent cross-run match: {}",
            unsupported_reason.unwrap_or_else(|| "unsupported DOCX structure".to_string())
        )
    })?;
    rewrite_cross_run_match(document_xml, &matched, replacement)
}

fn cross_run_match_in_paragraph(
    document_xml: &str,
    paragraph: XmlElementRange,
    visible_text: &str,
    selection_start: usize,
    selection_end: usize,
) -> Result<CrossRunTextMatch> {
    let paragraph_xml = &document_xml[paragraph.start..paragraph.end];
    if paragraph_has_unsupported_cross_run_content(paragraph_xml) {
        return Err(anyhow!(
            "paragraph contains a hyperlink, field, comment, revision, bookmark, drawing, control, or wrapper element"
        ));
    }
    let runs = simple_docx_text_runs(document_xml, paragraph)?;
    let combined = runs
        .iter()
        .map(|run| run.decoded.as_str())
        .collect::<String>();
    if combined != visible_text {
        return Err(anyhow!(
            "paragraph visible text is not represented by direct simple runs"
        ));
    }
    let mut cumulative = 0usize;
    let mut first = None::<(usize, usize)>;
    let mut last = None::<(usize, usize)>;
    for (index, run) in runs.iter().enumerate() {
        let next = cumulative + run.decoded.len();
        if first.is_none() && selection_start >= cumulative && selection_start < next {
            first = Some((index, selection_start - cumulative));
        }
        if selection_end > cumulative && selection_end <= next {
            last = Some((index, selection_end - cumulative));
            break;
        }
        cumulative = next;
    }
    let (first_index, first_offset) =
        first.ok_or_else(|| anyhow!("selection start does not map to a simple DOCX text run"))?;
    let (last_index, last_offset) =
        last.ok_or_else(|| anyhow!("selection end does not map to a simple DOCX text run"))?;
    if first_index == last_index {
        return Err(anyhow!(
            "selection is contained inside one run; use replace_docx_text instead"
        ));
    }
    let touched = last_index - first_index + 1;
    if touched > MAX_DOCX_CROSS_RUNS {
        return Err(anyhow!(
            "selection spans {touched} runs, exceeding the {MAX_DOCX_CROSS_RUNS} run safety limit"
        ));
    }
    let formatting = runs[first_index].formatting.as_str();
    if runs[first_index..=last_index]
        .iter()
        .any(|run| run.formatting != formatting)
    {
        return Err(anyhow!(
            "selection crosses runs with different run properties"
        ));
    }
    for pair in runs[first_index..=last_index].windows(2) {
        if !document_xml[pair[0].run_end..pair[1].run_start]
            .trim()
            .is_empty()
        {
            return Err(anyhow!(
                "selection crosses non-run markup between adjacent runs"
            ));
        }
    }
    Ok(CrossRunTextMatch {
        runs: runs[first_index..=last_index].to_vec(),
        first_offset,
        last_offset,
    })
}

pub(super) fn simple_docx_text_runs(
    document_xml: &str,
    paragraph: XmlElementRange,
) -> Result<Vec<SimpleDocxTextRun>> {
    let paragraph_xml = &document_xml[paragraph.start..paragraph.end];
    let ranges = xml_element_ranges(
        paragraph_xml,
        "<w:r",
        "</w:r>",
        1_000,
        "DOCX paragraph runs",
    )?;
    if ranges.is_empty() {
        return Err(anyhow!("paragraph contains no simple Word runs"));
    }
    let mut runs = Vec::with_capacity(ranges.len());
    for range in ranges {
        let run_start = paragraph.start + range.start;
        let run_end = paragraph.start + range.end;
        let run_xml = &document_xml[run_start..run_end];
        if count_exact_xml_tags(run_xml, "<w:t") != 1
            || run_xml.matches("</w:t>").count() != 1
            || run_has_unsupported_complex_content(run_xml)
        {
            return Err(anyhow!(
                "paragraph contains a run that is not one simple text run"
            ));
        }
        let text_start_relative =
            find_text_tag(run_xml, 0).ok_or_else(|| anyhow!("simple DOCX run is missing w:t"))?;
        let text_open_end_relative = run_xml[text_start_relative..]
            .find('>')
            .map(|offset| text_start_relative + offset + 1)
            .ok_or_else(|| anyhow!("DOCX text run has an unterminated opening tag"))?;
        let text_opening = &run_xml[text_start_relative..text_open_end_relative];
        if !matches!(text_opening, "<w:t>" | "<w:t xml:space=\"preserve\">") {
            return Err(anyhow!(
                "DOCX cross-run replacement supports only standard w:t opening tags"
            ));
        }
        let text_close_start_relative = run_xml[text_open_end_relative..]
            .find("</w:t>")
            .map(|offset| text_open_end_relative + offset)
            .ok_or_else(|| anyhow!("DOCX text run has no closing tag"))?;
        let text_close_end_relative = text_close_start_relative + "</w:t>".len();
        let raw_text = &run_xml[text_open_end_relative..text_close_start_relative];
        if raw_text.contains('<') {
            return Err(anyhow!(
                "DOCX cross-run text contains unsupported nested XML"
            ));
        }
        let prefix = run_xml[range.open_end - range.start..text_start_relative].trim();
        let formatting = simple_docx_run_properties(prefix)?;
        if !run_xml[text_close_end_relative..range.close_start - range.start]
            .trim()
            .is_empty()
        {
            return Err(anyhow!("DOCX simple text run contains content after w:t"));
        }
        runs.push(SimpleDocxTextRun {
            run_start,
            run_end,
            text_start: run_start + text_start_relative,
            text_open_end: run_start + text_open_end_relative,
            text_close_end: run_start + text_close_end_relative,
            formatting,
            decoded: unescape_xml(raw_text),
        });
    }
    if count_exact_xml_tags(paragraph_xml, "<w:t") != runs.len()
        || paragraph_xml.matches("</w:t>").count() != runs.len()
    {
        return Err(anyhow!(
            "paragraph contains text outside the direct simple runs"
        ));
    }
    Ok(runs)
}

fn simple_docx_run_properties(prefix: &str) -> Result<String> {
    if prefix.is_empty() {
        return Ok(String::new());
    }
    let ranges = xml_element_ranges(prefix, "<w:rPr", "</w:rPr>", 1, "DOCX run properties")?;
    if ranges.len() != 1
        || !prefix[..ranges[0].start].trim().is_empty()
        || !prefix[ranges[0].end..].trim().is_empty()
    {
        return Err(anyhow!(
            "DOCX simple text run contains content other than one run-properties element"
        ));
    }
    Ok(prefix[ranges[0].start..ranges[0].end].to_string())
}

pub(super) fn docx_visible_text(xml: &str) -> Result<String> {
    let mut output = String::new();
    let mut cursor = 0usize;
    while let Some(text_start) = find_text_tag(xml, cursor) {
        let text_open_end = xml[text_start..]
            .find('>')
            .map(|offset| text_start + offset + 1)
            .ok_or_else(|| anyhow!("DOCX text run has an unterminated opening tag"))?;
        let text_close_start = xml[text_open_end..]
            .find("</w:t>")
            .map(|offset| text_open_end + offset)
            .ok_or_else(|| anyhow!("DOCX text run has no closing tag"))?;
        let raw = &xml[text_open_end..text_close_start];
        if raw.contains('<') {
            return Err(anyhow!("DOCX text run contains unsupported nested XML"));
        }
        output.push_str(unescape_xml(raw).as_str());
        cursor = text_close_start + "</w:t>".len();
    }
    Ok(output)
}

fn overlapping_text_match_starts(text: &str, selection: &str) -> Vec<usize> {
    text.char_indices()
        .filter_map(|(index, _)| text[index..].starts_with(selection).then_some(index))
        .collect()
}

pub(super) fn paragraph_has_unsupported_cross_run_content(paragraph_xml: &str) -> bool {
    [
        "<w:hyperlink",
        "<w:fldSimple",
        "<w:fldChar",
        "<w:instrText",
        "<w:comment",
        "<w:ins",
        "<w:del",
        "<w:moveFrom",
        "<w:moveTo",
        "<w:bookmark",
        "<w:proofErr",
        "<w:permStart",
        "<w:permEnd",
        "<w:drawing",
        "<w:object",
        "<w:tab",
        "<w:br",
        "<w:cr",
        "<w:footnoteReference",
        "<w:endnoteReference",
        "<w:sym",
        "<w:sdt",
        "<w:smartTag",
        "<w:customXml",
        "<w:altChunk",
        "<m:oMath",
    ]
    .iter()
    .any(|marker| paragraph_xml.contains(marker))
}

fn rewrite_cross_run_match(
    document_xml: &str,
    matched: &CrossRunTextMatch,
    replacement: &str,
) -> Result<(String, usize, usize)> {
    let mut replacements = Vec::<(usize, usize, String)>::with_capacity(matched.runs.len());
    let mut emptied_runs = 0usize;
    let last_index = matched.runs.len() - 1;
    for (index, run) in matched.runs.iter().enumerate() {
        let text = if index == 0 {
            format!("{}{}", &run.decoded[..matched.first_offset], replacement)
        } else if index == last_index {
            run.decoded[matched.last_offset..].to_string()
        } else {
            String::new()
        };
        if text.is_empty() {
            emptied_runs += 1;
        }
        let opening = &document_xml[run.text_start..run.text_open_end];
        let opening = docx_text_opening_for_value(opening, text.as_str())?;
        replacements.push((
            run.text_start,
            run.text_close_end,
            format!("{opening}{}</w:t>", escape_xml(text.as_str())),
        ));
    }
    let mut output = document_xml.to_string();
    for (start, end, replacement) in replacements.into_iter().rev() {
        output.replace_range(start..end, replacement.as_str());
    }
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!("updated DOCX XML exceeds the local size limit"));
    }
    Ok((output, matched.runs.len(), emptied_runs))
}

pub(super) fn docx_text_opening_for_value(opening: &str, value: &str) -> Result<String> {
    let needs_preserve = value.chars().next().is_some_and(char::is_whitespace)
        || value.chars().next_back().is_some_and(char::is_whitespace);
    match (opening, needs_preserve) {
        ("<w:t>", true) => Ok("<w:t xml:space=\"preserve\">".to_string()),
        ("<w:t>", false) | ("<w:t xml:space=\"preserve\">", _) => Ok(opening.to_string()),
        _ => Err(anyhow!(
            "DOCX cross-run replacement supports only standard w:t opening tags"
        )),
    }
}

pub(super) fn find_text_tag(document_xml: &str, mut cursor: usize) -> Option<usize> {
    while let Some(offset) = document_xml[cursor..].find("<w:t") {
        let start = cursor + offset;
        let suffix = document_xml.as_bytes().get(start + 4).copied();
        if suffix.is_some_and(|byte| byte == b'>' || byte.is_ascii_whitespace()) {
            return Some(start);
        }
        cursor = start + 4;
    }
    None
}
