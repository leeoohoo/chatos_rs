// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;

use crate::config::Config;
use crate::core::internal_context_locale::InternalContextLocale;
use crate::services::{access_token_scope, task_runner_api_client};

#[derive(Debug, Deserialize)]
struct TaskPluginCatalogResponse {
    #[serde(default)]
    selectable_plugins: Vec<TaskPluginCandidate>,
}

#[derive(Debug, Deserialize)]
struct TaskPluginCandidate {
    plugin_key: String,
    display_name: String,
    description: String,
    version: String,
    #[serde(default)]
    components: Vec<TaskPluginComponent>,
    #[serde(default)]
    commands: Vec<TaskPluginCommand>,
}

#[derive(Debug, Deserialize)]
struct TaskPluginComponent {
    component_key: String,
    kind: String,
    #[serde(default)]
    available: bool,
}

#[derive(Debug, Deserialize)]
struct TaskPluginCommand {
    command_id: String,
    display_name: String,
    description: Option<String>,
}

pub(super) async fn resolve_task_plugin_catalog_prompt(
    project_id: &str,
    plan_mode: bool,
    preferred_plugin_keys: &[String],
    locale: InternalContextLocale,
) -> Result<Option<String>, String> {
    let access_token = access_token_scope::get_current_access_token()
        .ok_or_else(|| "current access token is unavailable for Task Plugin catalog".to_string())?;
    let config = Config::try_get()?;
    let payload = task_runner_api_client::list_task_runner_available_plugins(
        config.task_runner_base_url.as_str(),
        access_token.as_str(),
        project_id,
        plan_mode,
    )
    .await?;
    let catalog = serde_json::from_value::<TaskPluginCatalogResponse>(payload)
        .map_err(|error| format!("decode Task Plugin catalog failed: {error}"))?;
    Ok(compose_task_plugin_catalog_prompt(
        catalog.selectable_plugins.as_slice(),
        preferred_plugin_keys,
        locale,
    ))
}

fn compose_task_plugin_catalog_prompt(
    candidates: &[TaskPluginCandidate],
    preferred_plugin_keys: &[String],
    locale: InternalContextLocale,
) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }
    let english = locale.is_english();
    let mut lines = vec![if english {
        "Task Plugin Catalog".to_string()
    } else {
        "任务可用 Plugin 目录".to_string()
    }];
    lines.push(if english {
        "These Plugins may only be assigned to Tasks you create. You cannot call their tools directly. When creating a Task, select only Plugins that are necessary for that specific Task.".to_string()
    } else {
        "这些 Plugin 只能分配给你创建的 Task；你不能直接调用它们的工具。创建 Task 时，只能为该具体 Task 选择确实需要的 Plugin。".to_string()
    });
    let available_keys = candidates
        .iter()
        .map(|candidate| candidate.plugin_key.trim())
        .filter(|key| !key.is_empty())
        .collect::<std::collections::HashSet<_>>();
    let preferred = preferred_plugin_keys
        .iter()
        .map(|key| key.trim())
        .filter(|key| available_keys.contains(key))
        .fold(Vec::<&str>::new(), |mut keys, key| {
            if !keys.contains(&key) {
                keys.push(key);
            }
            keys
        });
    if !preferred.is_empty() {
        lines.push(if english {
            format!(
                "The user marked these as Task preferences: {}. Preferences are not authorization; include a plugin_hints entry only on each Task that actually needs it.",
                preferred.join(", ")
            )
        } else {
            format!(
                "用户标记了以下 Task Plugin 偏好：{}。偏好不是执行授权；仅在某个具体 Task 确实需要时，才为该 Task 写入 plugin_hints。",
                preferred.join("、")
            )
        });
    }
    for candidate in candidates {
        let plugin_key = candidate.plugin_key.trim();
        if plugin_key.is_empty() {
            continue;
        }
        lines.push(format!(
            "- {} (`{}`) v{}: {}",
            normalized_text(candidate.display_name.as_str(), plugin_key),
            plugin_key,
            normalized_text(candidate.version.as_str(), "unknown"),
            normalized_text(candidate.description.as_str(), "-")
        ));
        let components = candidate
            .components
            .iter()
            .filter(|component| component.available)
            .filter_map(|component| {
                let key = component.component_key.trim();
                (!key.is_empty()).then(|| {
                    format!(
                        "{}:{}",
                        normalized_text(component.kind.as_str(), "component"),
                        key
                    )
                })
            })
            .collect::<Vec<_>>();
        if !components.is_empty() {
            lines.push(format!(
                "  - {}: {}",
                if english {
                    "Capabilities"
                } else {
                    "能力组件"
                },
                components.join(", ")
            ));
        }
        for command in &candidate.commands {
            let command_id = command.command_id.trim();
            if command_id.is_empty() {
                continue;
            }
            lines.push(format!(
                "  - {} `{}`: {}{}",
                if english {
                    "Task command"
                } else {
                    "任务命令"
                },
                command_id,
                normalized_text(command.display_name.as_str(), command_id),
                command
                    .description
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| format!(" — {value}"))
                    .unwrap_or_default()
            ));
        }
    }
    (lines.len() > 2).then(|| lines.join("\n"))
}

fn normalized_text<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    let value = value.trim();
    if value.is_empty() {
        fallback
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_prompt_exposes_descriptions_without_runtime_identity() {
        let prompt = compose_task_plugin_catalog_prompt(
            &[TaskPluginCandidate {
                plugin_key: "open-computer-use".to_string(),
                display_name: "Open Computer Use".to_string(),
                description: "Operate local desktop applications.".to_string(),
                version: "0.3.1".to_string(),
                components: vec![TaskPluginComponent {
                    component_key: "computer-use-mcp".to_string(),
                    kind: "mcp".to_string(),
                    available: true,
                }],
                commands: vec![TaskPluginCommand {
                    command_id: "list-apps".to_string(),
                    display_name: "List Apps".to_string(),
                    description: Some("List local applications.".to_string()),
                }],
            }],
            &["open-computer-use".to_string()],
            InternalContextLocale::ZhCn,
        )
        .expect("catalog prompt");

        assert!(prompt.contains("open-computer-use"));
        assert!(prompt.contains("Task Plugin 偏好"));
        assert!(prompt.contains("不能直接调用"));
        assert!(prompt.contains("Operate local desktop applications"));
        assert!(!prompt.contains("device_id"));
        assert!(!prompt.contains("release_id"));
        assert!(!prompt.contains("artifact_sha256"));
        assert!(!prompt.contains("mcp_url"));
    }
}
