// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::workspace::paths::{
    canonicalize_existing_dir, normalize_request_workspace_relative_path, workspace_for_request,
};
use crate::LocalState;

mod artifacts;
mod web;

const MAX_MANIFEST_BYTES: usize = 256 * 1024;
const MAX_INSTRUCTIONS_BYTES: usize = 512 * 1024;
const MAX_VISUALIZATION_BYTES: usize = 2 * 1024 * 1024;

pub(super) fn tool_definitions(
    skill_id: &str,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Vec<Value>> {
    let tools = match skill_id {
        "internal_skill_skill_creator" => vec![
            validate_skill_bundle_manifest_tool(),
            scaffold_skill_bundle_tool(),
        ],
        "internal_skill_plugin_creator" => {
            vec![validate_plugin_manifest_tool(), scaffold_plugin_tool()]
        }
        "internal_skill_visualize" => vec![write_visualization_html_tool()],
        _ => artifacts::tool_definitions(skill_id),
    };
    if tools.is_empty() {
        web::tool_definitions(skill_id, state, request)
    } else {
        Ok(tools)
    }
}

pub(super) fn dependency_error(skill_id: &str) -> Option<String> {
    web::dependency_error(skill_id)
}

pub(super) fn execute(
    skill_id: &str,
    operation: &str,
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    match (skill_id, operation) {
        ("internal_skill_skill_creator", "validate_skill_bundle_manifest") => {
            validate_skill_bundle_manifest(arguments)
        }
        ("internal_skill_skill_creator", "scaffold_skill_bundle") => {
            scaffold_skill_bundle(arguments, state, request)
        }
        ("internal_skill_plugin_creator", "validate_plugin_manifest") => {
            validate_plugin_manifest(arguments)
        }
        ("internal_skill_plugin_creator", "scaffold_plugin") => {
            scaffold_plugin(arguments, state, request)
        }
        ("internal_skill_visualize", "write_visualization_html") => {
            write_visualization_html(arguments, state, request)
        }
        _ => artifacts::execute(skill_id, operation, arguments, state, request)
            .or_else(|| web::execute(skill_id, operation, arguments, state, request))
            .unwrap_or_else(|| {
                Err(anyhow!(
                    "Skill operation is not implemented: {skill_id}/{operation}"
                ))
            }),
    }
}

fn validate_skill_bundle_manifest_tool() -> Value {
    json!({
        "name": "validate_skill_bundle_manifest",
        "description": "Validate a ChatOS Local Connector Skill Bundle manifest without writing files.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "manifest": {
                    "type": "object",
                    "description": "The skill.json object to validate."
                }
            },
            "required": ["manifest"],
            "additionalProperties": false
        }
    })
}

fn scaffold_skill_bundle_tool() -> Value {
    json!({
        "name": "scaffold_skill_bundle",
        "description": "Create a versioned ChatOS Skill Bundle scaffold inside the authorized local workspace.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "target_directory": {"type":"string","description":"Workspace-relative bundle root, for example local_connector_client/skill_bundles/internal/my-skill."},
                "bundle_id": {"type":"string"},
                "skill_id": {"type":"string"},
                "name": {"type":"string"},
                "display_name": {"type":"string"},
                "description": {"type":"string"},
                "version": {"type":"string","default":"1.0.0"},
                "entrypoint_kind": {"type":"string","enum":["prompt_only","native_adapter","process","mcp_bridge","composite"]},
                "instructions": {"type":"string"},
                "permissions": {"type":"array","items":{"type":"string"},"default":[]},
                "platforms": {"type":"array","items":{"type":"string"},"default":["macos-arm64","macos-x64","windows-x64","windows-arm64"]},
                "overwrite": {"type":"boolean","default":false}
            },
            "required":["target_directory","bundle_id","skill_id","name","display_name","description","version","entrypoint_kind","instructions"],
            "additionalProperties":false
        }
    })
}

