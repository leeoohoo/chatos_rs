// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use chatos_plugin_management_sdk::{
    PluginArtifactDescriptor, PluginArtifactOwner, PLUGIN_ARTIFACT_MAX_BYTES,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::*;

const MAX_MCP_ARTIFACTS_PER_CALL: usize = 64;
const MAX_MCP_ARTIFACT_REGISTRY_ITEMS: usize = 1_024;

#[derive(Debug, Clone)]
pub(super) struct RegisteredMcpArtifact {
    pub(super) descriptor: PluginArtifactDescriptor,
    pub(super) absolute_path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct McpArtifactCandidate {
    producer_artifact_id: String,
    relative_path: String,
    display_name: String,
    media_type: String,
    size_bytes: u64,
    sha256: String,
}

impl PluginRuntimeHost {
    pub(super) fn remove_mcp_artifacts_for_session(&self, adapter_session_id: &str) {
        if let Ok(mut registered) = self.mcp_artifacts.lock() {
            registered.retain(|_, artifact| {
                artifact.descriptor.owner.adapter_session_id != adapter_session_id
                    && artifact.absolute_path.exists()
            });
        }
    }

    pub(super) fn register_mcp_artifacts(
        &self,
        session: &PreparedPluginSession,
        adapter_session_id: &str,
        tool_name: &str,
        result: &mut Value,
    ) -> Result<()> {
        let Some(candidates) = result
            .pointer("/_meta/chatos~1artifacts")
            .and_then(Value::as_array)
            .cloned()
        else {
            return Ok(());
        };
        if candidates.len() > MAX_MCP_ARTIFACTS_PER_CALL {
            bail!("Plugin MCP returned too many Artifact candidates");
        }
        let mcp = session
            .mcp
            .as_ref()
            .context("Plugin MCP Artifact registration requires an MCP session")?;
        let artifact_root = mcp
            .artifact_dir()?
            .canonicalize()
            .context("resolve Plugin MCP Artifact directory")?;
        let mut authoritative = Vec::with_capacity(candidates.len());
        let mut registered = self
            .mcp_artifacts
            .lock()
            .map_err(|_| anyhow::anyhow!("Plugin MCP Artifact registry is unavailable"))?;
        for candidate in candidates {
            let candidate: McpArtifactCandidate =
                serde_json::from_value(candidate).context("parse Plugin MCP Artifact candidate")?;
            validate_candidate_identity(&candidate)?;
            let absolute_path = resolve_artifact_candidate(
                artifact_root.as_path(),
                candidate.relative_path.as_str(),
            )?;
            let (size_bytes, sha256) = hash_regular_artifact(absolute_path.as_path())?;
            if size_bytes != candidate.size_bytes || sha256 != candidate.sha256 {
                bail!(
                    "Plugin MCP Artifact size or SHA-256 does not match its candidate descriptor"
                );
            }
            if media_type_for_path(absolute_path.as_path()) != Some(candidate.media_type.as_str()) {
                bail!("Plugin MCP Artifact MIME type does not match its file extension");
            }
            let artifact_id = format!("pa_{}", Uuid::new_v4().simple());
            let created_at = Utc::now().to_rfc3339();
            let descriptor = PluginArtifactDescriptor {
                artifact_id: artifact_id.clone(),
                owner: PluginArtifactOwner {
                    owner_user_id: session.owner_user_id.clone(),
                    run_id: session.run_id.clone(),
                    device_id: session.device_id.clone(),
                    workspace_id: session.workspace_id.clone(),
                    plugin_id: session.plugin_id.clone(),
                    release_id: session.release_id.clone(),
                    artifact_sha256: session.artifact_sha256.clone(),
                    component_key: session.component_key.clone(),
                    adapter_session_id: adapter_session_id.to_string(),
                },
                workspace_relative_path: format!(
                    "chatos-plugin-artifacts/{adapter_session_id}/{artifact_id}/{}",
                    candidate.display_name
                ),
                display_name: candidate.display_name.clone(),
                media_type: candidate.media_type.clone(),
                size_bytes,
                sha256,
                created_at,
                producer_tool_name: tool_name.to_string(),
                downloadable: true,
                mutable: false,
            };
            registered.insert(
                artifact_id,
                RegisteredMcpArtifact {
                    descriptor: descriptor.clone(),
                    absolute_path,
                },
            );
            authoritative.push(json!({
                "producer_artifact_id": candidate.producer_artifact_id,
                "artifact": descriptor,
            }));
        }
        if registered.len() > MAX_MCP_ARTIFACT_REGISTRY_ITEMS {
            let mut ids = registered.keys().cloned().collect::<Vec<_>>();
            ids.sort();
            for artifact_id in ids
                .into_iter()
                .take(registered.len() - MAX_MCP_ARTIFACT_REGISTRY_ITEMS)
            {
                registered.remove(artifact_id.as_str());
            }
        }
        if let Some(meta) = result.get_mut("_meta").and_then(Value::as_object_mut) {
            meta.insert("chatos/artifacts".to_string(), Value::Array(authoritative));
        }
        Ok(())
    }
}

fn validate_candidate_identity(candidate: &McpArtifactCandidate) -> Result<()> {
    if candidate.producer_artifact_id.trim() != candidate.producer_artifact_id
        || candidate.producer_artifact_id.is_empty()
        || candidate.producer_artifact_id.len() > 256
        || candidate.display_name.trim() != candidate.display_name
        || candidate.display_name.is_empty()
        || candidate.display_name.len() > 512
        || candidate
            .display_name
            .chars()
            .any(|character| matches!(character, '/' | '\\'))
        || candidate.media_type.trim() != candidate.media_type
        || candidate.media_type.is_empty()
        || candidate.media_type.len() > 256
        || candidate.size_bytes > PLUGIN_ARTIFACT_MAX_BYTES
        || candidate.sha256.len() != 64
        || !candidate
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        bail!("Plugin MCP Artifact candidate identity is invalid");
    }
    Ok(())
}

