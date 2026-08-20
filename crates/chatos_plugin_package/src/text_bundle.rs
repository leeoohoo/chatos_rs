// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use chatos_plugin_management_sdk::{
    normalized_plugin_manifest_sha256, PluginPortableComponentBundle, PluginPortableTextResource,
    PluginComponentDescriptor, PluginComponentKind, PluginExecutionHost, PluginReleaseRecord,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{PluginPackageError, VerifiedPluginPackage};

const MAX_TEXT_FILE_BYTES: usize = 256 * 1024;
const MAX_COMPONENT_RESOURCES: usize = 128;
const MAX_COMPONENT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Serialize)]
struct BundleHashInput<'a> {
    purpose: &'static str,
    plugin_id: &'a str,
    release_id: &'a str,
    version: &'a str,
    component_key: &'a str,
    kind: PluginComponentKind,
    execution_host: PluginExecutionHost,
    entrypoint: &'a str,
    primary_sha256: &'a str,
    resources: &'a [PluginPortableTextResource],
    artifact_sha256: &'a str,
    normalized_manifest_sha256: &'a str,
}

pub fn build_portable_component_bundles(
    release: &PluginReleaseRecord,
    package: &VerifiedPluginPackage,
    ingested_at: &str,
) -> Result<Vec<PluginPortableComponentBundle>, PluginPackageError> {
    let mut bundles = release
        .components
        .iter()
        .filter(|component| {
            component.execution_host != PluginExecutionHost::Local
                && matches!(
                    component.kind,
                    PluginComponentKind::SkillCollection
                        | PluginComponentKind::Command
                        | PluginComponentKind::Agent
                )
        })
        .map(|component| {
            build_component_text_bundle(
                release.plugin_id.as_str(),
                release.id.as_str(),
                release.version.as_str(),
                release.artifact_sha256.as_str(),
                package,
                component,
                ingested_at,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    bundles.sort_by(|left, right| left.component_key.cmp(&right.component_key));
    Ok(bundles)
}

#[allow(clippy::too_many_arguments)]
pub fn build_component_text_bundle(
    plugin_id: &str,
    release_id: &str,
    version: &str,
    artifact_sha256: &str,
    package: &VerifiedPluginPackage,
    component: &PluginComponentDescriptor,
    ingested_at: &str,
) -> Result<PluginPortableComponentBundle, PluginPackageError> {
    let manifest_sha256 = normalized_plugin_manifest_sha256(&package.manifest)
        .map_err(|error| PluginPackageError::Invalid(error.to_string()))?;
    build_bundle(
        plugin_id,
        release_id,
        version,
        artifact_sha256,
        package,
        component,
        manifest_sha256.as_str(),
        ingested_at,
    )
}

pub fn plugin_portable_bundle_sha256(
    bundle: &PluginPortableComponentBundle,
) -> Result<String, PluginPackageError> {
    if sha256(bundle.primary_text.as_bytes()) != bundle.primary_sha256
        || bundle.resources.iter().any(|resource| {
            sha256(resource.text.as_bytes()) != resource.sha256
                || resource.text.len() as u64 != resource.size_bytes
        })
    {
        return invalid("Plugin portable Bundle text does not match its immutable hashes");
    }
    bundle_hash(
        bundle.plugin_id.as_str(),
        bundle.release_id.as_str(),
        bundle.version.as_str(),
        bundle.component_key.as_str(),
        bundle.kind,
        bundle.execution_host,
        bundle.entrypoint.as_str(),
        bundle.primary_sha256.as_str(),
        bundle.resources.as_slice(),
        bundle.artifact_sha256.as_str(),
        bundle.normalized_manifest_sha256.as_str(),
    )
}

fn build_bundle(
    plugin_id: &str,
    release_id: &str,
    version: &str,
    artifact_sha256: &str,
    package: &VerifiedPluginPackage,
    component: &PluginComponentDescriptor,
    manifest_sha256: &str,
    ingested_at: &str,
) -> Result<PluginPortableComponentBundle, PluginPackageError> {
    if !matches!(
        component.kind,
        PluginComponentKind::SkillCollection
            | PluginComponentKind::Command
            | PluginComponentKind::Agent
    ) {
        return invalid(format!(
            "component {} cannot execute as a portable text component",
            component.component_key
        ));
    }
    if !component.permissions.is_empty() {
        return invalid(format!(
            "portable text component {} requests runtime permissions",
            component.component_key
        ));
    }
    let declared_entrypoint = component
        .entrypoint
        .as_ref()
        .ok_or_else(|| PluginPackageError::Invalid("portable component has no entrypoint".into()))?
        .path
        .trim_start_matches("./")
        .trim_end_matches('/')
        .to_string();
    let entrypoint = if component.kind == PluginComponentKind::SkillCollection {
        let skill_path = if declared_entrypoint.ends_with("/SKILL.md") {
            declared_entrypoint.clone()
        } else {
            format!("{declared_entrypoint}/SKILL.md")
        };
        let prefix = format!("{}/", declared_entrypoint.trim_end_matches("/SKILL.md"));
        let skill_documents = package
            .files
            .keys()
            .filter(|path| path.starts_with(prefix.as_str()) && path.ends_with("/SKILL.md"))
            .count();
        if skill_documents != 1 || !package.files.contains_key(skill_path.as_str()) {
            return invalid(format!(
                "Skill component {} must contain exactly one SKILL.md",
                component.component_key
            ));
        }
        skill_path
    } else {
        declared_entrypoint
    };
    let primary_text = read_primary_text(package, component.kind, entrypoint.as_str())?;
    let primary_sha256 = sha256(primary_text.as_bytes());
    let resources = if component.kind == PluginComponentKind::SkillCollection {
        reachable_skill_resources(package, entrypoint.as_str(), primary_text.as_str())?
    } else {
        Vec::new()
    };
    let total_bytes = primary_text.len()
        + resources
            .iter()
            .map(|resource| resource.text.len())
            .sum::<usize>();
    if resources.len() > MAX_COMPONENT_RESOURCES || total_bytes > MAX_COMPONENT_BYTES {
        return invalid(format!(
            "portable component {} exceeds its text Bundle limits",
            component.component_key
        ));
    }
    let bundle_sha256 = bundle_hash(
        plugin_id,
        release_id,
        version,
        component.component_key.as_str(),
        component.kind,
        component.execution_host,
        entrypoint.as_str(),
        primary_sha256.as_str(),
        resources.as_slice(),
        artifact_sha256,
        manifest_sha256,
    )?;
    Ok(PluginPortableComponentBundle {
        plugin_id: plugin_id.to_string(),
        release_id: release_id.to_string(),
        version: version.to_string(),
        component_key: component.component_key.clone(),
        kind: component.kind,
        execution_host: component.execution_host,
        entrypoint,
        primary_text,
        primary_sha256,
        resources,
        bundle_sha256,
        artifact_sha256: artifact_sha256.to_string(),
        normalized_manifest_sha256: manifest_sha256.to_string(),
        ingested_at: ingested_at.to_string(),
    })
}

fn reachable_skill_resources(
    package: &VerifiedPluginPackage,
    entrypoint: &str,
    primary_text: &str,
) -> Result<Vec<PluginPortableTextResource>, PluginPackageError> {
    let mut visiting = BTreeSet::from([entrypoint.to_string()]);
    let mut visited = BTreeSet::new();
    let mut resources = BTreeMap::new();
    visit_skill_references(
        package,
        entrypoint,
        primary_text,
        &mut visiting,
        &mut visited,
        &mut resources,
    )?;
    Ok(resources.into_values().collect())
}

fn visit_skill_references(
    package: &VerifiedPluginPackage,
    current_path: &str,
    current_text: &str,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
    resources: &mut BTreeMap<String, PluginPortableTextResource>,
) -> Result<(), PluginPackageError> {
    for path in extract_references(current_path, current_text)? {
        if visiting.contains(path.as_str()) {
            return invalid(format!(
                "Plugin Skill reference graph contains a cycle at {path}"
            ));
        }
        if visited.contains(path.as_str()) {
            continue;
        }
        ensure_portable_text_path(path.as_str())?;
        let text = read_text(package, path.as_str())?;
        visiting.insert(path.clone());
        visit_skill_references(
            package,
            path.as_str(),
            text.as_str(),
            visiting,
            visited,
            resources,
        )?;
        visiting.remove(path.as_str());
        visited.insert(path.clone());
        resources.insert(
            path.clone(),
            PluginPortableTextResource {
                path,
                sha256: sha256(text.as_bytes()),
                size_bytes: text.len() as u64,
                text,
            },
        );
        if resources.len() > MAX_COMPONENT_RESOURCES {
            return invalid("Plugin Skill reference graph contains too many resources");
        }
    }
    Ok(())
}

fn extract_references(
    current_file: &str,
    text: &str,
) -> Result<BTreeSet<String>, PluginPackageError> {
    let mut references = BTreeSet::new();
    let mut remaining = text;
    while let Some(start) = remaining.find("](") {
        remaining = &remaining[start + 2..];
        let Some(end) = remaining.find(')') else {
            break;
        };
        let raw = remaining[..end]
            .trim()
            .trim_start_matches('<')
            .split(['>', ' ', '\t'])
            .next()
            .unwrap_or_default();
        if let Some(path) = resolve_reference(current_file, raw)? {
            references.insert(path);
        }
        remaining = &remaining[end + 1..];
    }
    Ok(references)
}

fn resolve_reference(current_file: &str, raw: &str) -> Result<Option<String>, PluginPackageError> {
    let target = raw.split('#').next().unwrap_or_default().trim();
    if target.is_empty()
        || target.starts_with('#')
        || target.starts_with("https://")
        || target.starts_with("http://")
        || target.starts_with("mailto:")
    {
        return Ok(None);
    }
    if target.starts_with('/')
        || target.starts_with('~')
        || target.contains(['\\', '\0'])
        || target.contains("://")
    {
        return invalid(format!("unsafe Plugin Skill reference: {target}"));
    }
    let mut parts = current_file.split('/').collect::<Vec<_>>();
    parts.pop();
    for part in target.trim_start_matches("./").split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return invalid("Plugin Skill reference escapes the package");
                }
            }
            value => parts.push(value),
        }
    }
    let path = parts.join("/");
    ensure_portable_text_path(path.as_str())?;
    Ok(Some(path))
}

