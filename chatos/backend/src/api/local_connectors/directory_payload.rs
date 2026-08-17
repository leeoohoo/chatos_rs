// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
pub(super) fn local_connector_directory_list_payload(path: &str, value: Value) -> Value {
    let response_path = value
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| if path.trim().is_empty() { "." } else { path });
    let parent = value.get("parent").cloned().unwrap_or(Value::Null);
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
        "path": response_path,
        "parent": parent,
        "entries": entries,
    })
}

#[cfg(test)]
mod tests {
    use super::local_connector_directory_list_payload;
    use serde_json::json;

    #[test]
    fn preserves_control_plane_path_and_parent() {
        let payload = local_connector_directory_list_payload(
            ".",
            json!({
                "path": "apps",
                "parent": ".",
                "entries": [{ "name": "backend", "path": "apps/backend", "is_dir": true }],
            }),
        );

        assert_eq!(payload.get("path"), Some(&json!("apps")));
        assert_eq!(payload.get("parent"), Some(&json!(".")));
        assert_eq!(
            payload.pointer("/entries/0/path"),
            Some(&json!("apps/backend"))
        );
    }
}
