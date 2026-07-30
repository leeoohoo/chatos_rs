// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::fs::File;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use zip::ZipArchive;

use super::super::read_zip_text;
use super::{parse_relationship_document, resolve_part_target, validate_pptx_package};

pub(in crate::skills::native::artifacts) fn validate_pptx_for_render(path: &Path) -> Result<()> {
    let package_names = validate_pptx_package(path)?;
    for required in [
        "[Content_Types].xml",
        "_rels/.rels",
        "ppt/presentation.xml",
        "ppt/_rels/presentation.xml.rels",
    ] {
        if !package_names.contains(required) {
            return Err(anyhow!("PPTX rendering requires package part: {required}"));
        }
    }
    for name in &package_names {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with("vbaproject.bin")
            || lower.starts_with("ppt/activex/")
            || lower.starts_with("ppt/controls/")
            || lower.starts_with("ppt/embeddings/")
            || lower.starts_with("ppt/oleobjects/")
            || lower.starts_with("ppt/externallinks/")
            || lower.starts_with("ppt/webextensions/")
            || lower.starts_with("customui/")
        {
            return Err(anyhow!(
                "PPTX rendering rejects active, embedded, or externally connected presentation content: {name}"
            ));
        }
    }

    let mut archive = ZipArchive::new(File::open(path)?)
        .with_context(|| format!("open PPTX {} for render validation", path.display()))?;
    let content_types = read_zip_text(&mut archive, "[Content_Types].xml")?;
    let normalized_content_types = content_types.to_ascii_lowercase();
    if [
        "macroenabled",
        "vbaproject",
        "activex",
        "oleobject",
        "externaldata",
        "externallink",
        "webextension",
        "customui",
    ]
    .iter()
    .any(|token| normalized_content_types.contains(token))
    {
        return Err(anyhow!(
            "PPTX rendering rejects active, embedded, or externally connected content types"
        ));
    }

    let mut names = package_names.into_iter().collect::<Vec<_>>();
    names.sort();
    let names_set = names.iter().map(String::as_str).collect::<HashSet<_>>();
    for name in names.iter().filter(|name| name.ends_with(".rels")) {
        let source_part = relationship_source_part_for_render(name.as_str())?;
        let relationships = read_zip_text(&mut archive, name.as_str())?;
        let relationships = parse_relationship_document(relationships.as_str(), &source_part)?;
        for relationship in relationships.relationships {
            let relationship_type = relationship.relationship_type.to_ascii_lowercase();
            if relationship_type.ends_with("/oleobject")
                || relationship_type.ends_with("/package")
                || relationship_type.ends_with("/activex")
                || relationship_type.ends_with("/control")
                || relationship_type.ends_with("/externallink")
                || relationship_type.ends_with("/externaldata")
                || relationship_type.ends_with("/vbaproject")
                || relationship_type.contains("/webextension")
                || relationship_type.contains("/customui")
                || relationship_type.ends_with("/attachedtemplate")
            {
                return Err(anyhow!(
                    "PPTX rendering rejects active, embedded, or externally connected relationships"
                ));
            }
            if relationship.external {
                if !relationship_type.ends_with("/hyperlink") {
                    return Err(anyhow!(
                        "PPTX rendering rejects external non-hyperlink relationships"
                    ));
                }
                continue;
            }
            let target = resolve_part_target(&source_part, relationship.target.as_str())?;
            if !names_set.contains(target.as_str()) {
                return Err(anyhow!(
                    "PPTX rendering found a missing internal relationship target: {target}"
                ));
            }
        }
    }
    Ok(())
}

fn relationship_source_part_for_render(relationships_path: &str) -> Result<String> {
    if relationships_path == "_rels/.rels" {
        return Ok("package.xml".to_string());
    }
    let (parent, file) = relationships_path
        .split_once("/_rels/")
        .ok_or_else(|| anyhow!("PPTX relationship part path is invalid"))?;
    let source_file = file
        .strip_suffix(".rels")
        .ok_or_else(|| anyhow!("PPTX relationship part suffix is invalid"))?;
    if parent.is_empty() || source_file.is_empty() {
        return Err(anyhow!("PPTX relationship source part is invalid"));
    }
    Ok(format!("{parent}/{source_file}"))
}
