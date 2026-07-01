// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::internal_context_locale::{
    internal_context_locale_from_settings, InternalContextLocale,
};
use crate::core::messages::{
    build_message, create_message_and_maybe_rename, ensure_message_metadata_object,
    NewMessageFields,
};
use crate::core::validation::normalize_non_empty;
use crate::models::memory_mapping_types::MemoryProjectContactDto;
use crate::models::message::Message;
use crate::models::session::Session;
use crate::services::{
    chatos_memory_mappings, chatos_sessions, task_runner_api_client, user_settings,
};

use super::super::session_resolver::resolve_project_contact_session_id;
use super::errors::HandlerError;
use super::types::{RequirementPlanItem, SelectedContactRuntime, WorkItemPlanItem};

pub(in crate::api::projects) async fn select_contact_runtime(
    auth: &AuthUser,
    cfg: &Config,
    requested_contact_id: Option<String>,
    project_id: &str,
    user_access_token: &str,
) -> Result<SelectedContactRuntime, HandlerError> {
    let contacts = chatos_memory_mappings::list_project_contacts(project_id, Some(500), 0)
        .await
        .map_err(|err| HandlerError::internal("读取项目联系人失败", err))?;
    let requested_contact_id = normalize_non_empty(requested_contact_id);
    let mut candidates = contacts
        .into_iter()
        .filter(|contact| {
            requested_contact_id
                .as_deref()
                .is_none_or(|value| value == contact.contact_id)
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .last_message_at
            .cmp(&left.last_message_at)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
    });

    for contact in candidates {
        let runtime = chatos_memory_mappings::get_contact_task_runner_runtime_config(
            Some(auth.user_id.as_str()),
            Some(contact.contact_id.as_str()),
            Some(contact.agent_id.as_str()),
        )
        .await
        .map_err(|err| HandlerError::internal("读取联系人 Task Runner 配置失败", err))?;
        let Some(runtime) = runtime else {
            continue;
        };
        let Some(user_service_base_url) = cfg
            .user_service_base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Err(HandlerError::internal(
                "用户服务地址未配置",
                "CHATOS_USER_SERVICE_BASE_URL / USER_SERVICE_BASE_URL is required".to_string(),
            ));
        };
        let Some(agent_account_id) = runtime.agent_account_id.clone() else {
            continue;
        };
        let task_runner_agent_token =
            task_runner_api_client::exchange_task_runner_token_via_user_service(
                &task_runner_api_client::UserServiceTaskRunnerExchange {
                    base_url: user_service_base_url.to_string(),
                    access_token: user_access_token.to_string(),
                    task_runner_agent_account_id: agent_account_id,
                    contact_id: Some(contact.contact_id.clone()),
                },
            )
            .await
            .map_err(|err| HandlerError::bad_gateway("兑换 Task Runner agent token 失败", err))?;
        return Ok(SelectedContactRuntime {
            contact,
            task_runner_base_url: runtime.base_url,
            task_runner_agent_token,
        });
    }

    Err(if requested_contact_id.is_some() {
        HandlerError::bad_request("指定联系人未绑定可用的 Task Runner")
    } else {
        HandlerError::bad_request("项目没有绑定可用的 Task Runner 联系人")
    })
}

pub(in crate::api::projects) async fn resolve_or_create_execution_session(
    auth: &AuthUser,
    project: &crate::models::project::Project,
    contact: &MemoryProjectContactDto,
    requirement_title: &str,
) -> Result<Session, HandlerError> {
    if let Some(session_id) = contact.latest_session_id.as_deref() {
        if let Ok(Some(session)) = chatos_sessions::get_session_by_id(session_id).await {
            return Ok(session);
        }
    }
    if let Some((session_id, _)) = resolve_project_contact_session_id(
        auth.user_id.as_str(),
        project.id.as_str(),
        contact.contact_id.as_str(),
    )
    .await
    {
        if let Ok(Some(session)) = chatos_sessions::get_session_by_id(session_id.as_str()).await {
            return Ok(session);
        }
    }

    let title = format!("执行需求：{requirement_title}");
    let metadata = json!({
        "chat_runtime": {
            "project_id": project.id,
            "project_root": project.root_path,
            "contact_id": contact.contact_id,
            "contact_agent_id": contact.agent_id,
            "mcp_enabled": true
        },
        "contact": {
            "contact_id": contact.contact_id,
            "agent_id": contact.agent_id,
            "agent_name_snapshot": contact.agent_name_snapshot
        },
        "ui_contact": {
            "contact_id": contact.contact_id,
            "agent_id": contact.agent_id
        },
        "ui_chat_selection": {
            "selected_agent_id": contact.agent_id
        }
    });
    chatos_sessions::create_session(
        auth.user_id.clone(),
        title,
        Some(project.id.clone()),
        Some(metadata),
    )
    .await
    .map_err(|err| HandlerError::internal("创建联系人会话失败", err))
}

pub(in crate::api::projects) async fn create_execution_message(
    session: &Session,
    project_id: &str,
    requirement: &RequirementPlanItem,
    contact: &MemoryProjectContactDto,
    work_items: &[WorkItemPlanItem],
) -> Result<Message, HandlerError> {
    let content = format!(
        "执行需求：{}\n\n本消息由项目需求执行按钮创建，用于关联 Task Runner 执行任务，不会发送给 AI 对话。",
        requirement.title
    );
    let mut message = build_message(
        session.id.clone(),
        NewMessageFields {
            role: Some("user".to_string()),
            content: Some(content),
            message_mode: Some("project_requirement_execution".to_string()),
            message_source: Some("project_management".to_string()),
            metadata: Some(json!({
                "project_requirement_execution": {
                    "project_id": project_id,
                    "requirement_id": requirement.id,
                    "requirement_title": requirement.title,
                    "contact_id": contact.contact_id,
                    "contact_agent_id": contact.agent_id,
                    "project_task_ids": work_items.iter().map(|item| item.id.clone()).collect::<Vec<_>>(),
                    "task_links": [],
                },
                "task_runner_async": {
                    "mode": "project_requirement_execution",
                    "overall_status": "queued",
                    "source": "project_requirement_execute_button",
                    "project_id": project_id,
                    "requirement_id": requirement.id,
                    "created_task_ids": [],
                    "running_task_ids": [],
                    "terminal_task_ids": [],
                }
            })),
            ..NewMessageFields::default()
        },
        "user",
    );
    let turn_id = message.id.clone();
    let metadata = ensure_message_metadata_object(&mut message);
    metadata.insert(
        "conversation_turn_id".to_string(),
        Value::String(turn_id.clone()),
    );
    if let Some(Value::Object(task_runner_async)) = metadata.get_mut("task_runner_async") {
        task_runner_async.insert("source_turn_id".to_string(), Value::String(turn_id));
    }
    create_message_and_maybe_rename(message)
        .await
        .map_err(|err| HandlerError::internal("创建执行消息失败", err))
}

pub(in crate::api::projects) async fn load_task_runner_builtin_prompt_locale(
    user_id: &str,
) -> Result<String, HandlerError> {
    let settings = user_settings::get_effective_user_settings(Some(user_id.to_string()))
        .await
        .map_err(|err| HandlerError::internal("读取 Chatos 用户设置失败", err))?;
    let locale = internal_context_locale_from_settings(&settings);
    Ok(if locale.is_english() {
        InternalContextLocale::ENGLISH_KEY.to_string()
    } else {
        InternalContextLocale::DEFAULT_KEY.to_string()
    })
}
