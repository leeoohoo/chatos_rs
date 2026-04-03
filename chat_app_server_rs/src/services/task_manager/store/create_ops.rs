use crate::services::task_manager::normalizer::{normalize_task_drafts, trimmed_non_empty};
use crate::services::task_manager::types::{TaskDraft, TaskRecord};
use crate::services::task_service_client::{
    self, CreateTaskRequestDto, TaskContextAssetRefDto, TaskExecutionResultContractDto,
    TaskPlanningSnapshotDto,
};
use tracing::warn;

use super::remote_support::{
    map_remote_result_brief, map_remote_task_to_record, resolve_task_scope_context,
    TaskScopeContext,
};

fn planned_builtin_requires_project_root(id: &str) -> bool {
    matches!(
        id.trim(),
        "builtin_code_maintainer_read"
            | "builtin_code_maintainer_write"
            | "builtin_code_maintainer"
            | "builtin_terminal_controller"
    )
}

fn planned_builtin_requires_remote_connection(id: &str) -> bool {
    matches!(id.trim(), "builtin_remote_connection_controller")
}

fn ensure_planned_builtin_mcp_ids_present(
    planned_builtin_mcp_ids: &[String],
) -> Result<(), String> {
    if planned_builtin_mcp_ids.is_empty() {
        Err("planned_builtin_mcp_ids is required and cannot be empty".to_string())
    } else {
        Ok(())
    }
}

fn ensure_planned_builtin_mcp_ids_authorized(
    planned_builtin_mcp_ids: &[String],
    authorized_builtin_mcp_ids: &[String],
) -> Result<(), String> {
    let unauthorized = planned_builtin_mcp_ids
        .iter()
        .filter(|item| {
            !authorized_builtin_mcp_ids
                .iter()
                .any(|allowed| allowed == *item)
        })
        .cloned()
        .collect::<Vec<_>>();

    if unauthorized.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "planned_builtin_mcp_ids contains unauthorized items: {}. allowed={}",
            unauthorized.join(", "),
            authorized_builtin_mcp_ids.join(", ")
        ))
    }
}

fn ensure_runtime_requirements(
    planned_builtin_mcp_ids: &[String],
    scope: &TaskScopeContext,
) -> Result<(), String> {
    if planned_builtin_mcp_ids
        .iter()
        .any(|item| planned_builtin_requires_project_root(item))
        && scope
            .project_root
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return Err(
            "当前任务计划使用查看/读写/终端能力，但当前会话没有可用的 project_root".to_string(),
        );
    }

    if planned_builtin_mcp_ids
        .iter()
        .any(|item| planned_builtin_requires_remote_connection(item))
        && scope
            .remote_connection_id
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return Err(
            "当前任务计划使用远程连接能力，但当前会话没有选中的 remote_connection_id".to_string(),
        );
    }

    Ok(())
}

async fn resolve_contact_builtin_mcp_grants(scope: &TaskScopeContext) -> Vec<String> {
    let Ok(contacts) = crate::services::memory_server_client::list_memory_contacts(
        Some(scope.user_id.as_str()),
        Some(500),
        0,
    )
    .await
    else {
        return Vec::new();
    };

    contacts
        .into_iter()
        .find(|contact| contact.agent_id == scope.contact_agent_id)
        .map(|contact| contact.authorized_builtin_mcp_ids)
        .unwrap_or_default()
}

fn normalize_asset_type(asset_type: &str) -> Option<&'static str> {
    match asset_type.trim().to_ascii_lowercase().as_str() {
        "skill" => Some("skill"),
        "plugin" => Some("plugin"),
        "common" | "commons" => Some("common"),
        _ => None,
    }
}

fn resolve_runtime_skill<'a>(
    runtime_context: &'a crate::services::memory_server_client::MemoryAgentRuntimeContextDto,
    asset: &TaskContextAssetRefDto,
) -> Option<&'a crate::services::memory_server_client::MemoryAgentRuntimeSkillSummaryDto> {
    let asset_id = asset.asset_id.trim();
    let source_path = asset.source_path.as_deref().map(str::trim).unwrap_or("");
    let display_name = asset.display_name.as_deref().map(str::trim).unwrap_or("");

    runtime_context.runtime_skills.iter().find(|item| {
        item.id.trim() == asset_id
            || (!source_path.is_empty()
                && item.source_path.as_deref().map(str::trim).unwrap_or("") == source_path)
            || (!display_name.is_empty() && item.name.trim() == display_name)
    })
}

