// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::task::{LOCAL_TASK_MAX_OWNED_PATHS, LOCAL_TASK_MAX_OWNED_PATH_BYTES};
use serde_json::Value;
use std::collections::HashSet;
use unicode_normalization::UnicodeNormalization;

pub fn local_task_write_scope(input: &Value, task_id: &str) -> Result<Option<Vec<String>>, String> {
    let Some(normalized) = normalized_owned_paths(input, task_id)? else {
        return Ok(task_enables_code_write(input).then(Vec::new));
    };
    Ok(task_enables_code_write(input).then_some(normalized))
}

fn normalized_owned_paths(input: &Value, task_id: &str) -> Result<Option<Vec<String>>, String> {
    let Some(raw_paths) = input.get("owned_paths") else {
        return Ok(None);
    };
    let paths = raw_paths
        .as_array()
        .ok_or_else(|| format!("owned_paths must be an array for task: {task_id}"))?;
    if paths.len() > LOCAL_TASK_MAX_OWNED_PATHS {
        return Err(format!(
            "owned_paths must contain at most {LOCAL_TASK_MAX_OWNED_PATHS} entries for task: {task_id}"
        ));
    }
    let mut normalized = Vec::with_capacity(paths.len());
    let mut unique = HashSet::new();
    for value in paths {
        let path = value
            .as_str()
            .ok_or_else(|| format!("owned_paths must contain strings for task: {task_id}"))?;
        validate_owned_path(path).map_err(|error| format!("{error} for task: {task_id}"))?;
        if !unique.insert(path) {
            return Err(format!(
                "owned_paths contains a duplicate for task: {task_id}"
            ));
        }
        normalized.push(path.to_string());
    }
    Ok(Some(normalized))
}

/// Returns the project-relative write scope that must be held for the full Task Run.
///
/// `CodeMaintainerWrite` can be constrained by declared `owned_paths`. Terminal and External MCP
/// execution remain entire-project writers. Plugin execution holds one synthetic lock per selected
/// Plugin component plus any declared paths: Tasks using different components and disjoint files
/// may run in parallel, while the same component or an explicitly shared file stays serialized.
pub fn local_task_runtime_write_scope(
    input: &Value,
    task_id: &str,
) -> Result<Option<Vec<String>>, String> {
    let plugin_components = task_selected_plugin_components(input, task_id)?;
    let external_mcp_selected = task_selects_external_mcp(input, task_id)?;
    let code_write_scope = local_task_write_scope(input, task_id)?;
    if task_enables_terminal(input) || external_mcp_selected {
        return Ok(Some(Vec::new()));
    }
    if !plugin_components.is_empty() {
        let declared_paths = normalized_owned_paths(input, task_id)?.unwrap_or_default();
        if code_write_scope.as_ref().is_some_and(Vec::is_empty) {
            return Ok(Some(Vec::new()));
        }
        let mut paths = code_write_scope.unwrap_or(declared_paths);
        paths.extend(plugin_components.into_iter().map(plugin_runtime_lock_path));
        paths.sort();
        paths.dedup();
        return Ok(Some(paths));
    }
    Ok(code_write_scope)
}

fn task_enables_code_write(input: &Value) -> bool {
    task_enables_builtin_kind(input, "CodeMaintainerWrite")
}

fn task_enables_terminal(input: &Value) -> bool {
    task_enables_builtin_kind(input, "TerminalController")
}

fn task_selected_plugin_components(input: &Value, task_id: &str) -> Result<Vec<String>, String> {
    let Some(value) = input.pointer("/tool_options/plugin_hints") else {
        return Ok(Vec::new());
    };
    if value.is_null() {
        return Ok(Vec::new());
    }
    let values = value
        .as_array()
        .ok_or_else(|| format!("plugin_hints must be an array for task: {task_id}"))?;
    let mut components = Vec::with_capacity(values.len());
    let mut unique = HashSet::new();
    for value in values {
        let key = value
            .get("plugin_key")
            .and_then(Value::as_str)
            .filter(|key| !key.is_empty() && !key.contains('\0'))
            .ok_or_else(|| {
                format!("plugin_hints must contain plugin_key strings for task: {task_id}")
            })?;
        let component_key = value
            .get("component_key")
            .and_then(Value::as_str)
            .filter(|key| !key.is_empty() && !key.contains('\0'))
            .ok_or_else(|| {
                format!("plugin_hints must contain component_key strings for task: {task_id}")
            })?;
        let selection_id = format!("{key}\0{component_key}");
        if !unique.insert(selection_id.clone()) {
            return Err(format!(
                "plugin_hints contains a duplicate plugin component for task: {task_id}"
            ));
        }
        components.push(selection_id);
    }
    components.sort();
    Ok(components)
}

