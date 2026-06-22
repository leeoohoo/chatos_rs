use std::collections::HashMap;

use serde_json::Value;

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

pub fn should_parallelize_tool_batch(
    tool_calls: &[Value],
    tool_metadata: &HashMap<String, ToolInfo>,
) -> bool {
    if tool_calls.len() <= 1 {
        return false;
    }

    let mut profiles = Vec::with_capacity(tool_calls.len());
    for tool_call in tool_calls {
        let Some(prefixed_name) = extract_tool_call_name(tool_call) else {
            return false;
        };
        let Some(info) = tool_metadata.get(prefixed_name) else {
            return false;
        };
        if !PARALLEL_SAFE_TOOLS
            .iter()
            .any(|name| *name == info.original_name.as_str())
        {
            return false;
        }
        let Ok(args) = parse_tool_args(clone_tool_call_arguments(tool_call)) else {
            return false;
        };
        let Some(profile) = build_tool_access_profile(info, &args) else {
            return false;
        };
        profiles.push(profile);
    }

    !has_conflicting_tool_profiles(profiles.as_slice())
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

fn tool_profiles_conflict(left: &ToolAccessProfile, right: &ToolAccessProfile) -> bool {
    match (&left.scope, &right.scope) {
        (ToolScope::Global, ToolScope::Global) => false,
        (ToolScope::Global, ToolScope::Path { .. }) => false,
        (ToolScope::Path { .. }, ToolScope::Global) => false,
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
            if left_locator != right_locator {
                return false;
            }
            if !paths_overlap(left_path, right_path) {
                return false;
            }
            left.kind == ToolAccessKind::Write || right.kind == ToolAccessKind::Write
        }
    }
}

fn paths_overlap(left: &str, right: &str) -> bool {
    if left == "." || right == "." {
        return true;
    }
    left == right || is_path_prefix(left, right) || is_path_prefix(right, left)
}

fn is_path_prefix(parent: &str, child: &str) -> bool {
    let parent = parent.trim_end_matches('/');
    let child = child.trim_end_matches('/');
    child
        .strip_prefix(parent)
        .is_some_and(|rest| rest.starts_with('/'))
}

fn build_tool_access_profile(info: &ToolInfo, args: &Value) -> Option<ToolAccessProfile> {
    let kind = if PARALLEL_PATH_WRITE_TOOLS
        .iter()
        .any(|name| *name == info.original_name.as_str())
    {
        ToolAccessKind::Write
    } else {
        ToolAccessKind::Read
    };
    let scope = resolve_tool_scope(info, args)?;
    Some(ToolAccessProfile { kind, scope })
}

fn resolve_tool_scope(info: &ToolInfo, args: &Value) -> Option<ToolScope> {
    let tool_name = info.original_name.as_str();
    let remote_default_locator = format!("remote:{}", info.server_name);
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
            if PARALLEL_PATH_READ_TOOLS
                .iter()
                .any(|name| *name == tool_name)
            {
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

fn normalize_scope_path(path: &str) -> String {
    let normalized = path.trim().replace('\\', "/");
    let trimmed = normalized.trim_matches('/');
    if trimmed.is_empty() {
        ".".to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_scope_locator(locator: &str) -> String {
    locator.trim().to_ascii_lowercase()
}

fn parse_tool_args(args: Value) -> Result<Value, serde_json::Error> {
    if let Some(raw) = args.as_str() {
        serde_json::from_str::<Value>(raw)
    } else {
        Ok(args)
    }
}