async fn hydrate_context_assets(
    assets: &[TaskContextAssetRefDto],
    runtime_context: &crate::services::memory_server_client::MemoryAgentRuntimeContextDto,
) -> Result<Vec<TaskContextAssetRefDto>, String> {
    let mut out = Vec::new();

    for asset in assets {
        let Some(asset_type) = normalize_asset_type(asset.asset_type.as_str()) else {
            return Err(format!(
                "unsupported planned_context_assets.asset_type: {}",
                asset.asset_type
            ));
        };

        let hydrated = match asset_type {
            "skill" => {
                let skill = if let Some(skill) = resolve_runtime_skill(runtime_context, asset) {
                    skill
                } else if let Some(full_skill) =
                    crate::services::memory_server_client::get_memory_skill(asset.asset_id.as_str())
                        .await?
                {
                    resolve_runtime_skill(
                        runtime_context,
                        &TaskContextAssetRefDto {
                            asset_type: "skill".to_string(),
                            asset_id: full_skill.id,
                            display_name: Some(full_skill.name),
                            source_type: Some("skill_center".to_string()),
                            source_path: Some(full_skill.source_path),
                        },
                    )
                    .ok_or_else(|| {
                        format!(
                            "planned_context_assets skill not found in current contact runtime: {}",
                            asset.asset_id
                        )
                    })?
                } else {
                    return Err(format!(
                        "planned_context_assets skill not found in current contact runtime: {}",
                        asset.asset_id
                    ));
                };
                TaskContextAssetRefDto {
                    asset_type: "skill".to_string(),
                    asset_id: skill.id.clone(),
                    display_name: asset
                        .display_name
                        .clone()
                        .or_else(|| Some(skill.name.clone())),
                    source_type: asset
                        .source_type
                        .clone()
                        .or_else(|| Some(skill.source_type.clone())),
                    source_path: asset
                        .source_path
                        .clone()
                        .or_else(|| skill.source_path.clone()),
                }
            }
            "plugin" => {
                let plugin = runtime_context
                    .runtime_plugins
                    .iter()
                    .find(|item| item.source.trim() == asset.asset_id.trim())
                    .ok_or_else(|| {
                        format!(
                            "planned_context_assets plugin not found in current contact runtime: {}",
                            asset.asset_id
                        )
                    })?;
                TaskContextAssetRefDto {
                    asset_type: "plugin".to_string(),
                    asset_id: plugin.source.clone(),
                    display_name: asset
                        .display_name
                        .clone()
                        .or_else(|| Some(plugin.name.clone())),
                    source_type: asset
                        .source_type
                        .clone()
                        .or_else(|| Some("plugin".to_string())),
                    source_path: asset.source_path.clone(),
                }
            }
            "common" => {
                let command = runtime_context
                    .runtime_commands
                    .iter()
                    .find(|item| {
                        item.command_ref.trim() == asset.asset_id.trim()
                            || item.source_path.trim() == asset.asset_id.trim()
                    })
                    .ok_or_else(|| {
                        format!(
                            "planned_context_assets common not found in current contact runtime: {}",
                            asset.asset_id
                        )
                    })?;
                TaskContextAssetRefDto {
                    asset_type: "common".to_string(),
                    asset_id: command.command_ref.clone(),
                    display_name: asset
                        .display_name
                        .clone()
                        .or_else(|| Some(command.name.clone())),
                    source_type: asset
                        .source_type
                        .clone()
                        .or_else(|| Some("runtime_command".to_string())),
                    source_path: asset
                        .source_path
                        .clone()
                        .or_else(|| Some(command.source_path.clone())),
                }
            }
            _ => unreachable!(),
        };

        let duplicated = out.iter().any(|existing: &TaskContextAssetRefDto| {
            existing.asset_type == hydrated.asset_type && existing.asset_id == hydrated.asset_id
        });
        if !duplicated {
            out.push(hydrated);
        }
    }

    Ok(out)
}

