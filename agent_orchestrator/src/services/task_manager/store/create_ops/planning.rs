use crate::services::builtin_mcp::UI_PROMPTER_MCP_ID;
use crate::services::task_capability_registry::{
    infer_capability_mcp_ids_from_text, infer_default_capability_mcp_ids,
};
use crate::services::task_manager::types::TaskDraft;
use crate::services::task_service_client::{TaskContextAssetRefDto, TaskPlanningSnapshotDto};

use super::super::remote_support::TaskScopeContext;

type TurnRuntimeSnapshotDto = crate::services::memory_server_client::TurnRuntimeSnapshotDto;

pub(super) async fn build_task_planning_snapshot(
    session_id: &str,
    conversation_turn_id: &str,
    scope: &TaskScopeContext,
    contact_authorized_builtin_mcp_ids: &[String],
    runtime_snapshot: Option<&TurnRuntimeSnapshotDto>,
) -> TaskPlanningSnapshotDto {
    let turn_messages = fetch_turn_messages(session_id, conversation_turn_id)
        .await
        .unwrap_or_default();

    TaskPlanningSnapshotDto {
        contact_authorized_builtin_mcp_ids: contact_authorized_builtin_mcp_ids.to_vec(),
        selected_model_config_id: scope.model_config_id.clone(),
        source_user_goal_summary: build_source_user_goal_summary(turn_messages.as_slice()),
        source_constraints_summary: build_source_constraints_summary(
            turn_messages.as_slice(),
            scope,
            runtime_snapshot,
        ),
        planned_at: Some(crate::core::time::now_rfc3339()),
    }
}

pub(super) fn infer_task_builtin_mcp_ids(
    draft: &TaskDraft,
    scope: &TaskScopeContext,
    authorized_builtin_mcp_ids: &[String],
    runtime_snapshot: Option<&TurnRuntimeSnapshotDto>,
) -> Vec<String> {
    let mut out = draft.planned_builtin_mcp_ids.clone();

    let mut push_if_authorized = |mcp_id: &str| {
        if !authorized_builtin_mcp_ids.iter().any(|item| item == mcp_id) {
            return;
        }
        if !out.iter().any(|item| item == mcp_id) {
            out.push(mcp_id.to_string());
        }
    };

    if let Some(runtime) = runtime_snapshot.and_then(|snapshot| snapshot.runtime.as_ref()) {
        for enabled_mcp_id in &runtime.enabled_mcp_ids {
            let normalized = enabled_mcp_id.trim();
            if normalized.is_empty() || normalized == UI_PROMPTER_MCP_ID {
                continue;
            }
            push_if_authorized(normalized);
        }
    }

    let joined = format!("{}\n{}", draft.title, draft.details);
    let has_project_root = scope
        .project_root
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some();
    let has_remote_connection = scope
        .remote_connection_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some();

    for builtin_mcp_id in infer_default_capability_mcp_ids(
        authorized_builtin_mcp_ids,
        has_project_root,
        has_remote_connection,
    ) {
        push_if_authorized(builtin_mcp_id.as_str());
    }

    for builtin_mcp_id in infer_capability_mcp_ids_from_text(
        joined.as_str(),
        authorized_builtin_mcp_ids,
        has_project_root,
        has_remote_connection,
    ) {
        push_if_authorized(builtin_mcp_id.as_str());
    }

    out
}

pub(super) fn merge_runtime_selected_command_assets(
    draft_assets: &[TaskContextAssetRefDto],
    runtime_snapshot: Option<&TurnRuntimeSnapshotDto>,
) -> Vec<TaskContextAssetRefDto> {
    let mut out = draft_assets.to_vec();

    let Some(runtime) = runtime_snapshot.and_then(|snapshot| snapshot.runtime.as_ref()) else {
        return out;
    };

    for command in &runtime.selected_commands {
        let asset_id = command
            .command_ref
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| command.source_path.trim().to_string());
        if asset_id.is_empty() {
            continue;
        }
        let duplicated = out.iter().any(|existing| {
            existing.asset_type == "common"
                && (existing.asset_id == asset_id
                    || existing
                        .source_path
                        .as_deref()
                        .map(str::trim)
                        .unwrap_or("")
                        == command.source_path.trim())
        });
        if duplicated {
            continue;
        }
        out.push(TaskContextAssetRefDto {
            asset_type: "common".to_string(),
            asset_id,
            display_name: command.name.clone(),
            source_type: Some("runtime_command".to_string()),
            source_path: Some(command.source_path.clone()),
        });
    }

    out
}