fn ensure_portable_text_path(path: &str) -> Result<(), PluginPackageError> {
    let root = path.split('/').next().unwrap_or_default();
    if !matches!(root, "skills" | "references" | "schemas" | "licenses") {
        return invalid(format!(
            "portable Plugin text reference uses a forbidden root: {path}"
        ));
    }
    let extension = path.rsplit_once('.').map(|(_, extension)| extension);
    if !matches!(
        extension,
        Some("md" | "txt" | "json" | "yaml" | "yml" | "toml" | "csv")
    ) {
        return invalid(format!("portable Plugin resource is not text: {path}"));
    }
    Ok(())
}

fn read_primary_text(
    package: &VerifiedPluginPackage,
    kind: PluginComponentKind,
    path: &str,
) -> Result<String, PluginPackageError> {
    let root = path.split('/').next().unwrap_or_default();
    let expected_root = match kind {
        PluginComponentKind::SkillCollection => "skills",
        PluginComponentKind::Command => "commands",
        PluginComponentKind::Agent => "agents",
        _ => return invalid("unsupported portable Plugin primary text component"),
    };
    if root != expected_root {
        return invalid(format!(
            "portable Plugin primary text must be under {expected_root}/: {path}"
        ));
    }
    let extension = path.rsplit_once('.').map(|(_, extension)| extension);
    if !matches!(extension, Some("md" | "txt")) {
        return invalid(format!(
            "portable Plugin primary text is not Markdown/text: {path}"
        ));
    }
    read_utf8_text(package, path)
}

