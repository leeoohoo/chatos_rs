// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use serde_json::Value;

use crate::arguments::parse_json_tool_args;
use crate::tool_call::{clone_tool_call_arguments, extract_tool_call_name};
use crate::types::ToolInfo;

const PARALLEL_SAFE_TOOLS: &[&str] = &[
    "get_command_detail",
    "get_plugin_detail",
    "get_recent_logs",
    "get_skill_detail",
    "process_list",
    "process_log",
    "process_poll",
    "list_available_skills",
    "list_connections",
    "list_dir",
    "list_directory",
    "list_folders",
    "list_notes",
    "list_tags",
    "list_tasks",
    "preview_agent_context",
    "read_file",
    "read_file_range",
    "read_file_raw",
    "read_note",
    "recommend_agent_profile",
    "search_notes",
    "search_text",
    "test_connection",
    "web_extract",
    "web_research",
    "web_search",
];

const PARALLEL_PATH_READ_TOOLS: &[&str] = &[
    "list_dir",
    "list_directory",
    "read_file",
    "read_file_range",
    "read_file_raw",
    "search_text",
];

const PARALLEL_PATH_WRITE_TOOLS: &[&str] = &["edit_file", "write_file"];

pub trait ToolParallelismInfo {
    fn original_name(&self) -> &str;
    fn server_name(&self) -> &str;
}

impl ToolParallelismInfo for ToolInfo {
    fn original_name(&self) -> &str {
        self.original_name.as_str()
    }