fn validate_plugin_manifest_tool() -> Value {
    json!({
        "name": "validate_plugin_manifest",
        "description": "Validate a ChatOS plugin manifest without writing files.",
        "inputSchema": {
            "type":"object",
            "properties":{"manifest":{"type":"object"}},
            "required":["manifest"],
            "additionalProperties":false
        }
    })
}

fn scaffold_plugin_tool() -> Value {
    json!({
        "name": "scaffold_plugin",
        "description": "Create .chatos-plugin/plugin.json inside an authorized local workspace directory.",
        "inputSchema": {
            "type":"object",
            "properties":{
                "target_directory":{"type":"string","description":"Workspace-relative plugin project directory."},
                "plugin_id":{"type":"string"},
                "name":{"type":"string"},
                "description":{"type":"string"},
                "version":{"type":"string","default":"1.0.0"},
                "skill_bundle_ids":{"type":"array","items":{"type":"string"},"default":[]},
                "mcp_resource_ids":{"type":"array","items":{"type":"string"},"default":[]},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["target_directory","plugin_id","name","description","version"],
            "additionalProperties":false
        }
    })
}

fn write_visualization_html_tool() -> Value {
    json!({
        "name": "write_visualization_html",
        "description": "Write a self-contained interactive HTML visualization inside the authorized local workspace. Remote network resources are blocked by CSP.",
        "inputSchema": {
            "type":"object",
            "properties":{
                "target_path":{"type":"string","description":"Workspace-relative .html output path."},
                "title":{"type":"string"},
                "body_html":{"type":"string","description":"HTML placed inside the page body."},
                "css":{"type":"string","default":""},
                "javascript":{"type":"string","default":""},
                "overwrite":{"type":"boolean","default":false}
            },
            "required":["target_path","title","body_html"],
            "additionalProperties":false
        }
    })
}

fn validate_skill_bundle_manifest(arguments: &Value) -> Result<Value> {
    let manifest = required_object(arguments, "manifest")?;
    validate_json_size(manifest, MAX_MANIFEST_BYTES, "Skill manifest")?;
    let bundle_id = required_map_text(manifest, "bundle_id")?;
    let skill_id = required_map_text(manifest, "skill_id")?;
    let name = required_map_text(manifest, "name")?;
    let version = required_map_text(manifest, "version")?;
    let entrypoint_kind = manifest
        .get("entrypoint")
        .and_then(Value::as_object)
        .and_then(|entrypoint| entrypoint.get("kind"))
        .and_then(Value::as_str)
        .or_else(|| manifest.get("entrypoint_kind").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("entrypoint.kind is required"))?;
    validate_bundle_id(bundle_id)?;
    validate_skill_id(skill_id)?;
    validate_slug(name, "name")?;
    validate_version(version)?;
    validate_entrypoint_kind(entrypoint_kind)?;
    Ok(json!({
        "valid": true,
        "bundle_id": bundle_id,
        "skill_id": skill_id,
        "name": name,
        "version": version,
        "entrypoint_kind": entrypoint_kind,
    }))
}

fn scaffold_skill_bundle(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let target_directory = required_text(arguments, "target_directory")?;
    let version = required_text(arguments, "version")?;
    let instructions = required_text(arguments, "instructions")?;
    if instructions.len() > MAX_INSTRUCTIONS_BYTES {
        return Err(anyhow!("Skill instructions exceed the local size limit"));
    }
    let permissions = optional_string_array(arguments, "permissions")?;
    let platforms = optional_string_array(arguments, "platforms")?;
    let entrypoint_kind = required_text(arguments, "entrypoint_kind")?;
    let name = required_text(arguments, "name")?;
    let manifest = json!({
        "schema_version": 1,
        "bundle_id": required_text(arguments, "bundle_id")?,
        "skill_id": required_text(arguments, "skill_id")?,
        "name": name,
        "display_name": required_text(arguments, "display_name")?,
        "description": required_text(arguments, "description")?,
        "version": version,
        "publisher": "chatos",
        "source_kind": "admin_created",
        "entrypoint": {
            "kind": entrypoint_kind,
            "adapter": if entrypoint_kind == "native_adapter" { Some(name) } else { None::<&str> }
        },
        "instructions_path": "instructions.md",
        "permissions": permissions,
        "platforms": platforms,
    });
    validate_skill_bundle_manifest(&json!({"manifest": manifest.clone()}))?;
    let version_dir = format!(
        "{}/{}",
        target_directory.trim_end_matches(['/', '\\']),
        version
    );
    let (absolute_dir, relative_dir) = safe_workspace_path(state, request, version_dir.as_str())?;
    let overwrite = optional_bool(arguments, "overwrite");
    write_text_file(
        absolute_dir.join("skill.json").as_path(),
        serde_json::to_string_pretty(&manifest)?.as_str(),
        overwrite,
    )?;
    write_text_file(
        absolute_dir.join("instructions.md").as_path(),
        instructions,
        overwrite,
    )?;
    Ok(json!({
        "created": true,
        "bundle_directory": relative_dir,
        "files": [format!("{relative_dir}/skill.json"), format!("{relative_dir}/instructions.md")],
        "manifest": manifest,
    }))
}

fn validate_plugin_manifest(arguments: &Value) -> Result<Value> {
    let manifest = required_object(arguments, "manifest")?;
    validate_json_size(manifest, MAX_MANIFEST_BYTES, "Plugin manifest")?;
    let plugin_id = required_map_text(manifest, "plugin_id")?;
    let name = required_map_text(manifest, "name")?;
    let version = required_map_text(manifest, "version")?;
    validate_dotted_id(plugin_id, "plugin_id")?;
    if name.len() > 120 {
        return Err(anyhow!("name is too long"));
    }
    validate_version(version)?;
    Ok(json!({
        "valid": true,
        "plugin_id": plugin_id,
        "name": name,
        "version": version,
        "skill_bundle_count": manifest.get("skill_bundle_ids").and_then(Value::as_array).map_or(0, Vec::len),
        "mcp_resource_count": manifest.get("mcp_resource_ids").and_then(Value::as_array).map_or(0, Vec::len),
    }))
}

fn scaffold_plugin(arguments: &Value, state: &LocalState, request: &RelayRequest) -> Result<Value> {
    let target_directory = required_text(arguments, "target_directory")?;
    let manifest = json!({
        "schema_version": 1,
        "plugin_id": required_text(arguments, "plugin_id")?,
        "name": required_text(arguments, "name")?,
        "description": required_text(arguments, "description")?,
        "version": required_text(arguments, "version")?,
        "publisher": "chatos",
        "source_kind": "admin_created",
        "skill_bundle_ids": optional_string_array(arguments, "skill_bundle_ids")?,
        "mcp_resource_ids": optional_string_array(arguments, "mcp_resource_ids")?,
    });
    validate_plugin_manifest(&json!({"manifest": manifest.clone()}))?;
    let manifest_relative_path = format!(
        "{}/.chatos-plugin/plugin.json",
        target_directory.trim_end_matches(['/', '\\'])
    );
    let (manifest_path, relative_path) =
        safe_workspace_path(state, request, manifest_relative_path.as_str())?;
    write_text_file(
        manifest_path.as_path(),
        serde_json::to_string_pretty(&manifest)?.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "manifest_path": relative_path,
        "manifest": manifest,
    }))
}

