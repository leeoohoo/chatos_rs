// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::artifact_template_model::{
    template_argument_placeholders, template_manifest_placeholders, template_values,
    TemplatePlaceholder,
};
use super::artifact_template_zip::{
    ensure_distinct_template_output, instantiate_semantic_template, template_placeholder_counts,
};
use super::format_helpers::{
    read_template_manifest, required_json_text, sha256_file, supported_artifact_extension,
    template_artifact_file,
};
use super::{
    docx_render, file_size, input_file_any, optional_bool, optional_text, required_text,
    safe_workspace_path, write_binary_copy,
};

pub(super) fn inspect_artifact_template(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let directory = required_text(arguments, "template_directory")?;
    let (path, relative) = safe_workspace_path(state, request, directory)?;
    let manifest = read_template_manifest(path.as_path())?;
    let artifact_file = template_artifact_file(&manifest)?;
    let artifact_path = path.join(artifact_file);
    let expected = required_json_text(&manifest, "sha256")?;
    let actual = sha256_file(artifact_path.as_path())?;
    let hash_valid = expected == actual;
    let placeholders = template_manifest_placeholders(&manifest)?;
    let placeholder_valid = if hash_valid && !placeholders.is_empty() {
        let kind = required_json_text(&manifest, "artifact_kind")?;
        let counts = template_placeholder_counts(artifact_path.as_path(), kind, &placeholders)?;
        placeholders
            .iter()
            .all(|placeholder| counts.get(&placeholder.name) == Some(&placeholder.occurrences))
    } else {
        hash_valid
    };
    Ok(json!({
        "path":relative,
        "manifest":manifest,
        "hash_valid":hash_valid,
        "placeholder_valid":placeholder_valid,
        "placeholder_count":placeholders.len(),
        "actual_sha256":actual
    }))
}

pub(super) fn create_artifact_template(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let source_requested = required_text(arguments, "source_path")?;
    let (source, source_relative) = input_file_any(state, request, source_requested)?;
    let extension = supported_artifact_extension(source.as_path())?;
    let mut placeholders = template_argument_placeholders(arguments)?;
    if !placeholders.is_empty() && !matches!(extension.as_str(), "docx" | "pptx" | "xlsx") {
        return Err(anyhow!(
            "semantic placeholders are supported only for DOCX, PPTX, and XLSX templates"
        ));
    }
    if !placeholders.is_empty() {
        let counts =
            template_placeholder_counts(source.as_path(), extension.as_str(), &placeholders)?;
        for placeholder in &mut placeholders {
            placeholder.occurrences = *counts.get(&placeholder.name).unwrap_or(&0);
            if placeholder.occurrences == 0 {
                return Err(anyhow!(
                    "template placeholder token was not found inside a single supported text run or cell: {}",
                    placeholder.token
                ));
            }
        }
    }
    let target_directory = required_text(arguments, "target_directory")?;
    let (target, target_relative) = safe_workspace_path(state, request, target_directory)?;
    let overwrite = optional_bool(arguments, "overwrite");
    if target.exists() {
        if !overwrite {
            return Err(anyhow!(
                "template directory already exists; set overwrite=true to replace it"
            ));
        }
        if !target.is_dir() {
            return Err(anyhow!("template target exists and is not a directory"));
        }
        fs::remove_dir_all(target.as_path())
            .with_context(|| format!("replace template directory {}", target.display()))?;
    }
    fs::create_dir_all(target.as_path())
        .with_context(|| format!("create template directory {}", target.display()))?;
    let artifact_file = format!("artifact.{extension}");
    let artifact_path = target.join(artifact_file.as_str());
    fs::copy(source.as_path(), artifact_path.as_path())
        .with_context(|| format!("copy template artifact {}", source.display()))?;
    let bytes = file_size(artifact_path.as_path())?;
    let placeholder_manifest = placeholders
        .iter()
        .map(TemplatePlaceholder::manifest_value)
        .collect::<Vec<_>>();
    let manifest = json!({
        "schema_version": 2,
        "template_name": required_text(arguments, "template_name")?,
        "version": optional_text(arguments, "version").unwrap_or_else(|| "1.0.0".to_string()),
        "description": optional_text(arguments, "description").unwrap_or_default(),
        "artifact_kind": extension,
        "artifact_file": artifact_file,
        "sha256": sha256_file(artifact_path.as_path())?,
        "bytes": bytes,
        "source_path": source_relative,
        "placeholder_syntax": "double_braces_v1",
        "placeholders": placeholder_manifest,
    });
    let manifest_text = serde_json::to_string_pretty(&manifest)?;
    fs::write(target.join("template.json"), manifest_text)
        .with_context(|| format!("write template manifest {}", target.display()))?;
    Ok(json!({"created":true,"path":target_relative,"manifest":manifest}))
}

