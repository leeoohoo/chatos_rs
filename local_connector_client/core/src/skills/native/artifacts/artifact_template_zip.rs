// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use tempfile::NamedTempFile;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::artifact_template_model::{TemplatePlaceholder, MAX_TEMPLATE_ZIP_ENTRIES};
use super::{MAX_ARTIFACT_BYTES, MAX_XML_BYTES};

pub(super) fn template_placeholder_counts(
    path: &Path,
    kind: &str,
    placeholders: &[TemplatePlaceholder],
) -> Result<BTreeMap<String, usize>> {
    let mut archive = ZipArchive::new(File::open(path)?)
        .with_context(|| format!("open template artifact {}", path.display()))?;
    if archive.is_empty() || archive.len() > MAX_TEMPLATE_ZIP_ENTRIES {
        return Err(anyhow!(
            "template artifact ZIP entry count is outside the safety limit"
        ));
    }
    let mut names = HashSet::new();
    let mut expanded = 0u64;
    let mut counts = placeholders
        .iter()
        .map(|placeholder| (placeholder.name.clone(), 0usize))
        .collect::<BTreeMap<_, _>>();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if entry.enclosed_name().is_none() || entry.is_symlink() || !names.insert(name.clone()) {
            return Err(anyhow!(
                "template artifact ZIP contains an unsafe or duplicate entry"
            ));
        }
        expanded = expanded.saturating_add(entry.size());
        if expanded > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "template artifact exceeds the 100 MiB expanded safety limit"
            ));
        }
        let Some(tag) = template_xml_text_tag(kind, name.as_str()) else {
            continue;
        };
        if entry.size() as usize > MAX_XML_BYTES {
            return Err(anyhow!(
                "template XML entry exceeds the 16 MiB safety limit"
            ));
        }
        let mut xml = String::new();
        entry
            .read_to_string(&mut xml)
            .with_context(|| format!("read template XML part {name}"))?;
        for content in template_xml_text_contents(xml.as_str(), tag)? {
            for placeholder in placeholders {
                let count = content.matches(placeholder.token.as_str()).count();
                if count > 0 {
                    let current = counts.get_mut(&placeholder.name).ok_or_else(|| {
                        anyhow!("template placeholder count state is inconsistent")
                    })?;
                    *current = current.saturating_add(count);
                }
            }
        }
    }
    Ok(counts)
}

fn template_xml_text_tag(kind: &str, name: &str) -> Option<&'static str> {
    match kind {
        "docx"
            if name == "word/document.xml"
                || name.starts_with("word/header") && name.ends_with(".xml")
                || name.starts_with("word/footer") && name.ends_with(".xml") =>
        {
            Some("w:t")
        }
        "pptx"
            if name.starts_with("ppt/slides/slide") && name.ends_with(".xml")
                || name.starts_with("ppt/notesSlides/notesSlide") && name.ends_with(".xml") =>
        {
            Some("a:t")
        }
        "xlsx"
            if name == "xl/sharedStrings.xml"
                || name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml") =>
        {
            Some("t")
        }
        _ => None,
    }
}

fn template_xml_text_contents<'a>(xml: &'a str, tag: &str) -> Result<Vec<&'a str>> {
    let mut output = Vec::new();
    let mut cursor = 0usize;
    while let Some((start, end, next)) = next_template_text_range(xml, tag, cursor)? {
        let content = &xml[start..end];
        if content.contains('<') {
            return Err(anyhow!(
                "template text run contains unsupported nested XML content"
            ));
        }
        output.push(content);
        cursor = next;
    }
    Ok(output)
}

fn next_template_text_range(
    xml: &str,
    tag: &str,
    mut cursor: usize,
) -> Result<Option<(usize, usize, usize)>> {
    let opening = format!("<{tag}");
    let closing = format!("</{tag}>");
    while let Some(offset) = xml[cursor..].find(opening.as_str()) {
        let tag_start = cursor + offset;
        let boundary = xml.as_bytes().get(tag_start + opening.len()).copied();
        if !boundary.is_some_and(|byte| byte == b'>' || byte.is_ascii_whitespace()) {
            cursor = tag_start + opening.len();
            continue;
        }
        let open_end = xml[tag_start..]
            .find('>')
            .map(|offset| tag_start + offset)
            .ok_or_else(|| anyhow!("template XML text run has an invalid opening tag"))?;
        let content_start = open_end + 1;
        let content_end = xml[content_start..]
            .find(closing.as_str())
            .map(|offset| content_start + offset)
            .ok_or_else(|| anyhow!("template XML text run has an invalid closing tag"))?;
        return Ok(Some((
            content_start,
            content_end,
            content_end + closing.len(),
        )));
    }
    Ok(None)
}