fn write_visualization_html(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let target_path = required_text(arguments, "target_path")?;
    if !target_path.to_ascii_lowercase().ends_with(".html") {
        return Err(anyhow!("visualization target_path must end with .html"));
    }
    let title = required_text(arguments, "title")?;
    let body_html = required_text(arguments, "body_html")?;
    let css = optional_text(arguments, "css").unwrap_or_default();
    let javascript = optional_text(arguments, "javascript").unwrap_or_default();
    let page = format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data: blob:; font-src data:; connect-src 'none'; media-src data: blob:; object-src 'none'; base-uri 'none'; form-action 'none'\">\n<title>{}</title>\n<style>html{{color-scheme:light dark}}body{{margin:0;font-family:system-ui,-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif}}*{{box-sizing:border-box}}{}</style>\n</head>\n<body>\n{}\n<script>\"use strict\";\n{}\n</script>\n</body>\n</html>\n",
        escape_html(title),
        css,
        body_html,
        javascript,
    );
    if page.len() > MAX_VISUALIZATION_BYTES {
        return Err(anyhow!("visualization exceeds the local size limit"));
    }
    let (absolute_path, relative_path) = safe_workspace_path(state, request, target_path)?;
    write_text_file(
        absolute_path.as_path(),
        page.as_str(),
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "path": relative_path,
        "bytes": page.len(),
        "network_access": "blocked",
        "content_security_policy": true,
    }))
}

