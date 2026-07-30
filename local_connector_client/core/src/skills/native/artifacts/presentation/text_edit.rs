// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use quick_xml::escape::unescape;
use serde_json::Value;

use super::super::MAX_XML_BYTES;
use super::limits::{
    MAX_PPTX_CROSS_RUNS, MAX_PPTX_PARAGRAPHS_PER_SLIDE, MAX_PPTX_RUNS_PER_PARAGRAPH,
    MAX_SLIDE_TEXT_CHARS,
};
use super::model::{
    PptxCrossRunScan, PptxCrossRunTextMatch, PptxXmlElementRange, SimplePptxTextRun,
};
use super::text_validation::validate_slide_text;
use super::{
    escape_xml, find_next_pptx_xml_tag_start, pptx_xml_element_ranges,
    pptx_xml_open_element_stack_at,
};

pub(super) fn parse_pptx_text_replacement_input<'a>(
    arguments: &'a Value,
    subject: &str,
) -> Result<(&'a str, &'a str, usize)> {
    let find = super::required_text(arguments, "find")?;
    let replacement = arguments
        .get("replacement")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("replacement must be a string"))?;
    if find.is_empty() || find.chars().count() > 10_000 {
        return Err(anyhow!("find must contain between 1 and 10000 characters"));
    }
    validate_slide_text(find, "find", 10_000)?;
    validate_slide_text(replacement, "replacement", MAX_SLIDE_TEXT_CHARS)?;
    if find == replacement {
        return Err(anyhow!(
            "{subject} replacement must change the matched text"
        ));
    }
    let max_replacements = match arguments.get("max_replacements") {
        None => 100usize,
        Some(Value::Number(value)) => value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| (1..=10_000).contains(value))
            .ok_or_else(|| anyhow!("max_replacements must be an integer between 1 and 10000"))?,
        Some(_) => {
            return Err(anyhow!(
                "max_replacements must be an integer between 1 and 10000"
            ));
        }
    };
    Ok((find, replacement, max_replacements))
}

