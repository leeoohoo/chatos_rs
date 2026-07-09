// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
pub(super) fn local_connector_directory_list_payload(path: &str, value: Value) -> Value {
    let mut entries = value
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|entry| {
            let is_dir = entry
                .get("is_dir")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| entry.get("type").and_then(Value::as_str) == Some("dir"));
            json!({
                "name": entry.get("name").cloned().unwrap_or(Value::Null),
                "path": entry.get("path").cloned().unwrap_or(Value::Null),
                "is_dir": is_dir,
                "len": entry
                    .get("len")
                    .or_else(|| entry.get("size"))
                    .cloned()
                    .unwrap_or(Value::Null),
            })
        })
        .collect::<Vec<_>>();
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
    json!({
        "path": if path.trim().is_empty() { "." } else { path },
        "parent": Value::Null,
        "entries": entries,
    })
}