fn resolve_artifact_candidate(root: &Path, relative_path: &str) -> Result<PathBuf> {
    let relative = Path::new(relative_path);
    if relative.is_absolute()
        || relative_path.len() > 4_096
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("Plugin MCP Artifact path is not a safe relative path");
    }
    let mut cursor = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            bail!("Plugin MCP Artifact path is invalid");
        };
        cursor.push(component);
        let metadata = fs::symlink_metadata(cursor.as_path())
            .with_context(|| format!("inspect Plugin MCP Artifact path: {relative_path}"))?;
        if metadata.file_type().is_symlink() {
            bail!("Plugin MCP Artifact path contains a symbolic link");
        }
    }
    let canonical = cursor
        .canonicalize()
        .with_context(|| format!("resolve Plugin MCP Artifact path: {relative_path}"))?;
    if !canonical.starts_with(root) || !canonical.is_file() {
        bail!("Plugin MCP Artifact escaped its session directory or is not a regular file");
    }
    Ok(canonical)
}

fn hash_regular_artifact(path: &Path) -> Result<(u64, String)> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > PLUGIN_ARTIFACT_MAX_BYTES {
        bail!("Plugin MCP Artifact is not a regular file or exceeds the size limit");
    }
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > PLUGIN_ARTIFACT_MAX_BYTES {
            bail!("Plugin MCP Artifact exceeded the size limit while hashing");
        }
        hasher.update(&buffer[..read]);
    }
    if total != metadata.len() {
        bail!("Plugin MCP Artifact changed while registering");
    }
    Ok((total, hex::encode(hasher.finalize())))
}

fn media_type_for_path(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "pdf" => Some("application/pdf"),
        "json" => Some("application/json"),
        "har" => Some("application/json"),
        "txt" => Some("text/plain"),
        "csv" => Some("text/csv"),
        "zip" => Some("application/zip"),
        "docx" => Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
        "xlsx" => Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
        "pptx" => Some("application/vnd.openxmlformats-officedocument.presentationml.presentation"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn artifact_candidate_resolution_rejects_traversal_and_hashes_regular_files() {
        let temp = TempDir::new().expect("temp directory");
        let artifact = temp.path().join("capture.png");
        fs::write(artifact.as_path(), b"png-fixture").expect("write fixture");
        let root = temp.path().canonicalize().expect("canonical root");
        let resolved = resolve_artifact_candidate(root.as_path(), "capture.png")
            .expect("resolve safe Artifact");
        let (size, sha256) = hash_regular_artifact(resolved.as_path()).expect("hash Artifact");
        assert_eq!(size, 11);
        assert_eq!(sha256, hex::encode(Sha256::digest(b"png-fixture")));
        assert!(resolve_artifact_candidate(root.as_path(), "../outside.png").is_err());
        assert!(resolve_artifact_candidate(root.as_path(), "/tmp/outside.png").is_err());
    }

    #[test]
    fn artifact_candidate_media_types_include_office_documents() {
        assert_eq!(
            media_type_for_path(Path::new("report.docx")),
            Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document")
        );
        assert_eq!(
            media_type_for_path(Path::new("book.xlsx")),
            Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
        );
        assert_eq!(
            media_type_for_path(Path::new("slides.pptx")),
            Some("application/vnd.openxmlformats-officedocument.presentationml.presentation")
        );
    }

    #[cfg(unix)]
    #[test]
    fn artifact_candidate_resolution_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().expect("temp directory");
        let target = temp.path().join("target.png");
        fs::write(target.as_path(), b"png-fixture").expect("write target");
        symlink(target.as_path(), temp.path().join("link.png")).expect("create symlink");
        let root = temp.path().canonicalize().expect("canonical root");
        assert!(resolve_artifact_candidate(root.as_path(), "link.png").is_err());
    }
}