    fn server_name(&self) -> &str {
        self.server_name.as_str()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolAccessKind {
    Read,
    Write,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ToolScope {
    Global,
    Path { locator: String, path: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ToolAccessProfile {
    kind: ToolAccessKind,
    scope: ToolScope,
}

pub fn should_parallelize_tool_batch<T>(
    tool_calls: &[Value],
    tool_metadata: &HashMap<String, T>,
) -> bool
where
    T: ToolParallelismInfo,
{
    if tool_calls.len() <= 1 {
        return false;
    }

    let mut access_profiles = Vec::with_capacity(tool_calls.len());
    for tool_call in tool_calls {
        let Some(prefixed_name) = extract_tool_call_name(tool_call) else {
            return false;
        };
        let Some(info) = tool_metadata.get(prefixed_name) else {
            return false;
        };
        if !PARALLEL_SAFE_TOOLS.contains(&info.original_name()) {
            return false;
        }

        let args = clone_tool_call_arguments(tool_call);
        let Ok(args) = parse_json_tool_args(args) else {
            return false;
        };
        let Some(profile) = build_tool_access_profile(info, &args) else {
            return false;
        };
        access_profiles.push(profile);
    }

    !has_conflicting_tool_profiles(access_profiles.as_slice())
}

fn has_conflicting_tool_profiles(profiles: &[ToolAccessProfile]) -> bool {
    for (index, left) in profiles.iter().enumerate() {
        for right in profiles.iter().skip(index + 1) {
            if tool_profiles_conflict(left, right) {
                return true;
            }
        }
    }
    false
}

fn build_tool_access_profile<T>(info: &T, args: &Value) -> Option<ToolAccessProfile>
where
    T: ToolParallelismInfo,
{
    let kind = classify_tool_access_kind(info.original_name());
    let scope = resolve_tool_scope(info, args)?;
    Some(ToolAccessProfile { kind, scope })
}

fn classify_tool_access_kind(tool_name: &str) -> ToolAccessKind {
    if PARALLEL_PATH_WRITE_TOOLS.contains(&tool_name) {
        ToolAccessKind::Write
    } else {
        ToolAccessKind::Read
    }
}

fn resolve_tool_scope<T>(info: &T, args: &Value) -> Option<ToolScope>
where
    T: ToolParallelismInfo,
{
    let tool_name = info.original_name();
    let remote_default_locator = format!("remote:{}", info.server_name());
    match tool_name {
        "read_file" => extract_scoped_path(
            args,
            &["path"],
            None,
            &["connection_id", "remote_connection_id"],
            remote_default_locator.as_str(),
        ),
        "list_directory" => extract_scoped_path(
            args,
            &["path"],
            Some("."),
            &["connection_id", "remote_connection_id"],
            remote_default_locator.as_str(),
        ),
        "list_dir" | "search_text" => extract_scoped_path(
            args,
            &["path", "rel_path", "start_path"],
            Some("."),
            &["connection_id", "remote_connection_id"],
            "local",
        ),
        "read_file_raw" | "read_file_range" | "write_file" | "edit_file" => extract_scoped_path(
            args,
            &["path", "rel_path", "file_path", "target_path"],
            None,
            &["connection_id", "remote_connection_id"],
            "local",
        ),
        _ => {
            if PARALLEL_PATH_READ_TOOLS.contains(&tool_name) {
                extract_scoped_path(
                    args,
                    &["path", "rel_path", "file_path", "target_path", "start_path"],
                    None,
                    &["connection_id", "remote_connection_id"],
                    "local",
                )
            } else {
                Some(ToolScope::Global)
            }
        }
    }
}

fn extract_scoped_path(
    args: &Value,
    path_keys: &[&str],
    default_path: Option<&str>,
    locator_keys: &[&str],
    default_locator: &str,
) -> Option<ToolScope> {
    let path = first_non_empty_string(args, path_keys)
        .or_else(|| default_path.map(ToOwned::to_owned))
        .map(|raw| normalize_scope_path(raw.as_str()))?;
    let locator =
        first_non_empty_string(args, locator_keys).unwrap_or_else(|| default_locator.to_string());
    Some(ToolScope::Path {
        locator: normalize_scope_locator(locator.as_str()),
        path,
    })
}

fn first_non_empty_string(args: &Value, keys: &[&str]) -> Option<String> {
    let map = args.as_object()?;
    for key in keys {
        let value = map
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(value) = value {
            return Some(value.to_string());
        }
    }
    None
}

fn normalize_scope_locator(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "default".to_string()
    } else {
        trimmed.to_ascii_lowercase()
    }
}

fn normalize_scope_path(raw: &str) -> String {
    let mut segments = Vec::new();
    let normalized = raw.trim().replace('\\', "/");
    for part in normalized.split('/') {
        let segment = part.trim();
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            if !segments.is_empty() {
                segments.pop();
            }
            continue;
        }
        segments.push(segment);
    }
    if segments.is_empty() {
        ".".to_string()
    } else {
        segments.join("/")
    }
}

fn tool_profiles_conflict(left: &ToolAccessProfile, right: &ToolAccessProfile) -> bool {
    if left.kind == ToolAccessKind::Read && right.kind == ToolAccessKind::Read {
        return false;
    }
    tool_scopes_overlap(&left.scope, &right.scope)
}

fn tool_scopes_overlap(left: &ToolScope, right: &ToolScope) -> bool {
    match (left, right) {
        (ToolScope::Global, _) | (_, ToolScope::Global) => true,
        (
            ToolScope::Path {
                locator: left_locator,
                path: left_path,
            },
            ToolScope::Path {
                locator: right_locator,
                path: right_path,
            },
        ) => {
            left_locator == right_locator && paths_overlap(left_path.as_str(), right_path.as_str())
        }
    }
}

fn paths_overlap(left: &str, right: &str) -> bool {
    if left == "." || right == "." {
        return true;
    }
    left == right || is_path_prefix(left, right) || is_path_prefix(right, left)
}

fn is_path_prefix(path: &str, prefix: &str) -> bool {
    path.len() > prefix.len()
        && path.starts_with(prefix)
        && path.as_bytes().get(prefix.len()) == Some(&b'/')
}

#[cfg(test)]
mod tests;