pub(super) fn instantiate_semantic_template(
    source: &Path,
    target: &Path,
    kind: &str,
    values: &BTreeMap<String, String>,
    overwrite: bool,
) -> Result<usize> {
    if target.exists() && !overwrite {
        return Err(anyhow!(
            "refusing to overwrite existing artifact without overwrite=true"
        ));
    }
    let mut archive = ZipArchive::new(File::open(source)?)
        .with_context(|| format!("open template artifact {}", source.display()))?;
    let mut replacements = BTreeMap::<String, Vec<u8>>::new();
    let escaped = values
        .iter()
        .map(|(name, value)| {
            (
                format!("{{{{{name}}}}}"),
                (name.as_str(), escape_template_xml(value)),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut total_replacements = 0usize;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        let Some(tag) = template_xml_text_tag(kind, name.as_str()) else {
            continue;
        };
        if entry.size() as usize > MAX_XML_BYTES {
            return Err(anyhow!(
                "template XML entry exceeds the 16 MiB safety limit"
            ));
        }
        let mut xml = String::new();
        entry.read_to_string(&mut xml)?;
        let (updated, count) = replace_template_xml(xml.as_str(), tag, &escaped)?;
        if count > 0 {
            total_replacements = total_replacements.saturating_add(count);
            replacements.insert(name, updated.into_bytes());
        }
    }
    drop(archive);
    rewrite_template_zip(source, target, &replacements, overwrite)?;
    Ok(total_replacements)
}

fn replace_template_xml(
    xml: &str,
    tag: &str,
    values: &BTreeMap<String, (&str, String)>,
) -> Result<(String, usize)> {
    let mut output = String::with_capacity(xml.len());
    let mut cursor = 0usize;
    let mut replacements = 0usize;
    while let Some((start, end, next)) = next_template_text_range(xml, tag, cursor)? {
        output.push_str(&xml[cursor..start]);
        let content = &xml[start..end];
        if content.contains('<') {
            return Err(anyhow!(
                "template text run contains unsupported nested XML content"
            ));
        }
        let (updated, count) = replace_template_text(content, values);
        output.push_str(updated.as_str());
        replacements = replacements.saturating_add(count);
        output.push_str(&xml[end..next]);
        cursor = next;
    }
    output.push_str(&xml[cursor..]);
    if output.len() > MAX_XML_BYTES {
        return Err(anyhow!(
            "instantiated template XML exceeds the 16 MiB safety limit"
        ));
    }
    Ok((output, replacements))
}

fn replace_template_text(text: &str, values: &BTreeMap<String, (&str, String)>) -> (String, usize) {
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut replacements = 0usize;
    while let Some(start_offset) = text[cursor..].find("{{") {
        let start = cursor + start_offset;
        output.push_str(&text[cursor..start]);
        let Some(end_offset) = text[start + 2..].find("}}") else {
            output.push_str(&text[start..]);
            return (output, replacements);
        };
        let end = start + 2 + end_offset + 2;
        let token = &text[start..end];
        if let Some((_, value)) = values.get(token) {
            output.push_str(value.as_str());
            replacements = replacements.saturating_add(1);
        } else {
            output.push_str(token);
        }
        cursor = end;
    }
    output.push_str(&text[cursor..]);
    (output, replacements)
}

fn rewrite_template_zip(
    source: &Path,
    target: &Path,
    replacements: &BTreeMap<String, Vec<u8>>,
    overwrite: bool,
) -> Result<u64> {
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("template output path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = NamedTempFile::new_in(parent)?;
    let mut archive = ZipArchive::new(File::open(source)?)?;
    let mut writer = ZipWriter::new(temporary);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut names = HashSet::new();
    let mut expanded = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if entry.enclosed_name().is_none() || entry.is_symlink() || !names.insert(name.clone()) {
            return Err(anyhow!(
                "template ZIP contains an unsafe or duplicate entry"
            ));
        }
        expanded = expanded.saturating_add(
            replacements
                .get(name.as_str())
                .map_or(entry.size(), |content| content.len() as u64),
        );
        if expanded > MAX_ARTIFACT_BYTES {
            return Err(anyhow!(
                "instantiated template exceeds the 100 MiB safety limit"
            ));
        }
        if let Some(content) = replacements.get(name.as_str()) {
            writer.start_file(name.as_str(), options)?;
            writer.write_all(content)?;
        } else if entry.is_dir() {
            writer.add_directory(name.as_str(), entry.options())?;
        } else {
            writer.raw_copy_file(entry)?;
        }
    }
    let temporary = writer.finish()?;
    temporary.as_file().sync_all()?;
    let bytes = temporary.as_file().metadata()?.len();
    if bytes > MAX_ARTIFACT_BYTES {
        return Err(anyhow!(
            "instantiated template exceeds the 100 MiB safety limit"
        ));
    }
    if target.exists() {
        if !overwrite {
            return Err(anyhow!(
                "refusing to overwrite existing artifact without overwrite=true"
            ));
        }
        fs::remove_file(target)?;
    }
    temporary.persist(target).map_err(|error| error.error)?;
    Ok(bytes)
}

pub(super) fn ensure_distinct_template_output(source: &Path, target: &Path) -> Result<()> {
    let source_metadata = fs::symlink_metadata(source)?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_file() {
        return Err(anyhow!(
            "template artifact must be a regular non-symlink file"
        ));
    }
    if source == target {
        return Err(anyhow!(
            "template instantiation requires a distinct target_path"
        ));
    }
    if target.exists() {
        let metadata = fs::symlink_metadata(target)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(anyhow!(
                "template output target is not a regular non-symlink file"
            ));
        }
        if source.canonicalize()? == target.canonicalize()? {
            return Err(anyhow!(
                "template instantiation requires a distinct target_path"
            ));
        }
    }
    Ok(())
}

fn escape_template_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
