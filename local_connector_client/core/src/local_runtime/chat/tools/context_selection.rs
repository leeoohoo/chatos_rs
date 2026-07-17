// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_runtime::{
    builtin_kind_by_config_id, complete_builtin_kind_dependencies, BuiltinMcpKind,
};

use crate::mcp::manifest::LocalMcpManifestRecord;

const LOCAL_CHAT_BUILTINS: [BuiltinMcpKind; 6] = [
    BuiltinMcpKind::CodeMaintainerRead,
    BuiltinMcpKind::CodeMaintainerWrite,
    BuiltinMcpKind::TerminalController,
    BuiltinMcpKind::BrowserTools,
    BuiltinMcpKind::TaskManager,
    BuiltinMcpKind::AskUser,
];

pub(super) fn parse_selected_ids(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw)
        .unwrap_or_default()
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .fold(Vec::new(), |mut values, value| {
            if !values.contains(&value) {
                values.push(value);
            }
            values
        })
}

pub(super) fn selected_chat_builtin_kinds(
    mcp_enabled: bool,
    plan_mode_enabled: bool,
    selected_ids: &[String],
) -> Vec<BuiltinMcpKind> {
    if plan_mode_enabled {
        return vec![BuiltinMcpKind::ProjectManagement];
    }
    if !mcp_enabled {
        return Vec::new();
    }
    selected_builtin_kinds(selected_ids)
}

pub(super) fn manifest_is_selected(
    manifest: &LocalMcpManifestRecord,
    selected_ids: &[String],
) -> bool {
    selected_ids.is_empty()
        || selected_ids.iter().any(|selected| {
            selected == &manifest.manifest_id
                || manifest.plugin_mcp_id.as_deref() == Some(selected.as_str())
        })
}

fn selected_builtin_kinds(selected_ids: &[String]) -> Vec<BuiltinMcpKind> {
    if selected_ids.is_empty() {
        return LOCAL_CHAT_BUILTINS.to_vec();
    }
    complete_builtin_kind_dependencies(
        selected_ids
            .iter()
            .filter_map(|id| builtin_kind_by_config_id(id.as_str()))
            .filter(|kind| LOCAL_CHAT_BUILTINS.contains(kind)),
    )
}

#[cfg(test)]
mod tests {
    use chatos_mcp_runtime::BuiltinMcpKind;

    use super::selected_chat_builtin_kinds;

    #[test]
    fn normal_chat_defaults_exclude_project_management() {
        let kinds = selected_chat_builtin_kinds(true, false, &[]);
        assert_eq!(
            kinds,
            vec![
                BuiltinMcpKind::CodeMaintainerRead,
                BuiltinMcpKind::CodeMaintainerWrite,
                BuiltinMcpKind::TerminalController,
                BuiltinMcpKind::BrowserTools,
                BuiltinMcpKind::TaskManager,
                BuiltinMcpKind::AskUser,
            ]
        );
        assert!(!kinds.contains(&BuiltinMcpKind::ProjectManagement));
    }

    #[test]
    fn plan_mode_exposes_only_project_management() {
        assert_eq!(
            selected_chat_builtin_kinds(true, true, &["builtin_browser_tools".to_string()]),
            vec![BuiltinMcpKind::ProjectManagement]
        );
    }

    #[test]
    fn explicit_write_selection_adds_read_dependency() {
        assert_eq!(
            selected_chat_builtin_kinds(
                true,
                false,
                &["builtin_code_maintainer_write".to_string()],
            ),
            vec![
                BuiltinMcpKind::CodeMaintainerRead,
                BuiltinMcpKind::CodeMaintainerWrite,
            ]
        );
    }
}