async fn build_task_planning_snapshot(
    session_id: &str,
    conversation_turn_id: &str,
    scope: &TaskScopeContext,
    contact_authorized_builtin_mcp_ids: &[String],
) -> TaskPlanningSnapshotDto {
    let turn_messages = fetch_turn_messages(session_id, conversation_turn_id)
        .await
        .unwrap_or_default();
    let runtime_snapshot =
        crate::services::memory_server_client::get_turn_runtime_snapshot_by_turn(
            session_id,
            conversation_turn_id,
        )
        .await
        .ok()
        .and_then(|payload| payload.snapshot);

    TaskPlanningSnapshotDto {
        contact_authorized_builtin_mcp_ids: contact_authorized_builtin_mcp_ids.to_vec(),
        selected_model_config_id: scope.model_config_id.clone(),
        source_user_goal_summary: build_source_user_goal_summary(turn_messages.as_slice()),
        source_constraints_summary: build_source_constraints_summary(
            turn_messages.as_slice(),
            scope,
            runtime_snapshot.as_ref(),
        ),
        planned_at: Some(crate::core::time::now_rfc3339()),
    }
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
    runtime_snapshot: Option<&crate::services::memory_server_client::TurnRuntimeSnapshotDto>,
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

pub async fn create_tasks_for_turn(
    session_id: &str,
    conversation_turn_id: &str,
    draft_tasks: Vec<TaskDraft>,
) -> Result<Vec<TaskRecord>, String> {
    let session_id = trimmed_non_empty(session_id)
        .ok_or_else(|| "session_id is required".to_string())?
        .to_string();
    let conversation_turn_id = trimmed_non_empty(conversation_turn_id)
        .ok_or_else(|| "conversation_turn_id is required".to_string())?
        .to_string();
    let draft_tasks = normalize_task_drafts(draft_tasks)?;
    if draft_tasks.is_empty() {
        return Ok(Vec::new());
    }

    let scope = resolve_task_scope_context(session_id.as_str()).await?;
    let contact_authorized_builtin_mcp_ids = resolve_contact_builtin_mcp_grants(&scope).await;
    let runtime_context = crate::services::memory_server_client::get_memory_agent_runtime_context(
        scope.contact_agent_id.as_str(),
    )
    .await?
    .ok_or_else(|| {
        format!(
            "agent runtime context not found: {}",
            scope.contact_agent_id
        )
    })?;
    let planning_snapshot = build_task_planning_snapshot(
        session_id.as_str(),
        conversation_turn_id.as_str(),
        &scope,
        contact_authorized_builtin_mcp_ids.as_slice(),
    )
    .await;
    let mut out = Vec::with_capacity(draft_tasks.len());
    for mut draft in draft_tasks {
        ensure_planned_builtin_mcp_ids_present(draft.planned_builtin_mcp_ids.as_slice())?;
        ensure_planned_builtin_mcp_ids_authorized(
            draft.planned_builtin_mcp_ids.as_slice(),
            contact_authorized_builtin_mcp_ids.as_slice(),
        )?;
        ensure_runtime_requirements(draft.planned_builtin_mcp_ids.as_slice(), &scope)?;
        draft.planned_context_assets =
            hydrate_context_assets(draft.planned_context_assets.as_slice(), &runtime_context)
                .await?;
        let created = task_service_client::create_task(&CreateTaskRequestDto {
            user_id: Some(scope.user_id.clone()),
            contact_agent_id: scope.contact_agent_id.clone(),
            project_id: scope.project_id.clone(),
            project_root: scope.project_root.clone(),
            remote_connection_id: scope.remote_connection_id.clone(),
            session_id: Some(session_id.clone()),
            conversation_turn_id: Some(conversation_turn_id.clone()),
            source_message_id: None,
            model_config_id: scope.model_config_id.clone(),
            title: draft.title.clone(),
            content: if draft.details.trim().is_empty() {
                draft.title.clone()
            } else {
                draft.details.clone()
            },
            priority: Some(draft.priority.clone()),
            confirm_note: None,
            execution_note: None,
            planned_builtin_mcp_ids: draft.planned_builtin_mcp_ids.clone(),
            planned_context_assets: draft.planned_context_assets.clone(),
            execution_result_contract: draft.execution_result_contract.clone().or(Some(
                TaskExecutionResultContractDto {
                    result_required: true,
                    preferred_format: None,
                },
            )),
            planning_snapshot: Some(planning_snapshot.clone()),
        })
        .await?;
        let task_id = created.id.clone();
        let task_result_brief =
            match task_service_client::get_task_result_brief(task_id.as_str()).await {
                Ok(item) => item.map(map_remote_result_brief),
                Err(err) => {
                    warn!(
                        "load task result brief failed after task create: task_id={} detail={}",
                        task_id, err
                    );
                    None
                }
            };
        out.push(map_remote_task_to_record(created, task_result_brief));
    }

    Ok(out)
}
