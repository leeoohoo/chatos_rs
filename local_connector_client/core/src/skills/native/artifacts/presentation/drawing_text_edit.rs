// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use quick_xml::escape::{resolve_xml_entity, unescape};
use quick_xml::events::{BytesText, Event};
use quick_xml::{Reader, Writer, XmlVersion};

use super::package_edit::xml_output;

pub(super) fn replace_drawing_text_runs(
    xml: &str,
    find: &str,
    replacement: &str,
    max_replacements: usize,
) -> Result<(String, usize, bool)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(xml.len()));
    let mut in_drawing_text = false;
    let mut drawing_text_value = String::new();
    let mut drawing_text_events = Vec::<Event<'static>>::new();
    let mut replacements = 0usize;
    let mut limit_reached = false;
    loop {
        let event = reader.read_event().context("rewrite PPTX DrawingML text")?;
        match event {
            Event::Start(start) if start.name().as_ref() == b"a:t" => {
                if in_drawing_text {
                    return Err(anyhow!("PPTX contains nested DrawingML text elements"));
                }
                in_drawing_text = true;
                drawing_text_value.clear();
                drawing_text_events.clear();
                writer.write_event(Event::Start(start.into_owned()))?;
            }
            Event::End(end) if end.name().as_ref() == b"a:t" => {
                if !in_drawing_text {
                    return Err(anyhow!("PPTX contains an unmatched DrawingML text end tag"));
                }
                let occurrences = drawing_text_value.matches(find).count();
                let allowed = occurrences.min(max_replacements.saturating_sub(replacements));
                if allowed > 0 {
                    let updated = drawing_text_value.replacen(find, replacement, allowed);
                    writer.write_event(Event::Text(BytesText::new(updated.as_str())))?;
                    replacements = replacements.saturating_add(allowed);
                } else {
                    for event in drawing_text_events.drain(..) {
                        writer.write_event(event)?;
                    }
                }
                if occurrences > allowed {
                    limit_reached = true;
                }
                in_drawing_text = false;
                writer.write_event(Event::End(end.into_owned()))?;
            }
            Event::Text(text) if in_drawing_text => {
                let decoded = text
                    .xml_content(XmlVersion::Explicit1_0)
                    .context("decode PPTX DrawingML text")?;
                let value = unescape(decoded.as_ref())
                    .context("unescape PPTX DrawingML text")?
                    .into_owned();
                drawing_text_value.push_str(value.as_str());
                drawing_text_events.push(Event::Text(text.into_owned()));
            }
            Event::GeneralRef(reference) if in_drawing_text => {
                if let Some(character) = reference
                    .resolve_char_ref()
                    .context("resolve PPTX DrawingML character reference")?
                {
                    drawing_text_value.push(character);
                } else {
                    let entity = reference
                        .decode()
                        .context("decode PPTX DrawingML entity reference")?;
                    let value = resolve_xml_entity(entity.as_ref()).ok_or_else(|| {
                        anyhow!("PPTX DrawingML text contains an unsupported entity reference")
                    })?;
                    drawing_text_value.push_str(value);
                }
                drawing_text_events.push(Event::GeneralRef(reference.into_owned()));
            }
            Event::CData(cdata) if in_drawing_text => {
                let value = cdata
                    .xml_content(XmlVersion::Explicit1_0)
                    .context("decode PPTX DrawingML CDATA")?;
                drawing_text_value.push_str(value.as_ref());
                drawing_text_events.push(Event::CData(cdata.into_owned()));
            }
            _event if in_drawing_text => {
                return Err(anyhow!(
                    "PPTX DrawingML text run contains unsupported nested XML content"
                ));
            }
            Event::Eof => {
                if in_drawing_text {
                    return Err(anyhow!("PPTX contains an unclosed DrawingML text element"));
                }
                writer.write_event(Event::Eof)?;
                break;
            }
            event => writer.write_event(event.into_owned())?,
        }
    }
    Ok((
        xml_output(writer, "updated PPTX slide XML")?,
        replacements,
        limit_reached,
    ))
}
