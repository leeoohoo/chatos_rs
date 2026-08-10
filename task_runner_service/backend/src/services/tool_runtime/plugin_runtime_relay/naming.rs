// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn plugin_prompt_sort_key(value: &Value) -> (u8, String) {
    let text = value
        .pointer("/content/0/text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let rank = if text.contains("[Plugin Skill:") {
        0
    } else if text.contains("[Plugin Command:") {
        1
    } else if text.contains("[Plugin Agent Profile:") {
        2
    } else {
        3
    };
    (rank, text.to_string())
}

pub(super) fn plugin_server_name(
    plugin: &RunPluginSnapshot,
    component: &RunPluginComponentSnapshot,
) -> String {
    plugin_server_name_from_identity(plugin.plugin_id.as_str(), component.component_key.as_str())
}

pub(super) fn plugin_server_name_from_identity(plugin_id: &str, component_key: &str) -> String {
    let raw = format!("plugin_{plugin_id}_{component_key}");
    let mut normalized = raw
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    while normalized.contains("__") {
        normalized = normalized.replace("__", "_");
    }
    normalized.trim_matches('_').chars().take(96).collect()
}