fn safe_workspace_path(
    state: &LocalState,
    request: &RelayRequest,
    requested: &str,
) -> Result<(PathBuf, String)> {
    if request.workspace_id.trim().is_empty() {
        return Err(anyhow!("workspace_id is required for this Skill operation"));
    }
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let root = canonicalize_existing_dir(workspace.absolute_root.as_path())?;
    let relative = normalize_request_workspace_relative_path(workspace, request, requested)?;
    if relative == "." {
        return Err(anyhow!("a file or directory path is required"));
    }
    let candidate = root.join(Path::new(relative.as_str()));
    ensure_existing_ancestor_inside_workspace(root.as_path(), candidate.as_path())?;
    if candidate.exists() {
        let canonical = candidate
            .canonicalize()
            .with_context(|| format!("resolve Skill output path {}", candidate.display()))?;
        if !canonical.starts_with(root.as_path()) {
            return Err(anyhow!(
                "Skill output path escapes the authorized workspace"
            ));
        }
    }
    Ok((candidate, relative))
}

fn ensure_existing_ancestor_inside_workspace(root: &Path, candidate: &Path) -> Result<()> {
    let mut cursor = candidate;
    while !cursor.exists() {
        cursor = cursor
            .parent()
            .ok_or_else(|| anyhow!("Skill output path has no existing parent"))?;
    }
    let canonical = cursor
        .canonicalize()
        .with_context(|| format!("resolve Skill output parent {}", cursor.display()))?;
    if !canonical.starts_with(root) {
        return Err(anyhow!(
            "Skill output path escapes the authorized workspace"
        ));
    }
    Ok(())
}

fn write_text_file(path: &Path, content: &str, overwrite: bool) -> Result<()> {
    if path.exists() && !overwrite {
        return Err(anyhow!(
            "refusing to overwrite existing path without overwrite=true: {}",
            path.display()
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("output path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create Skill output directory {}", parent.display()))?;
    fs::write(path, content).with_context(|| format!("write Skill output {}", path.display()))?;
    Ok(())
}

fn required_object<'a>(
    value: &'a Value,
    field: &str,
) -> Result<&'a serde_json::Map<String, Value>> {
    value
        .get(field)
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("{field} must be an object"))
}

fn required_text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("{field} is required"))
}

fn required_map_text<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("{field} is required"))
}

fn optional_text(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(str::to_string)
}

fn optional_bool(value: &Value, field: &str) -> bool {
    value.get(field).and_then(Value::as_bool).unwrap_or(false)
}

fn optional_string_array(value: &Value, field: &str) -> Result<Vec<String>> {
    let Some(items) = value.get(field) else {
        return Ok(Vec::new());
    };
    let items = items
        .as_array()
        .ok_or_else(|| anyhow!("{field} must be an array"))?;
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| anyhow!("{field} contains an invalid string"))
        })
        .collect()
}