fn task_selects_external_mcp(input: &Value, task_id: &str) -> Result<bool, String> {
    let Some(value) = input.pointer("/tool_options/external_mcp_config_ids") else {
        return Ok(false);
    };
    if value.is_null() {
        return Ok(false);
    }
    let values = value
        .as_array()
        .ok_or_else(|| format!("external_mcp_config_ids must be an array for task: {task_id}"))?;
    Ok(!values.is_empty())
}

fn plugin_runtime_lock_path(plugin_component: String) -> String {
    let encoded = plugin_component
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!(".chatos-runtime/plugin/{encoded}")
}

fn task_enables_builtin_kind(input: &Value, expected: &str) -> bool {
    input
        .pointer("/tool_options/enabled_builtin_kinds")
        .and_then(Value::as_array)
        .is_some_and(|kinds| kinds.iter().any(|kind| kind.as_str() == Some(expected)))
}

fn validate_owned_path(path: &str) -> Result<(), String> {
    if path.is_empty() || path.len() > LOCAL_TASK_MAX_OWNED_PATH_BYTES || path.trim() != path {
        return Err(format!(
            "owned path must be 1..={LOCAL_TASK_MAX_OWNED_PATH_BYTES} bytes without surrounding whitespace"
        ));
    }
    let first_component = path.split('/').next().unwrap_or_default();
    let windows_drive = first_component.as_bytes().get(1) == Some(&b':')
        && first_component
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic);
    if path.starts_with('/')
        || path.starts_with('\\')
        || path.starts_with('~')
        || path.contains('\\')
        || path.contains('\0')
        || path
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
        || windows_drive
    {
        return Err("owned path must be a normalized project-relative path".to_string());
    }
    Ok(())
}

pub fn local_task_write_scope_overlap(left: &[String], right: &[String]) -> Option<String> {
    if left.is_empty() || right.is_empty() {
        return Some("<entire-project>".to_string());
    }
    // Default macOS volumes alias case and Unicode normalization variants. Normalize each
    // scope once, rather than once per pair, and preserve the original model paths.
    let keys = |paths: &[String]| -> Vec<String> {
        paths
            .iter()
            .map(|path| path.nfd().flat_map(char::to_lowercase).collect())
            .collect()
    };
    let left_keys = keys(left);
    let right_keys = keys(right);
    for (left_path, left_key) in left.iter().zip(&left_keys) {
        for (right_path, right_key) in right.iter().zip(&right_keys) {
            if left_key == right_key
                || is_path_ancestor(left_key, right_key)
                || is_path_ancestor(right_key, left_key)
            {
                return Some(if left_path.len() <= right_path.len() {
                    left_path.clone()
                } else {
                    right_path.clone()
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod ownership_alias_tests {
    use super::local_task_write_scope_overlap;

    #[test]
    fn write_locks_cover_case_and_unicode_normalization_aliases() {
        for (left, right) in [
            ("Sources", "sources/app.swift"),
            ("assets/Caf\u{e9}", "ASSETS/Cafe\u{301}/image.png"),
        ] {
            assert!(local_task_write_scope_overlap(&[left.into()], &[right.into()]).is_some());
        }
        assert!(local_task_write_scope_overlap(&["src".into()], &["src-other".into()]).is_none());
    }
}

fn is_path_ancestor(ancestor: &str, descendant: &str) -> bool {
    descendant
        .strip_prefix(ancestor)
        .is_some_and(|suffix| suffix.starts_with('/'))
}
