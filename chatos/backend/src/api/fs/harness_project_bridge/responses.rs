// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn list_response(
    path: &HarnessProjectPath,
    value: Value,
    include_files: bool,
) -> (StatusCode, Json<Value>) {
    let mut entries = value
        .get("entries")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|entry| normalize_entry(path.project_id.as_str(), entry, include_files))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    sort_entries(entries.as_mut_slice());
    (
        StatusCode::OK,
        Json(json!({
            "path": logical_path(path.project_id.as_str(), path.relative_path.as_str()),
            "display_path": logical_path(path.project_id.as_str(), path.relative_path.as_str()),
            "parent": parent_logical_path(path),
            "writable": true,
            "entries": entries,
            "roots": Vec::<Value>::new(),
            "harness_project": true,
        })),
    )
}

pub(super) fn normalize_entry(
    project_id: &str,
    entry: &Value,
    include_files: bool,
) -> Option<Value> {
    let is_dir = entry.get("type").and_then(Value::as_str) == Some("dir");
    if !is_dir && !include_files {
        return None;
    }
    let name = entry
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if name == ".gitkeep" {
        return None;
    }
    let relative_path = entry.get("path").and_then(Value::as_str)?;
    let path = logical_path(project_id, relative_path);
    Some(json!({
        "name": name,
        "path": path,
        "display_path": path,
        "is_dir": is_dir,
        "writable": true,
        "size": entry.get("size").cloned().unwrap_or(Value::Null),
        "modified_at": Value::Null,
    }))
}

pub(super) fn read_response(path: &HarnessProjectPath, value: Value) -> (StatusCode, Json<Value>) {
    let relative = value
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or(path.relative_path.as_str());
    let content = value
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let name = relative
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or("");
    let content_type = mime_guess::from_path(relative)
        .first_or_text_plain()
        .essence_str()
        .to_string();
    let is_binary = value.get("content_encoding").and_then(Value::as_str) == Some("base64");
    (
        StatusCode::OK,
        Json(json!({
            "path": logical_path(path.project_id.as_str(), relative),
            "display_path": logical_path(path.project_id.as_str(), relative),
            "name": name,
            "size": value
                .get("size_bytes")
                .and_then(Value::as_u64)
                .unwrap_or(content.len() as u64),
            "content_type": content_type,
            "is_binary": is_binary,
            "writable": true,
            "modified_at": Value::Null,
            "content": content,
            "harness_project": true,
        })),
    )
}

pub(super) fn search_entries_response(
    path: &HarnessProjectPath,
    value: Value,
    query: &str,
) -> (StatusCode, Json<Value>) {
    let entries = value
        .get("matches")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|entry| normalize_entry(path.project_id.as_str(), entry, true))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    (
        StatusCode::OK,
        Json(json!({
            "path": logical_path(path.project_id.as_str(), path.relative_path.as_str()),
            "query": query,
            "entries": entries,
            "truncated": value.get("truncated").and_then(Value::as_bool).unwrap_or(false),
            "visited_dirs": value.get("visited_dirs").and_then(Value::as_u64).unwrap_or(0),
            "harness_project": true,
        })),
    )
}