async fn fetch_turn_messages(
    session_id: &str,
    conversation_turn_id: &str,
) -> Result<Vec<crate::models::message::Message>, String> {
    let turn_id = conversation_turn_id.trim();
    if turn_id.is_empty() {
        return Ok(Vec::new());
    }

    let messages =
        crate::services::memory_server_client::list_messages(session_id, Some(200), 0, true)
            .await?;
    Ok(messages
        .into_iter()
        .filter(|message| message_turn_id(message) == Some(turn_id))
        .collect())
}

fn message_turn_id(message: &crate::models::message::Message) -> Option<&str> {
    message
        .metadata
        .as_ref()
        .and_then(|meta| {
            meta.get("conversation_turn_id")
                .and_then(serde_json::Value::as_str)
        })
        .or_else(|| {
            message.metadata.as_ref().and_then(|meta| {
                meta.get("conversationTurnId")
                    .and_then(serde_json::Value::as_str)
            })
        })
}

fn build_source_user_goal_summary(messages: &[crate::models::message::Message]) -> Option<String> {
    let user_texts = messages
        .iter()
        .filter(|message| message.role.trim().eq_ignore_ascii_case("user"))
        .map(|message| message.content.trim())
        .filter(|content| !content.is_empty())
        .collect::<Vec<_>>();
    if user_texts.is_empty() {
        return None;
    }

    let joined = user_texts.join("\n");
    Some(limit_text(joined.as_str(), 600))
}

fn build_source_constraints_summary(
    messages: &[crate::models::message::Message],
    scope: &TaskScopeContext,
    runtime_snapshot: Option<&TurnRuntimeSnapshotDto>,
) -> Option<String> {
    let mut lines = Vec::new();

    let explicit_constraints = messages
        .iter()
        .filter(|message| message.role.trim().eq_ignore_ascii_case("user"))
        .flat_map(|message| message.content.lines())
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| {
            ["不要", "不能", "必须", "只", "优先", "限定", "通过", "使用"]
                .iter()
                .any(|keyword| line.contains(keyword))
        })
        .take(6)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if !explicit_constraints.is_empty() {
        lines.push("用户在本轮显式提出的约束:".to_string());
        lines.extend(
            explicit_constraints
                .iter()
                .map(|line| format!("- {}", limit_text(line.as_str(), 160))),
        );
    }

    lines.push(format!("- project_id={}", scope.project_id));
    if let Some(project_root) = scope
        .project_root
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("- project_root={}", project_root));
    }
    if let Some(remote_connection_id) = scope
        .remote_connection_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("- remote_connection_id={}", remote_connection_id));
    }

    if let Some(snapshot) = runtime_snapshot {
        if let Some(runtime) = snapshot.runtime.as_ref() {
            if !runtime.enabled_mcp_ids.is_empty() {
                lines.push(format!(
                    "- 本轮会话启用的 MCP={}",
                    runtime.enabled_mcp_ids.join(", ")
                ));
            }
            if !runtime.selected_commands.is_empty() {
                let selected = runtime
                    .selected_commands
                    .iter()
                    .filter_map(|item| {
                        item.name
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(ToOwned::to_owned)
                            .or_else(|| Some(item.source_path.trim().to_string()))
                    })
                    .take(6)
                    .collect::<Vec<_>>();
                if !selected.is_empty() {
                    lines.push(format!("- 本轮已选择命令/commons={}", selected.join(", ")));
                }
            }
        }
    }

    let text = lines
        .into_iter()
        .map(|line| limit_text(line.as_str(), 240))
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

fn limit_text(input: &str, max_chars: usize) -> String {
    let trimmed = input.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let truncated = trimmed.chars().take(max_chars).collect::<String>();
    format!("{}...", truncated)
}
