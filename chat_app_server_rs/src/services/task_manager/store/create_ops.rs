use crate::services::builtin_mcp::UI_PROMPTER_MCP_ID;
use crate::services::task_manager::normalizer::{normalize_task_drafts, trimmed_non_empty};
use crate::services::task_manager::types::{
    TaskDraft, TaskRecord, TaskRequiredContextAssetDraft,
};
use crate::services::task_capability_registry::{
    capability_runtime_requirements_satisfied, find_task_capability_by_mcp_id,
    find_task_capability_by_token, infer_capability_mcp_ids_from_text,
    infer_default_capability_mcp_ids, planning_task_capability_tokens,
};
use crate::services::task_service_client::{
    self, CreateTaskRequestDto, TaskContextAssetRefDto, TaskExecutionResultContractDto,
    TaskPlanningSnapshotDto,
};
use tracing::{info, warn};

use super::remote_support::{
    map_remote_result_brief, map_remote_task_to_record, resolve_task_scope_context,
    TaskScopeContext,
};

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
    for planned_builtin_mcp_id in planned_builtin_mcp_ids {
        let Some(capability) = find_task_capability_by_mcp_id(planned_builtin_mcp_id.as_str())
        else {
            continue;
        };
        if capability_runtime_requirements_satisfied(
            capability,
            has_project_root,
            has_remote_connection,
        ) {
            continue;
        }
        if capability
            .runtime_requirements
            .iter()
            .any(|item| item.trim() == "project_root")
            && !has_project_root
        {
            return Err(format!(
                "当前任务计划使用 {} 能力，但当前会话没有可用的 project_root",
                capability.display_name
            ));
        }
        if capability
            .runtime_requirements
            .iter()
            .any(|item| item.trim() == "remote_connection_id")
            && !has_remote_connection
        {
            return Err(format!(
                "当前任务计划使用 {} 能力，但当前会话没有选中的 remote_connection_id",
                capability.display_name
            ));
        }
    }

    Ok(())
}

async fn resolve_contact_builtin_mcp_grants(scope: &TaskScopeContext) -> Vec<String> {
    let Ok(contact) = crate::services::memory_server_client::resolve_memory_contact(
        Some(scope.user_id.as_str()),
        scope.contact_id.as_deref(),
        Some(scope.contact_agent_id.as_str()),
    )
    .await
    else {
        return Vec::new();
    };

    let grants = contact
        .map(|contact| contact.authorized_builtin_mcp_ids)
        .unwrap_or_default();

    info!(
        "resolved contact builtin MCP grants for task creation: contact_id={} contact_agent_id={} grants={}",
        scope.contact_id.as_deref().unwrap_or_default(),
        scope.contact_agent_id,
        grants.join(", ")
    );

    grants
}

fn normalize_asset_type(asset_type: &str) -> Option<&'static str> {
    match asset_type.trim().to_ascii_lowercase().as_str() {
        "skill" => Some("skill"),
        "plugin" => Some("plugin"),
        "common" | "commons" => Some("common"),
        _ => None,
    }
}

fn resolve_required_builtin_capabilities(
    capabilities: &[String],
    authorized_builtin_mcp_ids: &[String],
) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut unsupported = Vec::new();
    let mut unauthorized = Vec::new();

    for capability in capabilities {
        let token = capability.trim();
        let Some(definition) = find_task_capability_by_token(token) else {
            unsupported.push(token.to_string());
            continue;
        };
        let mcp_id = definition.builtin_mcp_id.as_str();
        if !authorized_builtin_mcp_ids.iter().any(|item| item == mcp_id) {
            unauthorized.push(format!("{token}->{mcp_id}"));
            continue;
        }
        if !out.iter().any(|item| item == mcp_id) {
            out.push(mcp_id.to_string());
        }
    }

    if !unsupported.is_empty() {
        return Err(format!(
            "required_builtin_capabilities contains unsupported items: {}. allowed={}",
            unsupported.join(", "),
            planning_task_capability_tokens().join(", ")
        ));
    }
    if !unauthorized.is_empty() {
        return Err(format!(
            "required_builtin_capabilities contains unauthorized items: {}",
            unauthorized.join(", ")
        ));
    }

    Ok(out)
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