pub(super) fn scan_pptx_cross_run_text(xml: &str, selection: &str) -> Result<PptxCrossRunScan> {
    if xml.contains("<!--") || xml.contains("<![CDATA[") || xml.contains("<!DOCTYPE") {
        return Err(anyhow!(
            "PPTX cross-run replacement does not support comments, CDATA, or DTD markup"
        ));
    }
    let paragraphs = pptx_xml_element_ranges(
        xml,
        "<a:p",
        "</a:p>",
        MAX_PPTX_PARAGRAPHS_PER_SLIDE,
        "PPTX paragraphs",
    )?;
    let mut occurrences = 0usize;
    let mut matched = None::<PptxCrossRunTextMatch>;
    let mut unsupported_reason = None::<String>;
    for paragraph in paragraphs {
        let paragraph_xml = &xml[paragraph.start..paragraph.end];
        let visible_text = pptx_visible_text(paragraph_xml)?;
        for start in overlapping_pptx_text_match_starts(visible_text.as_str(), selection) {
            occurrences = occurrences.saturating_add(1);
            let candidate = pptx_cross_run_match_in_paragraph(
                xml,
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
    Ok(PptxCrossRunScan {
        occurrences,
        matched: (occurrences == 1).then_some(matched).flatten(),
        unsupported_reason,
    })
}

fn pptx_cross_run_match_in_paragraph(
    slide_xml: &str,
    paragraph: PptxXmlElementRange,
    visible_text: &str,
    selection_start: usize,
    selection_end: usize,
) -> Result<PptxCrossRunTextMatch> {
    let paragraph_xml = &slide_xml[paragraph.start..paragraph.end];
    if pptx_paragraph_has_unsupported_cross_run_content(paragraph_xml) {
        return Err(anyhow!(
            "paragraph contains a field, line break, hyperlink, extension, or other unsupported DrawingML content"
        ));
    }
    let runs = simple_pptx_text_runs(slide_xml, paragraph)?;
    let combined = runs
        .iter()
        .map(|run| run.decoded.as_str())
        .collect::<String>();
    if combined != visible_text {
        return Err(anyhow!(
            "paragraph visible text is not represented by direct simple DrawingML runs"
        ));
    }
    let mut cumulative = 0usize;
    let mut first = None::<(usize, usize)>;
    let mut last = None::<(usize, usize)>;
    for (index, run) in runs.iter().enumerate() {
        let next = cumulative.saturating_add(run.decoded.len());
        if first.is_none() && selection_start >= cumulative && selection_start < next {
            first = Some((index, selection_start - cumulative));
        }
        if selection_end > cumulative && selection_end <= next {
            last = Some((index, selection_end - cumulative));
            break;
        }
        cumulative = next;
    }
    let (first_index, first_offset) = first
        .ok_or_else(|| anyhow!("selection start does not map to a simple DrawingML text run"))?;
    let (last_index, last_offset) =
        last.ok_or_else(|| anyhow!("selection end does not map to a simple DrawingML text run"))?;
    if first_index == last_index {
        return Err(anyhow!(
            "selection is contained inside one run; use replace_pptx_text instead"
        ));
    }
    let touched = last_index - first_index + 1;
    if touched > MAX_PPTX_CROSS_RUNS {
        return Err(anyhow!(
            "selection spans {touched} runs, exceeding the {MAX_PPTX_CROSS_RUNS} run safety limit"
        ));
    }
    let formatting = runs[first_index].formatting.as_str();
    if runs[first_index..=last_index]
        .iter()
        .any(|run| run.formatting != formatting)
    {
        return Err(anyhow!(
            "selection crosses runs with different DrawingML run properties"
        ));
    }
    for pair in runs[first_index..=last_index].windows(2) {
        if !slide_xml[pair[0].run_end..pair[1].run_start]
            .trim()
            .is_empty()
        {
            return Err(anyhow!(
                "selection crosses non-run markup between adjacent DrawingML runs"
            ));
        }
    }
    Ok(PptxCrossRunTextMatch {
        runs: runs[first_index..=last_index].to_vec(),
        first_offset,
        last_offset,
    })
}

pub(super) fn simple_pptx_text_runs(
    slide_xml: &str,
    paragraph: PptxXmlElementRange,
) -> Result<Vec<SimplePptxTextRun>> {
    let paragraph_xml = &slide_xml[paragraph.start..paragraph.end];
    let ranges = pptx_xml_element_ranges(
        paragraph_xml,
        "<a:r",
        "</a:r>",
        MAX_PPTX_RUNS_PER_PARAGRAPH,
        "PPTX paragraph runs",
    )?;
    if ranges.is_empty() {
        return Err(anyhow!("paragraph contains no simple DrawingML runs"));
    }
    let mut runs = Vec::with_capacity(ranges.len());
    for range in ranges {
        let stack = pptx_xml_open_element_stack_at(paragraph_xml, range.start)?;
        if stack.len() != 1 || stack[0] != "a:p" {
            return Err(anyhow!(
                "paragraph contains a DrawingML run that is not a direct child"
            ));
        }
        if range.open_end == range.end {
            return Err(anyhow!("paragraph contains a self-closing DrawingML run"));
        }
        let run_start = paragraph.start + range.start;
        let run_end = paragraph.start + range.end;
        let run_xml = &slide_xml[run_start..run_end];
        let run_opening = &run_xml[..range.open_end - range.start];
        if run_opening != "<a:r>" {
            return Err(anyhow!(
                "PPTX cross-run replacement supports only standard a:r opening tags"
            ));
        }
        let text_ranges =
            pptx_xml_element_ranges(run_xml, "<a:t", "</a:t>", 2, "PPTX run text elements")?;
        if text_ranges.len() != 1 || text_ranges[0].open_end == text_ranges[0].end {
            return Err(anyhow!(
                "paragraph contains a run that is not one simple DrawingML text run"
            ));
        }
        let text = text_ranges[0];
        let text_stack = pptx_xml_open_element_stack_at(run_xml, text.start)?;
        if text_stack.len() != 1 || text_stack[0] != "a:r" {
            return Err(anyhow!("DrawingML text is not a direct child of its run"));
        }
        let text_opening = &run_xml[text.start..text.open_end];
        if !matches!(text_opening, "<a:t>" | "<a:t xml:space=\"preserve\">") {
            return Err(anyhow!(
                "PPTX cross-run replacement supports only standard a:t opening tags"
            ));
        }
        let raw_text = &run_xml[text.open_end..text.close_start];
        if raw_text.contains('<') {
            return Err(anyhow!(
                "PPTX cross-run text contains unsupported nested XML"
            ));
        }
        let prefix = run_xml[range.open_end - range.start..text.start].trim();
        let formatting = simple_pptx_run_properties(prefix)?;
        if !run_xml[text.end..range.close_start - range.start]
            .trim()
            .is_empty()
        {
            return Err(anyhow!(
                "simple DrawingML text run contains content after a:t"
            ));
        }
        let decoded = unescape(raw_text)
            .context("decode PPTX cross-run DrawingML text")?
            .into_owned();
        runs.push(SimplePptxTextRun {
            run_start,
            run_end,
            text_start: run_start + text.start,
            text_open_end: run_start + text.open_end,
            text_close_end: run_start + text.end,
            formatting,
            decoded,
        });
    }
    let all_text = pptx_text_values(paragraph_xml)?;
    if all_text.len() != runs.len()
        || all_text.concat()
            != runs
                .iter()
                .map(|run| run.decoded.as_str())
                .collect::<String>()
    {
        return Err(anyhow!(
            "paragraph contains text outside the direct simple DrawingML runs"
        ));
    }
    Ok(runs)
}

fn simple_pptx_run_properties(prefix: &str) -> Result<String> {
    if prefix.is_empty() {
        return Ok(String::new());
    }
    let ranges = pptx_xml_element_ranges(prefix, "<a:rPr", "</a:rPr>", 1, "PPTX run properties")?;
    if ranges.len() != 1
        || !prefix[..ranges[0].start].trim().is_empty()
        || !prefix[ranges[0].end..].trim().is_empty()
    {
        return Err(anyhow!(
            "simple DrawingML run contains content other than one run-properties element"
        ));
    }
    Ok(prefix[ranges[0].start..ranges[0].end].to_string())
}

pub(super) fn pptx_visible_text(xml: &str) -> Result<String> {
    Ok(pptx_text_values(xml)?.concat())
}

fn pptx_text_values(xml: &str) -> Result<Vec<String>> {
    let ranges = pptx_xml_element_ranges(
        xml,
        "<a:t",
        "</a:t>",
        MAX_PPTX_RUNS_PER_PARAGRAPH.saturating_mul(2),
        "PPTX text elements",
    )?;
    let mut values = Vec::with_capacity(ranges.len());
    let mut characters = 0usize;
    for range in ranges {
        if range.open_end == range.end {
            values.push(String::new());
            continue;
        }
        let raw = &xml[range.open_end..range.close_start];
        if raw.contains('<') {
            return Err(anyhow!(
                "PPTX DrawingML text contains unsupported nested XML"
            ));
        }
        let decoded = unescape(raw)
            .context("decode PPTX DrawingML paragraph text")?
            .into_owned();
        characters = characters.saturating_add(decoded.chars().count());
        if characters > MAX_SLIDE_TEXT_CHARS {
            return Err(anyhow!(
                "PPTX paragraph text exceeds the {MAX_SLIDE_TEXT_CHARS} character safety limit"
            ));
        }
        values.push(decoded);
    }
    Ok(values)
}

fn overlapping_pptx_text_match_starts(text: &str, selection: &str) -> Vec<usize> {
    text.char_indices()
        .filter_map(|(index, _)| text[index..].starts_with(selection).then_some(index))
        .collect()
}

pub(super) fn pptx_paragraph_has_unsupported_cross_run_content(paragraph_xml: &str) -> bool {
    [
        "<a:fld",
        "<a:br",
        "<a:tab",
        "<a:hlinkClick",
        "<a:hlinkMouseOver",
        "<a:extLst",
    ]
    .iter()
    .any(|marker| find_next_pptx_xml_tag_start(paragraph_xml, marker, 0).is_some())
}

pub(super) fn rewrite_pptx_cross_run_match(
    slide_xml: &str,
    matched: &PptxCrossRunTextMatch,
    replacement: &str,
) -> Result<(String, usize, usize)> {
    let mut replacements = Vec::<(usize, usize, String)>::with_capacity(matched.runs.len());
    let mut emptied_runs = 0usize;
    let last_index = matched
        .runs
        .len()
        .checked_sub(1)
        .ok_or_else(|| anyhow!("PPTX cross-run match contains no text runs"))?;
    for (index, run) in matched.runs.iter().enumerate() {
        let text = if index == 0 {
            format!("{}{}", &run.decoded[..matched.first_offset], replacement)
        } else if index == last_index {
            run.decoded[matched.last_offset..].to_string()
        } else {
            String::new()
        };
        if text.is_empty() {
            emptied_runs = emptied_runs.saturating_add(1);
        }
        let opening = &slide_xml[run.text_start..run.text_open_end];
        let opening = pptx_text_opening_for_value(opening, text.as_str())?;
        replacements.push((
            run.text_start,
            run.text_close_end,
            format!("{opening}{}</a:t>", escape_xml(text.as_str())),
        ));
    }
    let mut output = slide_xml.to_string();
    for (start, end, replacement) in replacements.into_iter().rev() {
        output.replace_range(start..end, replacement.as_str());
    }
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!(
            "updated PPTX slide XML exceeds the local XML size limit"
        ));
    }
    Ok((output, matched.runs.len(), emptied_runs))
}

pub(super) fn pptx_text_opening_for_value(opening: &str, value: &str) -> Result<String> {
    let needs_preserve = value.chars().next().is_some_and(char::is_whitespace)
        || value.chars().next_back().is_some_and(char::is_whitespace);
    match (opening, needs_preserve) {
        ("<a:t>", true) => Ok("<a:t xml:space=\"preserve\">".to_string()),
        ("<a:t>", false) | ("<a:t xml:space=\"preserve\">", _) => Ok(opening.to_string()),
        _ => Err(anyhow!(
            "PPTX cross-run replacement supports only standard a:t opening tags"
        )),
    }
}
