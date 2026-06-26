use axum::{extract::Path, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::internal_context_locale::{
    internal_context_locale_from_settings, InternalContextLocale,
};
use crate::core::messages::{
    build_message, create_message_and_maybe_rename, ensure_message_metadata_object,
    message_turn_id, NewMessageFields,
};
use crate::core::project_access::ensure_owned_project;
use crate::core::time::now_rfc3339;
use crate::core::validation::normalize_non_empty;
use crate::models::memory_mapping_types::MemoryProjectContactDto;
use crate::models::message::Message;
use crate::models::session::Session;
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::services::{
    access_token_scope, chatos_memory_mappings, chatos_sessions, project_management_api_client,
    task_runner_api_client, user_settings,
};

use super::session_resolver::resolve_project_contact_session_id;

#[derive(Debug, Default, Deserialize)]
pub(super) struct ExecuteRequirementRequest {
    contact_id: Option<String>,
}

#[derive(Debug)]
struct HandlerError {
    status: StatusCode,
    error: String,
    detail: Option<String>,
}

#[derive(Debug, Clone)]
struct RequirementPlanItem {
    id: String,
    title: String,
    status: String,
    parent_requirement_id: Option<String>,
}

#[derive(Debug, Clone)]
struct WorkItemPlanItem {
    id: String,
    requirement_id: String,
    title: String,
    description: Option<String>,
    task_runner_default_model_config_id: String,
    task_runner_enabled_tool_ids: Vec<String>,
    status: String,
    priority: i32,
    tags: Vec<String>,
}

#[derive(Debug, Clone)]
struct SelectedContactRuntime {
    contact: MemoryProjectContactDto,
    task_runner_base_url: String,
    task_runner_agent_token: String,
}

#[derive(Debug, Clone)]
struct CreatedExecutionTask {
    project_task_id: String,
    requirement_id: String,
    task_runner_task_id: String,
    task_runner_run_id: Option<String>,
    task_runner_status: String,
}

#[derive(Debug, Clone)]
struct ExecutionLink {
    work_item_id: String,
    task_runner_task_id: String,
    task_runner_run_id: Option<String>,
    task_runner_status: Option<String>,
    source_session_id: Option<String>,
    source_user_message_id: Option<String>,
}

pub(super) async fn execute_requirement(
    auth: AuthUser,
    Path((project_id, requirement_id)): Path<(String, String)>,
    Json(req): Json<ExecuteRequirementRequest>,
) -> (StatusCode, Json<Value>) {
    match execute_requirement_inner(auth, project_id, requirement_id, req).await {
        Ok(value) => (StatusCode::CREATED, Json(value)),
        Err(err) => {
            let mut body = json!({ "error": err.error });
            if let Some(detail) = err.detail {
                body["detail"] = Value::String(detail);
            }
            (err.status, Json(body))
        }
    }
}

pub(super) async fn stop_requirement_execution(
    auth: AuthUser,
    Path((project_id, requirement_id)): Path<(String, String)>,
    Json(req): Json<ExecuteRequirementRequest>,
) -> (StatusCode, Json<Value>) {
    match stop_requirement_execution_inner(auth, project_id, requirement_id, req).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => {
            let mut body = json!({ "error": err.error });
            if let Some(detail) = err.detail {
                body["detail"] = Value::String(detail);
            }
            (err.status, Json(body))
        }
    }
}