fn read_text(package: &VerifiedPluginPackage, path: &str) -> Result<String, PluginPackageError> {
    ensure_portable_text_path(path)?;
    read_utf8_text(package, path)
}

fn read_utf8_text(
    package: &VerifiedPluginPackage,
    path: &str,
) -> Result<String, PluginPackageError> {
    let bytes = package.files.get(path).ok_or_else(|| {
        PluginPackageError::Invalid(format!("Plugin text file is missing: {path}"))
    })?;
    if bytes.len() > MAX_TEXT_FILE_BYTES || bytes.contains(&0) {
        return invalid(format!(
            "Plugin text file exceeds limits or contains NUL: {path}"
        ));
    }
    String::from_utf8(bytes.clone())
        .map_err(|_| PluginPackageError::Invalid(format!("Plugin text file is not UTF-8: {path}")))
}

#[allow(clippy::too_many_arguments)]
fn bundle_hash(
    plugin_id: &str,
    release_id: &str,
    version: &str,
    component_key: &str,
    kind: PluginComponentKind,
    execution_host: PluginExecutionHost,
    entrypoint: &str,
    primary_sha256: &str,
    resources: &[PluginPortableTextResource],
    artifact_sha256: &str,
    normalized_manifest_sha256: &str,
) -> Result<String, PluginPackageError> {
    let payload = BundleHashInput {
        purpose: "chatos.plugin.portable-component-bundle.v1",
        plugin_id,
        release_id,
        version,
        component_key,
        kind,
        execution_host,
        entrypoint,
        primary_sha256,
        resources,
        artifact_sha256,
        normalized_manifest_sha256,
    };
    serde_json::to_vec(&payload)
        .map(|bytes| sha256(bytes.as_slice()))
        .map_err(PluginPackageError::Json)
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn invalid<T>(message: impl Into<String>) -> Result<T, PluginPackageError> {
    Err(PluginPackageError::Invalid(message.into()))
}
