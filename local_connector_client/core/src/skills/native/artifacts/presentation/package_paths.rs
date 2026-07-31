// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::path::{Component, Path};

use anyhow::{anyhow, Result};

pub(super) fn next_relationship_id(used: &mut HashSet<String>) -> Result<String> {
    for number in 1..=100_000usize {
        let candidate = format!("rId{number}");
        if used.insert(candidate.clone()) {
            return Ok(candidate);
        }
    }
    Err(anyhow!(
        "PPTX relationship id space exceeds the conservative safety limit"
    ))
}

pub(super) fn numbered_part(name: &str, prefix: &str, suffix: &str) -> Option<usize> {
    name.strip_prefix(prefix)?
        .strip_suffix(suffix)?
        .parse()
        .ok()
}

pub(super) fn relationships_part_path(part: &str) -> Result<String> {
    let path = Path::new(part);
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("PPTX part has no relationship parent"))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("PPTX part name is not valid UTF-8"))?;
    let parent = parent
        .to_str()
        .ok_or_else(|| anyhow!("PPTX part parent is not valid UTF-8"))?;
    Ok(format!("{parent}/_rels/{file_name}.rels"))
}

pub(super) fn relative_part_target(source_part: &str, target_part: &str) -> Result<String> {
    let source_parent = Path::new(source_part)
        .parent()
        .ok_or_else(|| anyhow!("PPTX source part has no parent"))?;
    let source = normal_part_components(source_parent)?;
    let target = normal_part_components(Path::new(target_part))?;
    let common = source
        .iter()
        .zip(target.iter())
        .take_while(|(left, right)| left == right)
        .count();
    let mut output = Vec::new();
    output.extend(std::iter::repeat_n("..".to_string(), source.len() - common));
    output.extend(target.into_iter().skip(common));
    if output.is_empty() {
        return Err(anyhow!("PPTX relationship target cannot be empty"));
    }
    Ok(output.join("/"))
}

pub(super) fn resolve_part_target(source_part: &str, target: &str) -> Result<String> {
    if target.is_empty() || target.starts_with('/') || target.contains(['\\', '\0']) {
        return Err(anyhow!("PPTX relationship target is invalid"));
    }
    let mut parts = Path::new(source_part)
        .parent()
        .ok_or_else(|| anyhow!("PPTX relationship source has no parent"))?
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str().map(str::to_string),
            _ => None,
        })
        .collect::<Vec<_>>();
    for component in Path::new(target).components() {
        match component {
            Component::Normal(value) => parts.push(
                value
                    .to_str()
                    .ok_or_else(|| anyhow!("PPTX relationship target is not UTF-8"))?
                    .to_string(),
            ),
            Component::ParentDir => {
                if parts.pop().is_none() {
                    return Err(anyhow!("PPTX relationship target escapes the package"));
                }
            }
            Component::CurDir => {}
            _ => return Err(anyhow!("PPTX relationship target escapes the package")),
        }
    }
    if parts.is_empty() {
        return Err(anyhow!("PPTX relationship target is empty"));
    }
    Ok(parts.join("/"))
}

fn normal_part_components(path: &Path) -> Result<Vec<String>> {
    path.components()
        .map(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .map(str::to_string)
                .ok_or_else(|| anyhow!("PPTX part path is not valid UTF-8")),
            _ => Err(anyhow!("PPTX part path contains unsafe components")),
        })
        .collect()
}

pub(super) fn sibling_qualified_name(existing: &str, local_name: &str) -> String {
    existing.rsplit_once(':').map_or_else(
        || local_name.to_string(),
        |(prefix, _)| format!("{prefix}:{local_name}"),
    )
}
