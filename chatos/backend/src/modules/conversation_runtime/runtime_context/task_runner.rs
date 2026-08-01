// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{PluginAgentSelection, PluginCommandInvocation};

const MAX_PLUGIN_COMMAND_INVOCATIONS: usize = 64;
const MAX_PLUGIN_COMMAND_ARGUMENT_BYTES: usize = 16 * 1024;
const MAX_PLUGIN_COMPONENT_ID_BYTES: usize = 256;

pub(super) fn normalize_plugin_agent_selection(
    selected_plugin_ids: &[String],
    selection: Option<&PluginAgentSelection>,
) -> Option<PluginAgentSelection> {
    let selection = selection?;
    let plugin_id = selection.plugin_id.trim();
    let agent_id = selection.agent_id.trim();
    if plugin_id.is_empty()
        || agent_id.is_empty()
        || plugin_id.len() > MAX_PLUGIN_COMPONENT_ID_BYTES
        || agent_id.len() > MAX_PLUGIN_COMPONENT_ID_BYTES
        || plugin_id.contains('\0')
        || agent_id.contains('\0')
        || !selected_plugin_ids
            .iter()
            .any(|selected| selected == plugin_id)
    {
        return None;
    }
    Some(PluginAgentSelection {
        plugin_id: plugin_id.to_string(),
        agent_id: agent_id.to_string(),
    })
}

pub(super) fn normalize_selected_plugin_ids(selected_plugin_ids: &[String]) -> Vec<String> {
    let mut normalized_plugin_ids = Vec::new();
    for plugin_id in selected_plugin_ids {
        let plugin_id = plugin_id.trim();
        if plugin_id.is_empty()
            || normalized_plugin_ids
                .iter()
                .any(|existing: &String| existing == plugin_id)
        {
            continue;
        }
        normalized_plugin_ids.push(plugin_id.to_string());
    }
    normalized_plugin_ids
}

pub(super) fn normalize_plugin_command_invocations(
    selected_plugin_ids: &[String],
    invocations: &[PluginCommandInvocation],
) -> Vec<PluginCommandInvocation> {
    let mut normalized = Vec::new();
    for invocation in invocations {
        if normalized.len() >= MAX_PLUGIN_COMMAND_INVOCATIONS {
            break;
        }
        let plugin_id = invocation.plugin_id.trim();
        let command_id = invocation.command_id.trim();
        if plugin_id.is_empty()
            || command_id.is_empty()
            || !selected_plugin_ids
                .iter()
                .any(|selected| selected == plugin_id)
            || normalized.iter().any(|existing: &PluginCommandInvocation| {
                existing.plugin_id == plugin_id && existing.command_id == command_id
            })
        {
            continue;
        }
        let arguments = invocation
            .arguments
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if arguments.is_some_and(|value| {
            value.contains('\0') || value.len() > MAX_PLUGIN_COMMAND_ARGUMENT_BYTES
        }) {
            continue;
        }
        normalized.push(PluginCommandInvocation {
            plugin_id: plugin_id.to_string(),
            command_id: command_id.to_string(),
            arguments: arguments.map(str::to_string),
        });
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_selection_is_normalized_for_snapshots() {
        let selected = normalize_selected_plugin_ids(&[
            " plugin-a ".to_string(),
            "plugin-a".to_string(),
            "plugin-b".to_string(),
        ]);
        assert_eq!(selected, vec!["plugin-a", "plugin-b"]);

        let selection = normalize_plugin_agent_selection(
            selected.as_slice(),
            Some(&PluginAgentSelection {
                plugin_id: " plugin-a ".to_string(),
                agent_id: " reviewer ".to_string(),
            }),
        )
        .expect("normalized selection");
        assert_eq!(selection.plugin_id, "plugin-a");
        assert_eq!(selection.agent_id, "reviewer");
    }

    #[test]
    fn command_invocations_fail_closed_on_invalid_arguments() {
        let normalized = normalize_plugin_command_invocations(
            &["plugin-a".to_string()],
            &[
                PluginCommandInvocation {
                    plugin_id: "plugin-a".to_string(),
                    command_id: "review".to_string(),
                    arguments: Some(" valid ".to_string()),
                },
                PluginCommandInvocation {
                    plugin_id: "plugin-a".to_string(),
                    command_id: "unsafe".to_string(),
                    arguments: Some("invalid\0argument".to_string()),
                },
            ],
        );
        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].command_id, "review");
        assert_eq!(normalized[0].arguments.as_deref(), Some("valid"));
    }
}