pub(super) fn instantiate_artifact_template(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let (template, template_relative) = safe_workspace_path(
        state,
        request,
        required_text(arguments, "template_directory")?,
    )?;
    let manifest = read_template_manifest(template.as_path())?;
    let artifact_file = template_artifact_file(&manifest)?;
    let source = template.join(artifact_file);
    let expected_hash = required_json_text(&manifest, "sha256")?;
    let actual_hash = sha256_file(source.as_path())?;
    if expected_hash != actual_hash {
        return Err(anyhow!(
            "template artifact hash does not match template.json"
        ));
    }
    let target_requested = required_text(arguments, "target_path")?;
    let target_extension = Path::new(target_requested)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if target_extension != required_json_text(&manifest, "artifact_kind")? {
        return Err(anyhow!(
            "target extension does not match the template artifact kind"
        ));
    }
    let (target, target_relative) = safe_workspace_path(state, request, target_requested)?;
    ensure_distinct_template_output(source.as_path(), target.as_path())?;
    let placeholders = template_manifest_placeholders(&manifest)?;
    let replacements = template_values(arguments, &placeholders)?;
    let replacement_count = if placeholders.is_empty() {
        write_binary_copy(
            source.as_path(),
            target.as_path(),
            optional_bool(arguments, "overwrite"),
        )?;
        0
    } else {
        let counts = template_placeholder_counts(
            source.as_path(),
            required_json_text(&manifest, "artifact_kind")?,
            &placeholders,
        )?;
        if placeholders
            .iter()
            .any(|placeholder| counts.get(&placeholder.name) != Some(&placeholder.occurrences))
        {
            return Err(anyhow!(
                "template placeholder occurrences do not match template.json"
            ));
        }
        instantiate_semantic_template(
            source.as_path(),
            target.as_path(),
            required_json_text(&manifest, "artifact_kind")?,
            &replacements,
            optional_bool(arguments, "overwrite"),
        )?
    };
    Ok(json!({
        "created":true,
        "template":template_relative,
        "path":target_relative,
        "sha256":sha256_file(target.as_path())?,
        "source_sha256":actual_hash,
        "bytes":file_size(target.as_path())?,
        "placeholders":placeholders.len(),
        "replacements":replacement_count,
        "source_unchanged":true
    }))
}

pub(super) fn render_artifact_template_preview(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    render_artifact_template_preview_with_runtime(arguments, state, request, action_cancelled, None)
}