fn validate_json_size(
    value: &serde_json::Map<String, Value>,
    max_bytes: usize,
    label: &str,
) -> Result<()> {
    if serde_json::to_vec(value)?.len() > max_bytes {
        return Err(anyhow!("{label} exceeds the local size limit"));
    }
    Ok(())
}

fn validate_bundle_id(value: &str) -> Result<()> {
    validate_dotted_id(value, "bundle_id")?;
    if !value.starts_with("chatos.") {
        return Err(anyhow!("bundle_id must use the chatos. namespace"));
    }
    Ok(())
}

fn validate_dotted_id(value: &str, field: &str) -> Result<()> {
    if value.len() > 128
        || value
            .split('.')
            .any(|part| part.is_empty() || !is_slug(part))
    {
        return Err(anyhow!(
            "{field} must contain lowercase dotted slug segments"
        ));
    }
    Ok(())
}

fn validate_skill_id(value: &str) -> Result<()> {
    if value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(anyhow!(
            "skill_id must contain lowercase letters, digits, and underscores only"
        ));
    }
    Ok(())
}

fn validate_slug(value: &str, field: &str) -> Result<()> {
    if value.len() > 64 || !is_slug(value) {
        return Err(anyhow!(
            "{field} must contain lowercase letters, digits, and hyphens only"
        ));
    }
    Ok(())
}

fn is_slug(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-')
}

fn validate_version(value: &str) -> Result<()> {
    let core = value.split_once('-').map_or(value, |(core, _)| core);
    let parts = core.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(anyhow!("version must be a semantic x.y.z version"));
    }
    Ok(())
}

fn validate_entrypoint_kind(value: &str) -> Result<()> {
    if !matches!(
        value,
        "prompt_only" | "native_adapter" | "process" | "mcp_bridge" | "composite"
    ) {
        return Err(anyhow!("unsupported entrypoint kind: {value}"));
    }
    Ok(())
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorkspaceState;
    use std::collections::BTreeMap;

    #[test]
    fn validates_skill_manifest() {
        let result = validate_skill_bundle_manifest(&json!({
            "manifest": {
                "bundle_id": "chatos.internal.demo-skill",
                "skill_id": "internal_skill_demo",
                "name": "demo-skill",
                "version": "1.0.0",
                "entrypoint": {"kind": "native_adapter"}
            }
        }))
        .expect("manifest");
        assert_eq!(result.get("valid").and_then(Value::as_bool), Some(true));
    }

    #[test]
    fn visualization_is_written_inside_workspace() {
        let root = std::env::temp_dir().join(format!("chatos-skill-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.as_path()).expect("workspace");
        let state = LocalState {
            workspaces: vec![WorkspaceState {
                id: "workspace-1".to_string(),
                absolute_root: root.clone(),
                alias: "test".to_string(),
                fingerprint: "fp".to_string(),
            }],
            ..LocalState::default()
        };
        let request = RelayRequest {
            _message_type: "skill_execute_request".to_string(),
            request_id: "request-1".to_string(),
            owner_user_id: Some("owner-1".to_string()),
            device_id: Some("device-1".to_string()),
            workspace_id: "workspace-1".to_string(),
            method: Some("POST".to_string()),
            path: Some("/skills/execute".to_string()),
            headers: BTreeMap::new(),
            body: Value::Null,
        };
        let result = write_visualization_html(
            &json!({
                "target_path": "artifacts/demo.html",
                "title": "Demo",
                "body_html": "<main>ok</main>"
            }),
            &state,
            &request,
        )
        .expect("visualization");
        assert_eq!(result.get("created").and_then(Value::as_bool), Some(true));
        let output = fs::read_to_string(root.join("artifacts/demo.html")).expect("output");
        assert!(output.contains("connect-src 'none'"));
        let _ = fs::remove_dir_all(root);
    }
}
