// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};

use super::model::PptxXmlElementRange;
use super::optional_xml_attribute;

pub(super) fn pptx_direct_element_ranges(
    xml: &str,
    opening: &str,
    closing: &str,
    maximum: usize,
    parent: &str,
    label: &str,
) -> Result<Vec<PptxXmlElementRange>> {
    let ranges = pptx_xml_element_ranges(xml, opening, closing, maximum, label)?;
    for range in &ranges {
        let stack = pptx_xml_open_element_stack_at(xml, range.start)?;
        if stack.len() != 1 || stack[0] != parent {
            return Err(anyhow!("{label} must be direct children of {parent}"));
        }
    }
    Ok(ranges)
}

pub(super) fn pptx_direct_child_local_names(xml: &str, expected_root: &str) -> Result<Vec<String>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut depth = 0usize;
    let mut root_seen = false;
    let mut children = Vec::new();
    loop {
        match reader.read_event().context("parse PPTX table XML")? {
            Event::Start(event) => {
                let local = String::from_utf8_lossy(event.local_name().as_ref()).into_owned();
                if depth == 0 {
                    if root_seen || local != expected_root {
                        return Err(anyhow!("PPTX table XML has an unexpected root element"));
                    }
                    root_seen = true;
                } else if depth == 1 {
                    children.push(local);
                }
                depth = depth
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("PPTX table XML nesting exceeds the safety limit"))?;
                if depth > 256 {
                    return Err(anyhow!("PPTX table XML nesting exceeds the safety limit"));
                }
            }
            Event::Empty(event) => {
                let local = String::from_utf8_lossy(event.local_name().as_ref()).into_owned();
                if depth == 0 {
                    if root_seen || local != expected_root {
                        return Err(anyhow!("PPTX table XML has an unexpected root element"));
                    }
                    root_seen = true;
                } else if depth == 1 {
                    children.push(local);
                }
            }
            Event::End(_) => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| anyhow!("PPTX table XML has an unmatched closing tag"))?;
            }
            Event::Text(text) => {
                if depth <= 1
                    && !text
                        .xml_content(XmlVersion::Explicit1_0)
                        .context("decode PPTX table whitespace")?
                        .trim()
                        .is_empty()
                {
                    return Err(anyhow!(
                        "PPTX table XML contains unexpected direct text content"
                    ));
                }
            }
            Event::Decl(_) if depth == 0 && !root_seen => {}
            Event::Comment(_) | Event::CData(_) | Event::DocType(_) | Event::PI(_) => {
                return Err(anyhow!(
                    "simple PPTX table editing does not support comments, CDATA, DTD, or processing instructions"
                ));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !root_seen || depth != 0 {
        return Err(anyhow!("PPTX table XML has invalid element boundaries"));
    }
    Ok(children)
}

pub(super) fn pptx_opening_attribute(
    opening: &str,
    expected_local_name: &str,
    attribute_name: &str,
) -> Result<Option<String>> {
    let mut reader = Reader::from_str(opening);
    reader.config_mut().trim_text(false);
    match reader.read_event().context("parse PPTX opening tag")? {
        Event::Start(event) | Event::Empty(event) => {
            if event.local_name().as_ref() != expected_local_name.as_bytes() {
                return Err(anyhow!("PPTX XML opening tag has an unexpected name"));
            }
            optional_xml_attribute(&reader, &event, attribute_name)
        }
        _ => Err(anyhow!("PPTX XML opening tag is invalid")),
    }
}