pub(super) fn render_artifact_template_preview_with_runtime(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
    runtime_root_override: Option<&Path>,
) -> Result<Value> {
    let (template, template_relative) = safe_workspace_path(
        state,
        request,
        required_text(arguments, "template_directory")?,
    )?;
    let template_metadata = fs::symlink_metadata(template.as_path()).map_err(|error| {
        anyhow!("template_render/template_invalid: inspect template directory: {error}")
    })?;
    if template_metadata.file_type().is_symlink() || !template_metadata.is_dir() {
        return Err(anyhow!(
            "template_render/template_invalid: template_directory must be a regular non-symlink directory"
        ));
    }
    let manifest = read_template_manifest(template.as_path())
        .map_err(|error| anyhow!("template_render/template_invalid: {error}"))?;
    let artifact_file = template_artifact_file(&manifest)
        .map_err(|error| anyhow!("template_render/template_invalid: {error}"))?;
    let artifact_kind = required_json_text(&manifest, "artifact_kind")?.to_ascii_lowercase();
    let artifact_path = template.join(artifact_file);
    let artifact_metadata = fs::symlink_metadata(artifact_path.as_path()).map_err(|error| {
        anyhow!("template_render/template_invalid: inspect template artifact: {error}")
    })?;
    if artifact_metadata.file_type().is_symlink() || !artifact_metadata.is_file() {
        return Err(anyhow!(
            "template_render/template_invalid: template artifact must be a regular non-symlink file"
        ));
    }
    let actual_kind = supported_artifact_extension(artifact_path.as_path())?;
    if actual_kind != artifact_kind {
        return Err(anyhow!(
            "template_render/template_invalid: artifact extension does not match template.json"
        ));
    }
    if artifact_kind == "csv" {
        return Err(anyhow!(
            "template_render/artifact_unsupported: CSV templates do not have a paginated visual preview"
        ));
    }
    let expected_hash = required_json_text(&manifest, "sha256")?;
    let actual_hash = sha256_file(artifact_path.as_path())?;
    if expected_hash != actual_hash {
        return Err(anyhow!(
            "template_render/template_hash_mismatch: template artifact hash does not match template.json"
        ));
    }
    let placeholders = template_manifest_placeholders(&manifest)?;
    if !placeholders.is_empty() {
        let counts = template_placeholder_counts(
            artifact_path.as_path(),
            artifact_kind.as_str(),
            &placeholders,
        )?;
        if placeholders
            .iter()
            .any(|placeholder| counts.get(&placeholder.name) != Some(&placeholder.occurrences))
        {
            return Err(anyhow!(
                "template_render/placeholder_mismatch: template placeholder occurrences do not match template.json"
            ));
        }
    }

    let artifact_relative = Path::new(template_relative.as_str())
        .join(artifact_file)
        .to_string_lossy()
        .replace('\\', "/");
    let mut render_arguments = serde_json::Map::new();
    render_arguments.insert("path".to_string(), Value::String(artifact_relative));
    let first = arguments
        .get("first_page")
        .cloned()
        .unwrap_or_else(|| json!(1));
    if artifact_kind == "pptx" {
        render_arguments.insert("first_slide".to_string(), first);
        if let Some(last) = arguments.get("last_page") {
            render_arguments.insert("last_slide".to_string(), last.clone());
        }
    } else {
        render_arguments.insert("first_page".to_string(), first);
        if let Some(last) = arguments.get("last_page") {
            render_arguments.insert("last_page".to_string(), last.clone());
        }
    }
    for field in ["dpi", "timeout_seconds"] {
        if let Some(value) = arguments.get(field) {
            render_arguments.insert(field.to_string(), value.clone());
        }
    }
    let render_arguments = Value::Object(render_arguments);
    let mut rendered = match artifact_kind.as_str() {
        "docx" => docx_render::render_docx_pages_with_runtime(
            &render_arguments,
            state,
            request,
            action_cancelled,
            runtime_root_override,
        ),
        "pdf" => docx_render::render_pdf_pages_with_runtime(
            &render_arguments,
            state,
            request,
            action_cancelled,
            runtime_root_override,
        ),
        "pptx" => docx_render::render_presentation_pages_with_runtime(
            &render_arguments,
            state,
            request,
            action_cancelled,
            runtime_root_override,
        ),
        "xlsx" => docx_render::render_spreadsheet_pages_with_runtime(
            &render_arguments,
            state,
            request,
            action_cancelled,
            runtime_root_override,
        ),
        _ => Err(anyhow!(
            "template_render/artifact_unsupported: unsupported template artifact kind"
        )),
    }?;
    let structured = rendered
        .get_mut("_structured_result")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            anyhow!("template_render/result_invalid: renderer omitted structured result")
        })?;
    structured.insert("template".to_string(), Value::String(template_relative));
    structured.insert("artifact_kind".to_string(), Value::String(artifact_kind));
    structured.insert(
        "preview_of".to_string(),
        Value::String("stored_template_reference".to_string()),
    );
    structured.insert("template_hash_valid".to_string(), Value::Bool(true));
    structured.insert("template_placeholder_valid".to_string(), Value::Bool(true));
    Ok(rendered)
}