fn resolve_required_context_assets(
    selections: &[TaskRequiredContextAssetDraft],
    runtime_context: &crate::services::memory_server_client::MemoryAgentRuntimeContextDto,
) -> Result<Vec<TaskContextAssetRefDto>, String> {
    let mut out = Vec::new();

    for selection in selections {
        let Some(asset_type) = normalize_asset_type(selection.asset_type.as_str()) else {
            return Err(format!(
                "unsupported required_context_assets.asset_type: {}",
                selection.asset_type
            ));
        };
        let asset_ref = selection.asset_ref.trim();
        if asset_ref.is_empty() {
            continue;
        }

        let resolved = match asset_type {
            "skill" => {
                let skill = runtime_context
                    .runtime_skills
                    .iter()
                    .enumerate()
                    .find(|(index, item)| {
                        asset_ref.eq_ignore_ascii_case(format!("SK{}", index + 1).as_str())
                            || item.id.trim() == asset_ref
                            || item
                                .source_path
                                .as_deref()
                                .map(str::trim)
                                .unwrap_or("")
                                == asset_ref
                            || item.name.trim() == asset_ref
                    })
                    .map(|(_, item)| item)
                    .ok_or_else(|| {
                        format!(
                            "required_context_assets skill not found in current contact runtime: {}",
                            asset_ref
                        )
                    })?;
                TaskContextAssetRefDto {
                    asset_type: "skill".to_string(),
                    asset_id: skill.id.clone(),
                    display_name: Some(skill.name.clone()),
                    source_type: Some(skill.source_type.clone()),
                    source_path: skill.source_path.clone(),
                }
            }
            "plugin" => {
                let plugin = runtime_context
                    .runtime_plugins
                    .iter()
                    .enumerate()
                    .find(|(index, item)| {
                        asset_ref.eq_ignore_ascii_case(format!("PL{}", index + 1).as_str())
                            || item.source.trim() == asset_ref
                            || item.name.trim() == asset_ref
                    })
                    .map(|(_, item)| item)
                    .ok_or_else(|| {
                        format!(
                            "required_context_assets plugin not found in current contact runtime: {}",
                            asset_ref
                        )
                    })?;
                TaskContextAssetRefDto {
                    asset_type: "plugin".to_string(),
                    asset_id: plugin.source.clone(),
                    display_name: Some(plugin.name.clone()),
                    source_type: Some("plugin".to_string()),
                    source_path: None,
                }
            }
            "common" => {
                let command = runtime_context
                    .runtime_commands
                    .iter()
                    .find(|item| {
                        item.command_ref.trim() == asset_ref
                            || item.source_path.trim() == asset_ref
                            || item.name.trim() == asset_ref
                    })
                    .ok_or_else(|| {
                        format!(
                            "required_context_assets common not found in current contact runtime: {}",
                            asset_ref
                        )
                    })?;
                TaskContextAssetRefDto {
                    asset_type: "common".to_string(),
                    asset_id: command.command_ref.clone(),
                    display_name: Some(command.name.clone()),
                    source_type: Some("runtime_command".to_string()),
                    source_path: Some(command.source_path.clone()),
                }
            }
            _ => unreachable!(),
        };

        let duplicated = out.iter().any(|existing: &TaskContextAssetRefDto| {
            existing.asset_type == resolved.asset_type && existing.asset_id == resolved.asset_id
        });
        if !duplicated {
            out.push(resolved);
        }
    }

    Ok(out)
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
    runtime_snapshot: Option<&crate::services::memory_server_client::TurnRuntimeSnapshotDto>,
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

fn infer_task_builtin_mcp_ids(
    draft: &TaskDraft,
    scope: &TaskScopeContext,
    authorized_builtin_mcp_ids: &[String],
    runtime_snapshot: Option<&crate::services::memory_server_client::TurnRuntimeSnapshotDto>,
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

fn merge_runtime_selected_command_assets(
    draft_assets: &[TaskContextAssetRefDto],
    runtime_snapshot: Option<&crate::services::memory_server_client::TurnRuntimeSnapshotDto>,
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
    let runtime_snapshot =
        crate::services::memory_server_client::get_turn_runtime_snapshot_by_turn(
            session_id.as_str(),
            conversation_turn_id.as_str(),
        )
        .await
        .ok()
        .and_then(|payload| payload.snapshot);
    let planning_snapshot = build_task_planning_snapshot(
        session_id.as_str(),
        conversation_turn_id.as_str(),
        &scope,
        contact_authorized_builtin_mcp_ids.as_slice(),
        runtime_snapshot.as_ref(),
    )
    .await;
    let mut out = Vec::with_capacity(draft_tasks.len());
    for mut draft in draft_tasks {
        let capability_builtin_mcp_ids = resolve_required_builtin_capabilities(
            draft.required_builtin_capabilities.as_slice(),
            contact_authorized_builtin_mcp_ids.as_slice(),
        )?;
        for mcp_id in capability_builtin_mcp_ids {
            if !draft.planned_builtin_mcp_ids.iter().any(|item| item == &mcp_id) {
                draft.planned_builtin_mcp_ids.push(mcp_id);
            }
        }
        let required_context_assets = resolve_required_context_assets(
            draft.required_context_assets.as_slice(),
            &runtime_context,
        )?;
        for asset in required_context_assets {
            if !draft.planned_context_assets.iter().any(|existing| {
                existing.asset_type == asset.asset_type && existing.asset_id == asset.asset_id
            }) {
                draft.planned_context_assets.push(asset);
            }
        }
        draft.planned_builtin_mcp_ids = infer_task_builtin_mcp_ids(
            &draft,
            &scope,
            contact_authorized_builtin_mcp_ids.as_slice(),
            runtime_snapshot.as_ref(),
        );
        info!(
            "resolved task draft builtin MCP ids: title={} required_builtin_capabilities={} planned_builtin_mcp_ids={}",
            draft.title,
            draft.required_builtin_capabilities.join(", "),
            draft.planned_builtin_mcp_ids.join(", ")
        );
        ensure_planned_builtin_mcp_ids_authorized(
            draft.planned_builtin_mcp_ids.as_slice(),
            contact_authorized_builtin_mcp_ids.as_slice(),
        )?;
        ensure_runtime_requirements(draft.planned_builtin_mcp_ids.as_slice(), &scope)?;
        draft.planned_context_assets = merge_runtime_selected_command_assets(
            draft.planned_context_assets.as_slice(),
            runtime_snapshot.as_ref(),
        );
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
