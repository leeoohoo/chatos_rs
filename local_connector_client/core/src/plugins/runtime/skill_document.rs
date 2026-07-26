// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use anyhow::{bail, Result};

use super::skill_loader::PluginSkillMetadata;

pub(super) struct ParsedSkillDocument {
    pub(super) metadata: PluginSkillMetadata,
    pub(super) references: Vec<String>,
}

pub(super) fn parse_skill_document(raw: &str, fallback_name: &str) -> Result<ParsedSkillDocument> {
    let metadata = parse_frontmatter(raw, fallback_name)?;
    Ok(ParsedSkillDocument {
        metadata,
        references: extract_references(raw),
    })
}

pub(super) fn extract_references(raw: &str) -> Vec<String> {
    let mut references = BTreeSet::new();
    extract_markdown_links(raw, &mut references);
    extract_inline_code_paths(raw, &mut references);
    references.into_iter().collect()
}

pub(super) fn resolve_reference_path(current_file: &str, target: &str) -> Result<Option<String>> {
    let target = target.trim();
    if target.is_empty() || target.starts_with('#') {
        return Ok(None);
    }
    let target = target
        .split_once('#')
        .map(|(path, _)| path)
        .unwrap_or(target)
        .trim();
    if target.is_empty() {
        return Ok(None);
    }
    let lower = target.to_ascii_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("data:")
    {
        return Ok(None);
    }
    if target.contains('\0')
        || target.contains('\\')
        || target.starts_with('/')
        || target.starts_with('~')
        || target.contains("://")
        || has_windows_drive_prefix(target)
    {
        bail!("Plugin Skill reference is not a safe relative path: {target}");
    }

    let mut segments = current_file
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.pop().is_none() {
        bail!("Plugin Skill reference base path is invalid");
    }
    for segment in target.trim_start_matches("./").split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    bail!("Plugin Skill reference escapes the Plugin installation");
                }
            }
            _ if segment.chars().any(char::is_control) => {
                bail!("Plugin Skill reference contains control characters")
            }
            _ => segments.push(segment),
        }
    }
    if segments.is_empty() || segments.len() > 32 {
        bail!("Plugin Skill reference path is empty or too deep");
    }
    let normalized = segments.join("/");
    if normalized.len() > 512 {
        bail!("Plugin Skill reference path exceeds 512 bytes");
    }
    let root = segments[0];
    if !matches!(
        root,
        "skills" | "references" | "scripts" | "assets" | "schemas" | "binaries" | "licenses"
    ) {
        bail!("Plugin Skill reference points outside an allowed Plugin content root");
    }
    Ok(Some(normalized))
}

fn parse_frontmatter(raw: &str, fallback_name: &str) -> Result<PluginSkillMetadata> {
    let mut metadata = PluginSkillMetadata {
        name: fallback_name.to_string(),
        description: None,
        disable_model_invocation: false,
    };
    let normalized = raw.replace("\r\n", "\n");
    let Some(rest) = normalized.strip_prefix("---\n") else {
        validate_metadata(&metadata)?;
        return Ok(metadata);
    };
    let Some((frontmatter, _)) = rest.split_once("\n---\n") else {
        bail!("Plugin Skill YAML frontmatter is not terminated");
    };
    for line in frontmatter.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = parse_scalar(value.trim());
        match key.trim() {
            "name" if !value.is_empty() => metadata.name = value,
            "description" if !value.is_empty() => metadata.description = Some(value),
            "disable-model-invocation" => {
                metadata.disable_model_invocation = match value.as_str() {
                    "true" => true,
                    "false" | "" => false,
                    _ => bail!("Plugin Skill disable-model-invocation must be true or false"),
                }
            }
            _ => {}
        }
    }
    validate_metadata(&metadata)?;
    Ok(metadata)
}

fn validate_metadata(metadata: &PluginSkillMetadata) -> Result<()> {
    if metadata.name.len() > 128
        || !metadata.name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        bail!("Plugin Skill name must be lower-case ASCII and at most 128 bytes");
    }
    if metadata
        .description
        .as_ref()
        .is_some_and(|description| description.len() > 4096)
    {
        bail!("Plugin Skill description exceeds 4096 bytes");
    }
    Ok(())
}

fn parse_scalar(value: &str) -> String {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        return serde_json::from_str::<String>(value)
            .unwrap_or_else(|_| value[1..value.len() - 1].to_string());
    }
    if value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'') {
        return value[1..value.len() - 1].replace("''", "'");
    }
    value.to_string()
}

fn extract_markdown_links(raw: &str, references: &mut BTreeSet<String>) {
    let mut remaining = raw;
    while let Some(start) = remaining.find("](") {
        remaining = &remaining[start + 2..];
        let Some(end) = remaining.find(')') else {
            break;
        };
        let inside = remaining[..end].trim();
        let target = if let Some(inside) = inside.strip_prefix('<') {
            inside.split_once('>').map(|(target, _)| target)
        } else {
            inside.split_whitespace().next()
        };
        if let Some(target) = target.and_then(clean_candidate) {
            references.insert(target);
        }
        remaining = &remaining[end + 1..];
    }
}

fn extract_inline_code_paths(raw: &str, references: &mut BTreeSet<String>) {
    let mut remaining = raw;
    while let Some(start) = remaining.find('`') {
        remaining = &remaining[start + 1..];
        if remaining.starts_with("``") {
            remaining = &remaining[2..];
            continue;
        }
        let Some(end) = remaining.find('`') else {
            break;
        };
        for token in remaining[..end].split_whitespace() {
            if let Some(candidate) = clean_candidate(token) {
                if looks_like_local_resource(candidate.as_str()) {
                    references.insert(candidate);
                }
            }
        }
        remaining = &remaining[end + 1..];
    }
}

fn clean_candidate(value: &str) -> Option<String> {
    let value = value.trim_matches(|character: char| {
        matches!(
            character,
            '<' | '>' | '"' | '\'' | ',' | ';' | ':' | '(' | ')'
        )
    });
    (!value.is_empty()).then(|| value.to_string())
}

fn looks_like_local_resource(value: &str) -> bool {
    let value = value.trim_start_matches("./");
    value.starts_with("../")
        || [
            "skills/",
            "references/",
            "scripts/",
            "assets/",
            "schemas/",
            "binaries/",
            "licenses/",
        ]
        .iter()
        .any(|prefix| value.starts_with(prefix))
}

fn has_windows_drive_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}