async fn execute_requirement_inner(
    auth: AuthUser,
    project_id: String,
    requirement_id: String,
    req: ExecuteRequirementRequest,
) -> Result<Value, HandlerError> {
    let project = ensure_owned_project(&project_id, &auth)
        .await
        .map_err(|err| match err {
            crate::core::project_access::ProjectAccessError::NotFound => {
                HandlerError::not_found("项目不存在")
            }
            crate::core::project_access::ProjectAccessError::Forbidden => {
                HandlerError::forbidden("无权访问该项目")
            }
            crate::core::project_access::ProjectAccessError::Internal(err) => {
                HandlerError::internal("读取项目失败", err)
            }
        })?;
    let cfg = Config::try_get().map_err(|err| HandlerError::internal("配置未初始化", err))?;
    let access_token = access_token_scope::get_current_access_token()
        .ok_or_else(|| HandlerError::unauthorized("current user access token is required"))?;
    let project_sync_secret = cfg
        .project_service_sync_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            HandlerError::internal(
                "项目执行需要配置项目管理同步密钥",
                "CHATOS_PROJECT_SERVICE_SYNC_SECRET / PROJECT_SERVICE_SYNC_SECRET is required"
                    .to_string(),
            )
        })?
        .to_string();

    let requirements = project_management_api_client::list_project_service_requirements(
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        project.id.as_str(),
        false,
    )
    .await
    .map_err(|err| HandlerError::bad_gateway("读取需求列表失败", err))?;
    let work_items = project_management_api_client::list_project_service_work_items(
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        project.id.as_str(),
        false,
    )
    .await
    .map_err(|err| HandlerError::bad_gateway("读取项目任务列表失败", err))?;
    let dependency_graph = project_management_api_client::get_project_service_dependency_graph(
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        project.id.as_str(),
        false,
    )
    .await
    .map_err(|err| HandlerError::bad_gateway("读取项目依赖图失败", err))?;

    let requirement_items = parse_requirements(requirements);
    let Some(root_requirement) = requirement_items
        .iter()
        .find(|item| item.id == requirement_id)
        .cloned()
    else {
        return Err(HandlerError::not_found("需求不存在"));
    };
    let requirement_scope = collect_requirement_scope(&requirement_items, requirement_id.as_str());
    let all_work_items = parse_work_items(work_items);
    let requirement_dependency_map = requirement_dependency_map(&dependency_graph);
    validate_requirement_prerequisites(
        &requirement_items,
        &requirement_scope,
        &requirement_dependency_map,
    )?;
    let mut dependency_map = work_item_dependency_map(&dependency_graph);
    let mut selected_work_items = all_work_items
        .iter()
        .cloned()
        .into_iter()
        .filter(|item| requirement_scope.contains(item.requirement_id.as_str()))
        .filter(|item| item.status != "archived")
        .collect::<Vec<_>>();
    add_requirement_work_item_dependencies(
        &mut dependency_map,
        &selected_work_items,
        &requirement_dependency_map,
        &requirement_scope,
    );
    let creation_order = topological_work_item_order(&selected_work_items, &dependency_map)?;
    selected_work_items.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.id.cmp(&right.id))
    });
    if selected_work_items.is_empty() {
        return Err(HandlerError::bad_request("该需求下没有可执行的项目任务"));
    }
    let contact_runtime = select_contact_runtime(
        &auth,
        cfg,
        req.contact_id,
        project.id.as_str(),
        access_token.as_str(),
    )
    .await?;
    ensure_requirement_execution_not_active(
        &root_requirement,
        &selected_work_items,
        cfg.project_service_base_url.as_str(),
        project_sync_secret.as_str(),
        access_token.as_str(),
        &contact_runtime,
    )
    .await?;
    for item in &selected_work_items {
        if item.task_runner_default_model_config_id.trim().is_empty() {
            return Err(HandlerError::bad_request(format!(
                "项目任务缺少 Task Runner 模型配置: {}",
                item.title
            )));
        }
        if item.task_runner_enabled_tool_ids.is_empty() {
            return Err(HandlerError::bad_request(format!(
                "项目任务缺少 Task Runner 工具集配置: {}",
                item.title
            )));
        }
    }

    let external_prerequisite_task_ids = load_external_prerequisite_task_ids(
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        &selected_work_items,
        &all_work_items,
        &dependency_map,
        &requirement_dependency_map,
        &requirement_scope,
    )
    .await?;
    let session = resolve_or_create_execution_session(
        &auth,
        &project,
        &contact_runtime.contact,
        root_requirement.title.as_str(),
    )
    .await?;
    let message = create_execution_message(
        &session,
        project.id.as_str(),
        &root_requirement,
        &contact_runtime.contact,
        &selected_work_items,
    )
    .await?;

    let execution_options = task_runner_api_client::fetch_task_runner_execution_options(
        contact_runtime.task_runner_base_url.as_str(),
        contact_runtime.task_runner_agent_token.as_str(),
    )
    .await
    .map_err(|err| HandlerError::bad_gateway("读取 Task Runner 工具集失败", err))?;
    let builtin_prompt_locale =
        load_task_runner_builtin_prompt_locale(auth.user_id.as_str()).await?;

    sync_requirement_execution_state(
        cfg.project_service_base_url.as_str(),
        project_sync_secret.as_str(),
        root_requirement.id.as_str(),
        Some("in_progress"),
        Vec::new(),
        None,
        false,
    )
    .await?;

    let created_tasks = create_and_start_execution_tasks(
        cfg,
        project_sync_secret.as_str(),
        access_token.as_str(),
        &contact_runtime,
        &session,
        &message,
        project.id.as_str(),
        project.root_path.as_str(),
        &selected_work_items,
        &creation_order,
        &dependency_map,
        &external_prerequisite_task_ids,
        &execution_options,
        builtin_prompt_locale.as_str(),
    )
    .await?;
    let final_message = persist_execution_message_links(
        &session,
        message,
        project.id.as_str(),
        requirement_id.as_str(),
        &created_tasks,
    )
    .await?;

    Ok(json!({
        "success": true,
        "project_id": project.id,
        "requirement_id": requirement_id,
        "contact_id": contact_runtime.contact.contact_id,
        "conversation_id": session.id,
        "message_id": final_message.id,
        "message": final_message,
        "created_tasks": created_tasks.iter().map(|item| {
            json!({
                "project_task_id": item.project_task_id,
                "requirement_id": item.requirement_id,
                "task_runner_task_id": item.task_runner_task_id,
                "task_runner_run_id": item.task_runner_run_id,
                "task_runner_status": item.task_runner_status,
            })
        }).collect::<Vec<_>>(),
        "plan_mode_enabled": false,
    }))
}