pub(super) fn search_content_response(
    path: &HarnessProjectPath,
    value: Value,
    query: &str,
) -> (StatusCode, Json<Value>) {
    let entries = value
        .get("results")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let relative_path = item.get("path").and_then(Value::as_str)?;
                    let text = item.get("text").and_then(Value::as_str).unwrap_or_default();
                    let column = text
                        .find(query)
                        .map(|offset| text[..offset].chars().count() + 1)
                        .unwrap_or(1);
                    Some(json!({
                        "path": logical_path(path.project_id.as_str(), relative_path),
                        "relative_path": relative_path,
                        "line": item.get("line").and_then(Value::as_u64).unwrap_or(1),
                        "column": column,
                        "text": text,
                    }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let returned_count = entries.len();
    (
        StatusCode::OK,
        Json(json!({
            "path": logical_path(path.project_id.as_str(), path.relative_path.as_str()),
            "query": query,
            "entries": entries,
            "truncated": value.get("count").and_then(Value::as_u64).unwrap_or(0) > returned_count as u64,
            "visited_dirs": value.get("scanned_files").and_then(Value::as_u64).unwrap_or(0),
            "harness_project": true,
        })),
    )
}

pub(super) fn mutation_response(
    project_id: &str,
    value: Value,
    fallback_name: &str,
    created: bool,
) -> (StatusCode, Json<Value>) {
    let result = value.get("result").unwrap_or(&value);
    let relative = result
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let name = relative
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(fallback_name);
    (
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(json!({
            "success": true,
            "path": logical_path(project_id, relative),
            "display_path": logical_path(project_id, relative),
            "name": name,
            "size": result.get("bytes").or_else(|| result.get("size")).cloned().unwrap_or(Value::Null),
            "created": created,
            "modified_at": Value::Null,
            "harness_project": true,
        })),
    )
}

pub(super) fn created_response(
    project_id: &str,
    relative: &str,
    name: &str,
    created: bool,
) -> (StatusCode, Json<Value>) {
    (
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(json!({
            "success": true,
            "path": logical_path(project_id, relative),
            "display_path": logical_path(project_id, relative),
            "name": name,
            "created": created,
            "harness_project": true,
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_harness_file_sizes_in_directory_entries() {
        let entry = json!({
            "name": "diagram.png",
            "path": "docs/diagram.png",
            "type": "file",
            "size": 128
        });

        let normalized = normalize_entry("project-1", &entry, true).expect("entry");

        assert_eq!(normalized["size"], 128);
    }

    #[test]
    fn maps_base64_image_content_to_a_binary_frontend_preview() {
        let path = HarnessProjectPath {
            project_id: "project-1".to_string(),
            relative_path: "docs/diagram.svg".to_string(),
        };

        let (_, Json(response)) = read_response(
            &path,
            json!({
                "path": "docs/diagram.svg",
                "size_bytes": 42,
                "content_encoding": "base64",
                "content": "PHN2Zz48L3N2Zz4="
            }),
        );

        assert_eq!(response["content_type"], "image/svg+xml");
        assert_eq!(response["is_binary"], true);
        assert_eq!(response["content"], "PHN2Zz48L3N2Zz4=");
    }
}

pub(super) fn parse_harness_project_path(raw_path: &str) -> Option<HarnessProjectPath> {
    let url = Url::parse(raw_path.trim()).ok()?;
    if url.scheme() != HARNESS_PROJECT_SCHEME || url.host_str()? != HARNESS_PROJECT_HOST {
        return None;
    }
    let mut segments = url.path_segments()?;
    let project_id = segments.next()?.trim().to_string();
    if project_id.is_empty() {
        return None;
    }
    let relative_path = segments
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/");
    Some(HarnessProjectPath {
        project_id,
        relative_path,
    })
}

pub(super) fn harness_relative_arg(path: &HarnessProjectPath) -> String {
    if path.relative_path.is_empty() {
        ".".to_string()
    } else {
        path.relative_path.clone()
    }
}

pub(super) fn child_relative_path(parent: &HarnessProjectPath, name: &str) -> String {
    if parent.relative_path.is_empty() {
        name.to_string()
    } else {
        format!("{}/{name}", parent.relative_path)
    }
}

pub(super) fn logical_path(project_id: &str, relative_path: &str) -> String {
    let root = harness_project_root_path(project_id);
    let relative_path = relative_path.trim_matches('/');
    if relative_path.is_empty() || relative_path == "." {
        root
    } else {
        format!("{root}/{relative_path}")
    }
}

pub(super) fn parent_logical_path(path: &HarnessProjectPath) -> Value {
    if path.relative_path.is_empty() {
        return Value::Null;
    }
    let parent = path
        .relative_path
        .rsplit_once('/')
        .map(|(value, _)| value)
        .unwrap_or("");
    Value::String(logical_path(path.project_id.as_str(), parent))
}

pub(super) fn sort_entries(entries: &mut [Value]) {
    entries.sort_by(|left, right| {
        let left_dir = left.get("is_dir").and_then(Value::as_bool).unwrap_or(false);
        let right_dir = right
            .get("is_dir")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if left_dir != right_dir {
            return right_dir.cmp(&left_dir);
        }
        let left_name = left
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        let right_name = right
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        left_name.cmp(&right_name)
    });
}