pub(super) fn pptx_xml_element_ranges(
    xml: &str,
    opening: &str,
    closing: &str,
    maximum: usize,
    label: &str,
) -> Result<Vec<PptxXmlElementRange>> {
    let mut ranges = Vec::new();
    let mut cursor = 0usize;
    let mut depth = 0usize;
    let mut current = None::<(usize, usize)>;
    loop {
        let next_open = find_next_pptx_xml_tag_start(xml, opening, cursor);
        let next_close = xml[cursor..].find(closing).map(|offset| cursor + offset);
        if next_open.is_none() && next_close.is_none() {
            break;
        }
        if next_open.is_some_and(|open| next_close.is_none_or(|close| open < close)) {
            let open_start =
                next_open.ok_or_else(|| anyhow!("{label} have an invalid opening tag boundary"))?;
            let open_end = pptx_xml_tag_end(xml, open_start, xml.len())?;
            let self_closing = xml[open_start..open_end - 1].trim_end().ends_with('/');
            if self_closing {
                if depth == 0 {
                    ranges.push(PptxXmlElementRange {
                        start: open_start,
                        open_end,
                        close_start: open_end,
                        end: open_end,
                    });
                    if ranges.len() > maximum {
                        return Err(anyhow!("{label} exceed the {maximum} item safety limit"));
                    }
                }
                cursor = open_end;
                continue;
            }
            if depth == 0 {
                current = Some((open_start, open_end));
            }
            depth = depth
                .checked_add(1)
                .ok_or_else(|| anyhow!("{label} nesting exceeds the local safety limit"))?;
            cursor = open_end;
        } else {
            let close_start = next_close
                .ok_or_else(|| anyhow!("{label} have an invalid closing tag boundary"))?;
            if depth == 0 {
                return Err(anyhow!("{label} contain an unmatched closing tag"));
            }
            let close_end = close_start + closing.len();
            depth -= 1;
            if depth == 0 {
                let (start, open_end) = current
                    .take()
                    .ok_or_else(|| anyhow!("{label} have an invalid element boundary"))?;
                ranges.push(PptxXmlElementRange {
                    start,
                    open_end,
                    close_start,
                    end: close_end,
                });
                if ranges.len() > maximum {
                    return Err(anyhow!("{label} exceed the {maximum} item safety limit"));
                }
            }
            cursor = close_end;
        }
    }
    if depth != 0 {
        return Err(anyhow!("{label} contain an unclosed element"));
    }
    Ok(ranges)
}

pub(super) fn find_next_pptx_xml_tag_start(
    xml: &str,
    prefix: &str,
    mut cursor: usize,
) -> Option<usize> {
    while let Some(offset) = xml[cursor..].find(prefix) {
        let index = cursor + offset;
        let suffix = xml.as_bytes().get(index + prefix.len()).copied();
        if suffix.is_some_and(|byte| byte == b'>' || byte == b'/' || byte.is_ascii_whitespace()) {
            return Some(index);
        }
        cursor = index + prefix.len();
    }
    None
}

pub(super) fn pptx_xml_open_element_stack_at(xml: &str, boundary: usize) -> Result<Vec<String>> {
    if boundary > xml.len() || !xml.is_char_boundary(boundary) {
        return Err(anyhow!("PPTX XML boundary is invalid"));
    }
    let mut stack = Vec::<String>::new();
    let mut cursor = 0usize;
    while let Some(offset) = xml[cursor..boundary].find('<') {
        let start = cursor + offset;
        if xml[start..boundary].starts_with("<?") {
            let end = xml[start + 2..boundary]
                .find("?>")
                .map(|offset| start + 2 + offset + 2)
                .ok_or_else(|| anyhow!("PPTX XML processing instruction is unterminated"))?;
            cursor = end;
            continue;
        }
        if xml[start..boundary].starts_with("<!") {
            return Err(anyhow!(
                "PPTX cross-run replacement does not support declarations inside slide XML"
            ));
        }
        let end = pptx_xml_tag_end(xml, start, boundary)?;
        let tag = xml[start + 1..end - 1].trim();
        if let Some(closing) = tag.strip_prefix('/') {
            let name = closing
                .split_ascii_whitespace()
                .next()
                .filter(|name| !name.is_empty())
                .ok_or_else(|| anyhow!("PPTX XML contains an invalid closing tag"))?;
            let opened = stack
                .pop()
                .ok_or_else(|| anyhow!("PPTX XML contains an unmatched closing tag"))?;
            if opened != name {
                return Err(anyhow!("PPTX XML contains mismatched element boundaries"));
            }
        } else {
            let self_closing = tag.trim_end().ends_with('/');
            let name = tag
                .trim_end_matches('/')
                .split_ascii_whitespace()
                .next()
                .filter(|name| !name.is_empty())
                .ok_or_else(|| anyhow!("PPTX XML contains an invalid opening tag"))?;
            if !self_closing {
                stack.push(name.to_string());
                if stack.len() > 256 {
                    return Err(anyhow!("PPTX XML nesting exceeds the safety limit"));
                }
            }
        }
        cursor = end;
    }
    Ok(stack)
}

pub(super) fn pptx_xml_tag_end(xml: &str, start: usize, boundary: usize) -> Result<usize> {
    let mut quote = None::<u8>;
    for (offset, byte) in xml.as_bytes()[start + 1..boundary]
        .iter()
        .copied()
        .enumerate()
    {
        match (quote, byte) {
            (Some(active), current) if active == current => quote = None,
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'>') => return Ok(start + 1 + offset + 1),
            _ => {}
        }
    }
    Err(anyhow!("PPTX XML contains an unterminated tag"))
}