async fn stop_requirement_execution_inner(
    auth: AuthUser,
    project_id: String,
    requirement_id: String,
    req: ExecuteRequirementRequest,
) -> Result<Value, HandlerError> {
    let project = ensure_owned_project(&project_id, &auth)
        .await
        .map_err(|err| match err {
            crate::core::project_access::ProjectAccessError::NotFound => {
                HandlerError::not_found("项目不存在")
            }
            crate::core::project_access::ProjectAccessError::Forbidden => {
                HandlerError::forbidden("无权访问该项目")
            }
            crate::core::project_access::ProjectAccessError::Internal(err) => {
                HandlerError::internal("读取项目失败", err)
            }
        })?;
    let cfg = Config::try_get().map_err(|err| HandlerError::internal("配置未初始化", err))?;
    let access_token = access_token_scope::get_current_access_token()
        .ok_or_else(|| HandlerError::unauthorized("current user access token is required"))?;
    let project_sync_secret = cfg
        .project_service_sync_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            HandlerError::internal(
                "项目执行需要配置项目管理同步密钥",
                "CHATOS_PROJECT_SERVICE_SYNC_SECRET / PROJECT_SERVICE_SYNC_SECRET is required"
                    .to_string(),
            )
        })?
        .to_string();

    let requirements = project_management_api_client::list_project_service_requirements(
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        project.id.as_str(),
        false,
    )
    .await
    .map_err(|err| HandlerError::bad_gateway("读取需求列表失败", err))?;
    let work_items = project_management_api_client::list_project_service_work_items(
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        project.id.as_str(),
        false,
    )
    .await
    .map_err(|err| HandlerError::bad_gateway("读取项目任务列表失败", err))?;
    let requirement_items = parse_requirements(requirements);
    let Some(root_requirement) = requirement_items
        .iter()
        .find(|item| item.id == requirement_id)
        .cloned()
    else {
        return Err(HandlerError::not_found("需求不存在"));
    };
    let requirement_scope = collect_requirement_scope(&requirement_items, requirement_id.as_str());
    let selected_work_items = parse_work_items(work_items)
        .into_iter()
        .filter(|item| requirement_scope.contains(item.requirement_id.as_str()))
        .filter(|item| item.status != "archived")
        .collect::<Vec<_>>();
    if selected_work_items.is_empty() {
        return Err(HandlerError::bad_request("该需求下没有可停止的项目任务"));
    }

    let contact_runtime = select_contact_runtime(
        &auth,
        cfg,
        req.contact_id,
        project.id.as_str(),
        access_token.as_str(),
    )
    .await?;
    let links = load_execution_links_for_work_items(
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        &selected_work_items,
    )
    .await?;
    let active_links = links
        .iter()
        .filter(|link| task_runner_status_is_active(link.task_runner_status.as_deref()))
        .cloned()
        .collect::<Vec<_>>();
    mark_execution_messages_for_stop(&active_links, "stopping").await;

    let mut cancelled_tasks = Vec::new();
    let mut skipped_tasks = Vec::new();
    let mut cancel_errors = Vec::new();
    for link in &links {
        if task_runner_status_is_success(link.task_runner_status.as_deref()) {
            skipped_tasks.push(json!({
                "project_task_id": link.work_item_id,
                "task_runner_task_id": link.task_runner_task_id,
                "reason": "succeeded",
            }));
            continue;
        }
        if !task_runner_status_is_active(link.task_runner_status.as_deref()) {
            skipped_tasks.push(json!({
                "project_task_id": link.work_item_id,
                "task_runner_task_id": link.task_runner_task_id,
                "status": link.task_runner_status,
                "reason": "not_active",
            }));
            continue;
        }
        let cancel_result = task_runner_api_client::cancel_task_runner_task(
            contact_runtime.task_runner_base_url.as_str(),
            contact_runtime.task_runner_agent_token.as_str(),
            Some(access_token.as_str()),
            link.task_runner_task_id.as_str(),
            &task_runner_api_client::CancelTaskRunnerTaskRequest {
                reason: format!("用户停止需求执行：{}", root_requirement.title),
                replacement_task_ids: Vec::new(),
            },
        )
        .await;
        match cancel_result {
            Ok(value) => {
                let status = value_string(&value, "status")
                    .or_else(|| {
                        value
                            .get("task")
                            .and_then(|task| value_string(task, "status"))
                    })
                    .unwrap_or_else(|| "cancelled".to_string());
                if let Err(err) = sync_execution_link_status(
                    cfg.project_service_base_url.as_str(),
                    project_sync_secret.as_str(),
                    link,
                    status.as_str(),
                    task_runner_callback_event_for_status(status.as_str())
                        .or(Some("task.cancelled")),
                )
                .await
                {
                    cancel_errors.push(format!("{}: {}", link.task_runner_task_id, err.error));
                    continue;
                }
                cancelled_tasks.push(json!({
                    "project_task_id": link.work_item_id,
                    "task_runner_task_id": link.task_runner_task_id,
                    "task_runner_run_id": link.task_runner_run_id,
                    "task_runner_status": status,
                    "result": value,
                }));
            }
            Err(err) => cancel_errors.push(format!("{}: {}", link.task_runner_task_id, err)),
        }
    }
    if !cancel_errors.is_empty() {
        return Err(HandlerError::bad_gateway(
            "取消 Task Runner 执行任务失败",
            cancel_errors.join("；"),
        ));
    }

    let work_item_ids = selected_work_items
        .iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    sync_requirement_execution_state(
        cfg.project_service_base_url.as_str(),
        project_sync_secret.as_str(),
        root_requirement.id.as_str(),
        Some("approved"),
        work_item_ids.clone(),
        Some("ready"),
        true,
    )
    .await?;
    mark_execution_messages_for_stop(&active_links, "stopped").await;

    Ok(json!({
        "success": true,
        "project_id": project.id,
        "requirement_id": requirement_id,
        "contact_id": contact_runtime.contact.contact_id,
        "cancelled_tasks": cancelled_tasks,
        "skipped_tasks": skipped_tasks,
        "reset_work_item_ids": work_item_ids,
    }))
}

