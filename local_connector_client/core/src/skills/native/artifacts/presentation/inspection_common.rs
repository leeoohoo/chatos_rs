// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;

use super::{find_next_pptx_xml_tag_start, required_xml_attribute, unescape_xml};

pub(super) fn slide_part_number(name: &str) -> Option<usize> {
    name.strip_prefix("ppt/slides/slide")?
        .strip_suffix(".xml")?
        .parse()
        .ok()
}

pub(super) fn presentation_slide_size(xml: &str) -> Result<(u64, u64)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    loop {
        match reader.read_event().context("parse PPTX presentation XML")? {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"sldSz" =>
            {
                let cx = required_xml_attribute(&reader, &event, "cx")?.parse()?;
                let cy = required_xml_attribute(&reader, &event, "cy")?.parse()?;
                return Ok((cx, cy));
            }
            Event::Eof => return Err(anyhow!("PPTX presentation is missing slide size")),
            _ => {}
        }
    }
}

pub(super) fn drawing_text_runs(xml: &str, max_chars: usize) -> Result<Vec<String>> {
    let mut runs = Vec::new();
    let mut cursor = 0usize;
    let mut chars = 0usize;
    while let Some(start) = find_next_pptx_xml_tag_start(xml, "<a:t", cursor) {
        let Some(open_end) = xml[start..].find('>') else {
            return Err(anyhow!("PPTX text run has an invalid opening tag"));
        };
        let content_start = start + open_end + 1;
        let Some(end) = xml[content_start..].find("</a:t>") else {
            return Err(anyhow!("PPTX text run has an invalid closing tag"));
        };
        let value = unescape_xml(&xml[content_start..content_start + end]);
        chars = chars.saturating_add(value.chars().count());
        if chars > max_chars {
            runs.push(
                value
                    .chars()
                    .take(max_chars.saturating_sub(chars - value.chars().count()))
                    .collect(),
            );
            break;
        }
        runs.push(value);
        cursor = content_start + end + "</a:t>".len();
    }
    Ok(runs)
}