async fn select_contact_runtime(
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

async fn resolve_or_create_execution_session(
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

async fn create_execution_message(
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

async fn create_and_start_execution_tasks(
    cfg: &Config,
    project_sync_secret: &str,
    user_access_token: &str,
    contact_runtime: &SelectedContactRuntime,
    session: &Session,
    message: &Message,
    project_id: &str,
    project_root: &str,
    work_items: &[WorkItemPlanItem],
    creation_order: &[String],
    dependency_map: &BTreeMap<String, Vec<String>>,
    external_prerequisite_task_ids: &BTreeMap<String, Vec<String>>,
    execution_options: &task_runner_api_client::TaskRunnerExecutionOptions,
    builtin_prompt_locale: &str,
) -> Result<Vec<CreatedExecutionTask>, HandlerError> {
    let work_item_ids = work_items
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    let by_id = work_items
        .iter()
        .map(|item| (item.id.clone(), item.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut created_by_work_item = BTreeMap::<String, CreatedExecutionTask>::new();

    for work_item_id in creation_order {
        let Some(work_item) = by_id.get(work_item_id.as_str()) else {
            continue;
        };
        let mut prerequisite_task_ids = external_prerequisite_task_ids
            .get(work_item_id.as_str())
            .cloned()
            .unwrap_or_default();
        for dep_id in dependency_map
            .get(work_item_id.as_str())
            .into_iter()
            .flatten()
            .filter(|dep_id| work_item_ids.contains(dep_id.as_str()))
        {
            if let Some(created) = created_by_work_item.get(dep_id) {
                prerequisite_task_ids.push(created.task_runner_task_id.clone());
            } else {
                return Err(HandlerError::bad_request(format!(
                    "项目任务前置尚未创建执行任务，无法继续: {}",
                    work_item.title
                )));
            }
        }
        prerequisite_task_ids.sort();
        prerequisite_task_ids.dedup();

        let mut mcp_config = execution_options
            .mcp_config_for_tool_ids(&work_item.task_runner_enabled_tool_ids)
            .map_err(HandlerError::bad_request)?;
        if let Some(workspace_dir) = normalize_non_empty(Some(project_root.to_string())) {
            mcp_config.workspace_dir = Some(workspace_dir);
        }
        mcp_config.builtin_prompt_locale = Some(builtin_prompt_locale.to_string());
        let create_request = task_runner_api_client::CreateTaskRunnerTaskRequest {
            title: work_item.title.clone(),
            description: build_task_description(work_item),
            objective: build_task_objective(work_item),
            input_payload: Some(json!({
                "source": "chatos_project_requirement_execution",
                "project_id": project_id,
                "project_root": project_root,
                "requirement_id": work_item.requirement_id,
                "project_task_id": work_item.id,
                "source_session_id": session.id,
                "source_user_message_id": message.id,
                "source_turn_id": message_turn_id(message),
            })),
            status: Some("ready".to_string()),
            priority: Some(work_item.priority),
            tags: Some(normalize_tags(
                work_item
                    .tags
                    .iter()
                    .cloned()
                    .chain(std::iter::once("project_requirement_execution".to_string()))
                    .collect(),
            )),
            default_model_config_id: Some(work_item.task_runner_default_model_config_id.clone()),
            project_id: Some(project_id.to_string()),
            task_profile: Some("default".to_string()),
            schedule: Some(task_runner_api_client::TaskRunnerTaskScheduleRequest {
                mode: "contact_async".to_string(),
                run_at: Some(now_rfc3339()),
            }),
            mcp_config: Some(mcp_config),
            prerequisite_task_ids: Some(prerequisite_task_ids),
        };
        let task = task_runner_api_client::create_task_runner_task(
            contact_runtime.task_runner_base_url.as_str(),
            contact_runtime.task_runner_agent_token.as_str(),
            Some(user_access_token),
            Some(session.id.as_str()),
            Some(message.id.as_str()),
            message_turn_id(message),
            &create_request,
        )
        .await
        .map_err(|err| HandlerError::bad_gateway("创建 Task Runner 执行任务失败", err))?;
        let task_runner_status = "queued".to_string();

        project_management_api_client::link_work_item_task_runner_task(
            cfg.project_service_base_url.as_str(),
            user_access_token,
            work_item.id.as_str(),
            &project_management_api_client::LinkTaskRunnerTaskRequest {
                task_runner_task_id: task.id.clone(),
                task_runner_run_id: task.last_run_id.clone(),
                link_type: Some("execution".to_string()),
                source_session_id: Some(session.id.clone()),
                source_user_message_id: Some(message.id.clone()),
                task_runner_status: Some(task_runner_status.clone()),
                last_callback_event: None,
                last_callback_at: None,
                last_error_message: None,
            },
        )
        .await
        .map_err(|err| HandlerError::bad_gateway("写入项目任务执行关联失败", err))?;

        project_management_api_client::sync_work_item_task_runner_status(
            cfg.project_service_base_url.as_str(),
            project_sync_secret,
            work_item.id.as_str(),
            &project_management_api_client::SyncTaskRunnerWorkItemStatusRequest {
                task_runner_task_id: task.id.clone(),
                task_runner_run_id: task.last_run_id.clone(),
                task_runner_status: Some(task_runner_status.clone()),
                last_callback_event: Some("task.queued".to_string()),
                last_callback_at: None,
                last_error_message: None,
                source_session_id: Some(session.id.clone()),
                source_user_message_id: Some(message.id.clone()),
            },
        )
        .await
        .map_err(|err| HandlerError::bad_gateway("同步项目任务执行状态失败", err))?;

        created_by_work_item.insert(
            work_item.id.clone(),
            CreatedExecutionTask {
                project_task_id: work_item.id.clone(),
                requirement_id: work_item.requirement_id.clone(),
                task_runner_task_id: task.id,
                task_runner_run_id: task.last_run_id,
                task_runner_status,
            },
        );
    }

    Ok(work_items
        .iter()
        .filter_map(|item| created_by_work_item.get(item.id.as_str()).cloned())
        .collect())
}

async fn load_external_prerequisite_task_ids(
    base_url: &str,
    access_token: &str,
    work_items: &[WorkItemPlanItem],
    all_work_items: &[WorkItemPlanItem],
    dependency_map: &BTreeMap<String, Vec<String>>,
    requirement_dependency_map: &BTreeMap<String, Vec<String>>,
    requirement_scope: &BTreeSet<String>,
) -> Result<BTreeMap<String, Vec<String>>, HandlerError> {
    let selected_ids = work_items
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    let work_item_by_id = all_work_items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let mut out = BTreeMap::new();
    for item in work_items {
        let mut task_ids = Vec::new();
        let mut blockers = Vec::new();
        for dep_id in dependency_map
            .get(item.id.as_str())
            .into_iter()
            .flatten()
            .filter(|dep_id| !selected_ids.contains(dep_id.as_str()))
        {
            if let Some(task_id) =
                linked_task_runner_task_id(base_url, access_token, dep_id.as_str()).await?
            {
                task_ids.push(task_id);
                continue;
            }
            match work_item_by_id.get(dep_id.as_str()) {
                Some(dep_item) if is_done_status(dep_item.status.as_str()) => {}
                Some(dep_item) => blockers.push(format!(
                    "{} 前置项目任务未完成且没有可等待的执行任务：{}",
                    item.title, dep_item.title
                )),
                None => blockers.push(format!(
                    "{} 前置项目任务不存在或不可见：{}",
                    item.title, dep_id
                )),
            }
        }

        for prerequisite_requirement_id in requirement_dependency_map
            .get(item.requirement_id.as_str())
            .into_iter()
            .flatten()
            .filter(|requirement_id| !requirement_scope.contains(requirement_id.as_str()))
        {
            for dep_item in all_work_items.iter().filter(|candidate| {
                candidate.requirement_id == *prerequisite_requirement_id
                    && candidate.status != "archived"
            }) {
                if let Some(task_id) =
                    linked_task_runner_task_id(base_url, access_token, dep_item.id.as_str()).await?
                {
                    task_ids.push(task_id);
                    continue;
                }
                if !is_done_status(dep_item.status.as_str()) {
                    blockers.push(format!(
                        "{} 前置需求下的项目任务未完成且没有可等待的执行任务：{}",
                        item.title, dep_item.title
                    ));
                }
            }
        }
        if !blockers.is_empty() {
            blockers.sort();
            blockers.dedup();
            return Err(HandlerError::bad_request(format!(
                "存在未满足的前置项目任务，无法执行：{}",
                blockers.join("；")
            )));
        }
        task_ids.sort();
        task_ids.dedup();
        out.insert(item.id.clone(), task_ids);
    }
    Ok(out)
}

async fn linked_task_runner_task_id(
    base_url: &str,
    access_token: &str,
    work_item_id: &str,
) -> Result<Option<String>, HandlerError> {
    let links = project_management_api_client::list_work_item_task_runner_links(
        base_url,
        access_token,
        work_item_id,
    )
    .await
    .map_err(|err| HandlerError::bad_gateway("读取前置项目任务执行关联失败", err))?;
    Ok(links
        .iter()
        .find_map(|link| value_string(link, "task_runner_task_id")))
}

async fn load_task_runner_builtin_prompt_locale(user_id: &str) -> Result<String, HandlerError> {
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

async fn ensure_requirement_execution_not_active(
    requirement: &RequirementPlanItem,
    work_items: &[WorkItemPlanItem],
    base_url: &str,
    project_sync_secret: &str,
    access_token: &str,
    contact_runtime: &SelectedContactRuntime,
) -> Result<(), HandlerError> {
    if requirement.status == "in_progress" {
        return Err(HandlerError::bad_request(
            "该需求已有执行中的任务，请先停止当前执行",
        ));
    }
    if let Some(item) = work_items
        .iter()
        .find(|item| project_work_item_status_is_active(item.status.as_str()))
    {
        return Err(HandlerError::bad_request(format!(
            "项目任务正在执行或待执行，请先停止当前执行：{}",
            item.title
        )));
    }
    let links = load_execution_links_for_work_items(base_url, access_token, work_items).await?;
    for link in links
        .iter()
        .filter(|link| task_runner_status_is_active(link.task_runner_status.as_deref()))
    {
        let task = task_runner_api_client::get_task_runner_task(
            contact_runtime.task_runner_base_url.as_str(),
            contact_runtime.task_runner_agent_token.as_str(),
            link.task_runner_task_id.as_str(),
        )
        .await
        .map_err(|err| HandlerError::bad_gateway("校验 Task Runner 任务状态失败", err))?;
        if task_runner_status_is_active(Some(task.status.as_str())) {
            return Err(HandlerError::bad_request(format!(
                "项目任务已有执行中的 Task Runner 任务，请先停止当前执行：{}",
                link.task_runner_task_id
            )));
        }
        sync_execution_link_status(
            base_url,
            project_sync_secret,
            link,
            task.status.as_str(),
            task_runner_callback_event_for_status(task.status.as_str()),
        )
        .await?;
    }
    Ok(())
}

async fn load_execution_links_for_work_items(
    base_url: &str,
    access_token: &str,
    work_items: &[WorkItemPlanItem],
) -> Result<Vec<ExecutionLink>, HandlerError> {
    let mut links = Vec::new();
    for work_item in work_items {
        let values = project_management_api_client::list_work_item_task_runner_links(
            base_url,
            access_token,
            work_item.id.as_str(),
        )
        .await
        .map_err(|err| HandlerError::bad_gateway("读取项目任务执行关联失败", err))?;
        for value in values {
            let Some(task_runner_task_id) = value_string(&value, "task_runner_task_id") else {
                continue;
            };
            links.push(ExecutionLink {
                work_item_id: work_item.id.clone(),
                task_runner_task_id,
                task_runner_run_id: value_string(&value, "task_runner_run_id"),
                task_runner_status: value_string(&value, "task_runner_status"),
                source_session_id: value_string(&value, "source_session_id"),
                source_user_message_id: value_string(&value, "source_user_message_id"),
            });
        }
    }
    Ok(links)
}

async fn sync_requirement_execution_state(
    base_url: &str,
    sync_secret: &str,
    requirement_id: &str,
    requirement_status: Option<&str>,
    work_item_ids: Vec<String>,
    work_item_status: Option<&str>,
    skip_done_work_items: bool,
) -> Result<(), HandlerError> {
    project_management_api_client::sync_requirement_execution_state(
        base_url,
        sync_secret,
        requirement_id,
        &project_management_api_client::SyncRequirementExecutionStateRequest {
            requirement_status: requirement_status.map(ToOwned::to_owned),
            work_item_ids,
            work_item_status: work_item_status.map(ToOwned::to_owned),
            skip_done_work_items,
        },
    )
    .await
    .map(|_| ())
    .map_err(|err| HandlerError::bad_gateway("同步需求执行状态失败", err))
}

async fn sync_execution_link_status(
    base_url: &str,
    sync_secret: &str,
    link: &ExecutionLink,
    task_runner_status: &str,
    callback_event: Option<&str>,
) -> Result<(), HandlerError> {
    project_management_api_client::sync_work_item_task_runner_status(
        base_url,
        sync_secret,
        link.work_item_id.as_str(),
        &project_management_api_client::SyncTaskRunnerWorkItemStatusRequest {
            task_runner_task_id: link.task_runner_task_id.clone(),
            task_runner_run_id: link.task_runner_run_id.clone(),
            task_runner_status: Some(task_runner_status.to_string()),
            last_callback_event: callback_event.map(ToOwned::to_owned),
            last_callback_at: Some(now_rfc3339()),
            last_error_message: None,
            source_session_id: link.source_session_id.clone(),
            source_user_message_id: link.source_user_message_id.clone(),
        },
    )
    .await
    .map(|_| ())
    .map_err(|err| HandlerError::bad_gateway("同步项目任务 Task Runner 状态失败", err))
}

async fn mark_execution_messages_for_stop(links: &[ExecutionLink], overall_status: &str) {
    let mut by_message = BTreeMap::<(String, String), BTreeSet<String>>::new();
    for link in links {
        let Some(session_id) = link.source_session_id.as_deref() else {
            continue;
        };
        let Some(message_id) = link.source_user_message_id.as_deref() else {
            continue;
        };
        by_message
            .entry((session_id.to_string(), message_id.to_string()))
            .or_default()
            .insert(link.task_runner_task_id.clone());
    }
    for ((session_id, message_id), task_ids) in by_message {
        let Ok(Some(session)) = chatos_sessions::get_session_by_id(session_id.as_str()).await
        else {
            continue;
        };
        let Ok(Some(mut message)) =
            conversation_messages::get_message_by_id_in_session(&session, message_id.as_str())
                .await
        else {
            continue;
        };
        let metadata = ensure_message_metadata_object(&mut message);
        let async_meta = metadata
            .entry("task_runner_async".to_string())
            .or_insert_with(|| json!({}));
        if !async_meta.is_object() {
            *async_meta = json!({});
        }
        if let Some(async_meta) = async_meta.as_object_mut() {
            let mut stopped_task_ids = async_meta
                .get("stopped_task_ids")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<BTreeSet<_>>();
            stopped_task_ids.extend(task_ids.into_iter());
            async_meta.insert(
                "overall_status".to_string(),
                Value::String(overall_status.to_string()),
            );
            async_meta.insert("stopped_at".to_string(), Value::String(now_rfc3339()));
            async_meta.insert(
                "stopped_task_ids".to_string(),
                Value::Array(stopped_task_ids.into_iter().map(Value::String).collect()),
            );
        }
        let _ = conversation_messages::upsert_message_in_session(&session, &message).await;
    }
}

fn project_work_item_status_is_active(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "queued" | "running" | "processing" | "in_progress" | "pending"
    )
}

fn task_runner_status_is_active(status: Option<&str>) -> bool {
    matches!(
        status
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "ready" | "queued" | "running" | "processing" | "in_progress" | "pending"
    )
}

fn task_runner_status_is_success(status: Option<&str>) -> bool {
    matches!(
        status
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "succeeded" | "success" | "completed" | "done"
    )
}

fn task_runner_callback_event_for_status(status: &str) -> Option<&'static str> {
    match status.trim().to_ascii_lowercase().as_str() {
        "cancelled" | "canceled" => Some("task.cancelled"),
        "succeeded" | "success" | "completed" | "done" => Some("task.completed"),
        "failed" | "error" => Some("task.failed"),
        "blocked" => Some("task.blocked"),
        _ => None,
    }
}

async fn persist_execution_message_links(
    session: &Session,
    mut message: Message,
    project_id: &str,
    requirement_id: &str,
    created_tasks: &[CreatedExecutionTask],
) -> Result<Message, HandlerError> {
    let message_id = message.id.clone();
    let source_turn_id = message_turn_id(&message).map(ToOwned::to_owned);
    let metadata = ensure_message_metadata_object(&mut message);
    metadata.insert(
        "project_requirement_execution".to_string(),
        json!({
            "project_id": project_id,
            "requirement_id": requirement_id,
            "task_links": created_tasks.iter().map(|item| {
                json!({
                    "project_task_id": item.project_task_id,
                    "requirement_id": item.requirement_id,
                    "task_runner_task_id": item.task_runner_task_id,
                    "task_runner_run_id": item.task_runner_run_id,
                })
            }).collect::<Vec<_>>(),
        }),
    );
    metadata.insert(
        "task_runner_async".to_string(),
        json!({
            "mode": "project_requirement_execution",
            "overall_status": "running",
            "project_id": project_id,
            "requirement_id": requirement_id,
            "source_user_message_id": message_id,
            "source_turn_id": source_turn_id,
            "created_task_ids": created_tasks.iter().map(|item| item.task_runner_task_id.clone()).collect::<Vec<_>>(),
            "running_task_ids": created_tasks.iter().map(|item| item.task_runner_task_id.clone()).collect::<Vec<_>>(),
            "terminal_task_ids": [],
        }),
    );
    conversation_messages::upsert_message_in_session(session, &message)
        .await
        .map_err(|err| HandlerError::internal("更新执行消息失败", err))
}

fn build_task_objective(work_item: &WorkItemPlanItem) -> String {
    let mut parts = Vec::new();
    parts.push(format!("完成项目任务：{}", work_item.title));
    if let Some(description) = work_item.description.as_deref() {
        if !description.trim().is_empty() {
            parts.push(format!("任务说明：{}", description.trim()));
        }
    }
    parts.join("\n\n")
}

fn build_task_description(work_item: &WorkItemPlanItem) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(description) = work_item.description.as_deref() {
        if !description.trim().is_empty() {
            parts.push(format!("## 项目任务说明\n\n{}", description.trim()));
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

fn parse_requirements(values: Vec<Value>) -> Vec<RequirementPlanItem> {
    values
        .into_iter()
        .filter_map(|value| {
            Some(RequirementPlanItem {
                id: value_string(&value, "id")?,
                title: value_string(&value, "title").unwrap_or_else(|| "未命名需求".to_string()),
                status: value_string(&value, "status")
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
                parent_requirement_id: value_string(&value, "parent_requirement_id")
                    .or_else(|| value_string(&value, "parentRequirementId")),
            })
        })
        .collect()
}

fn parse_work_items(values: Vec<Value>) -> Vec<WorkItemPlanItem> {
    values
        .into_iter()
        .filter_map(|value| {
            Some(WorkItemPlanItem {
                id: value_string(&value, "id")?,
                requirement_id: value_string(&value, "requirement_id")
                    .or_else(|| value_string(&value, "requirementId"))?,
                title: value_string(&value, "title")
                    .unwrap_or_else(|| "未命名项目任务".to_string()),
                description: value_string(&value, "description"),
                task_runner_default_model_config_id: value_string(
                    &value,
                    "task_runner_default_model_config_id",
                )
                .or_else(|| value_string(&value, "taskRunnerDefaultModelConfigId"))
                .unwrap_or_default(),
                task_runner_enabled_tool_ids: value_string_vec(
                    &value,
                    "task_runner_enabled_tool_ids",
                )
                .or_else(|| value_string_vec(&value, "taskRunnerEnabledToolIds"))
                .unwrap_or_default(),
                status: value_string(&value, "status")
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
                priority: value_i64(&value, "priority")
                    .and_then(|value| i32::try_from(value).ok())
                    .unwrap_or_default(),
                tags: value_string_vec(&value, "tags").unwrap_or_default(),
            })
        })
        .collect()
}

fn collect_requirement_scope(items: &[RequirementPlanItem], root_id: &str) -> BTreeSet<String> {
    let mut scope = BTreeSet::from([root_id.to_string()]);
    loop {
        let before = scope.len();
        for item in items {
            if item
                .parent_requirement_id
                .as_deref()
                .is_some_and(|parent_id| scope.contains(parent_id))
            {
                scope.insert(item.id.clone());
            }
        }
        if scope.len() == before {
            break;
        }
    }
    scope
}

fn validate_requirement_prerequisites(
    items: &[RequirementPlanItem],
    requirement_scope: &BTreeSet<String>,
    dependency_map: &BTreeMap<String, Vec<String>>,
) -> Result<(), HandlerError> {
    let by_id = items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let mut blockers = Vec::new();
    for requirement_id in requirement_scope {
        let requirement_title = by_id
            .get(requirement_id.as_str())
            .map(|item| item.title.as_str())
            .unwrap_or(requirement_id.as_str());
        for prerequisite_id in dependency_map
            .get(requirement_id.as_str())
            .into_iter()
            .flatten()
            .filter(|prerequisite_id| !requirement_scope.contains(prerequisite_id.as_str()))
        {
            match by_id.get(prerequisite_id.as_str()) {
                Some(prerequisite) if is_done_status(prerequisite.status.as_str()) => {}
                Some(prerequisite) => blockers.push(format!(
                    "{} 的前置需求未完成：{}（{}）",
                    requirement_title, prerequisite.title, prerequisite.status
                )),
                None => blockers.push(format!(
                    "{} 的前置需求不存在或不可见：{}",
                    requirement_title, prerequisite_id
                )),
            }
        }
    }
    if blockers.is_empty() {
        return Ok(());
    }
    blockers.sort();
    blockers.dedup();
    Err(HandlerError::bad_request(format!(
        "存在未完成的前置需求，无法执行：{}",
        blockers.join("；")
    )))
}

fn add_requirement_work_item_dependencies(
    dependency_map: &mut BTreeMap<String, Vec<String>>,
    work_items: &[WorkItemPlanItem],
    requirement_dependency_map: &BTreeMap<String, Vec<String>>,
    requirement_scope: &BTreeSet<String>,
) {
    for work_item in work_items {
        for prerequisite_requirement_id in requirement_dependency_map
            .get(work_item.requirement_id.as_str())
            .into_iter()
            .flatten()
            .filter(|requirement_id| requirement_scope.contains(requirement_id.as_str()))
        {
            for prerequisite_item in work_items.iter().filter(|candidate| {
                candidate.requirement_id == *prerequisite_requirement_id
                    && candidate.id != work_item.id
            }) {
                dependency_map
                    .entry(work_item.id.clone())
                    .or_default()
                    .push(prerequisite_item.id.clone());
            }
        }
    }
    for deps in dependency_map.values_mut() {
        deps.sort();
        deps.dedup();
    }
}

fn topological_work_item_order(
    work_items: &[WorkItemPlanItem],
    dependency_map: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<String>, HandlerError> {
    let work_item_ids = work_items
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    let mut pending = work_item_ids.clone();
    let mut ready_done = BTreeSet::new();
    let mut order = Vec::new();

    while !pending.is_empty() {
        let ready_ids = pending
            .iter()
            .filter(|work_item_id| {
                dependency_map
                    .get(work_item_id.as_str())
                    .into_iter()
                    .flatten()
                    .filter(|dep_id| work_item_ids.contains(dep_id.as_str()))
                    .all(|dep_id| ready_done.contains(dep_id.as_str()))
            })
            .cloned()
            .collect::<Vec<_>>();
        if ready_ids.is_empty() {
            return Err(HandlerError::bad_request(
                "项目任务存在循环前置关系，无法执行",
            ));
        }
        for work_item_id in ready_ids {
            pending.remove(work_item_id.as_str());
            ready_done.insert(work_item_id.clone());
            order.push(work_item_id);
        }
    }

    Ok(order)
}

fn requirement_dependency_map(graph: &Value) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let Some(edges) = graph.get("edges").and_then(Value::as_array) else {
        return out;
    };
    for edge in edges {
        let Some(from) = value_string(edge, "from") else {
            continue;
        };
        let Some(to) = value_string(edge, "to") else {
            continue;
        };
        let Some(prereq_id) = from.strip_prefix("requirement:") else {
            continue;
        };
        let Some(requirement_id) = to.strip_prefix("requirement:") else {
            continue;
        };
        out.entry(requirement_id.to_string())
            .or_default()
            .push(prereq_id.to_string());
    }
    for deps in out.values_mut() {
        deps.sort();
        deps.dedup();
    }
    out
}

fn work_item_dependency_map(graph: &Value) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let Some(edges) = graph.get("edges").and_then(Value::as_array) else {
        return out;
    };
    for edge in edges {
        let Some(from) = value_string(edge, "from") else {
            continue;
        };
        let Some(to) = value_string(edge, "to") else {
            continue;
        };
        let Some(prereq_id) = from.strip_prefix("work_item:") else {
            continue;
        };
        let Some(work_item_id) = to.strip_prefix("work_item:") else {
            continue;
        };
        out.entry(work_item_id.to_string())
            .or_default()
            .push(prereq_id.to_string());
    }
    for deps in out.values_mut() {
        deps.sort();
        deps.dedup();
    }
    out
}

fn is_done_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "done" | "succeeded" | "success" | "completed"
    )
}

fn value_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .and_then(|value| normalize_non_empty(Some(value.to_string())))
}

fn value_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

fn value_string_vec(value: &Value, key: &str) -> Option<Vec<String>> {
    let items = value.get(key)?.as_array()?;
    Some(normalize_tags(
        items
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect(),
    ))
}

fn normalize_tags(values: Vec<String>) -> Vec<String> {
    let mut out = values
        .into_iter()
        .filter_map(|value| normalize_non_empty(Some(value)))
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

impl HandlerError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            error: message.into(),
            detail: None,
        }
    }

    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            error: message.into(),
            detail: None,
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            error: message.into(),
            detail: None,
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            error: message.into(),
            detail: None,
        }
    }

    fn internal(message: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: message.into(),
            detail: Some(detail.into()),
        }
    }

    fn bad_gateway(message: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            error: message.into(),
            detail: Some(detail.into()),
        }
    }
}
