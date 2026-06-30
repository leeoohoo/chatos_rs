use axum::{extract::Path, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::project_access::ensure_owned_project;
use crate::services::{access_token_scope, project_management_api_client, task_runner_api_client};

use super::requirement_execution::{
    add_requirement_work_item_dependencies, collect_downstream_requirement_scope,
    create_and_start_execution_tasks, create_execution_message,
    ensure_requirement_execution_not_active, is_done_status, load_execution_links_for_work_items,
    load_external_prerequisite_task_ids, load_task_runner_builtin_prompt_locale,
    mark_execution_messages_for_stop, parse_requirements, parse_work_items,
    persist_execution_message_links, project_plan_array, project_plan_value,
    requirement_dependency_map, resolve_or_create_execution_session, select_contact_runtime,
    sync_execution_link_status, sync_requirement_execution_state,
    task_runner_callback_event_for_status, task_runner_status_is_active,
    task_runner_status_is_success, topological_work_item_order, validate_requirement_prerequisites,
    value_string, work_item_dependency_map, HandlerError,
};

#[derive(Debug, Default, Deserialize)]
pub(super) struct ExecuteRequirementRequest {
    contact_id: Option<String>,
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

    let plan = project_management_api_client::get_project_service_plan(
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        project.id.as_str(),
        false,
    )
    .await
    .map_err(|err| HandlerError::bad_gateway("read project plan snapshot failed", err))?;

    let requirement_items =
        parse_requirements(project_plan_array(&plan, "requirements", "requirements"));
    let Some(root_requirement) = requirement_items
        .iter()
        .find(|item| item.id == requirement_id)
        .cloned()
    else {
        return Err(HandlerError::not_found("需求不存在"));
    };
    let all_work_items = parse_work_items(project_plan_array(&plan, "work_items", "workItems"));
    let dependency_graph = project_plan_value(&plan, "dependency_graph", "dependencyGraph");
    let requirement_dependency_map = requirement_dependency_map(&dependency_graph);
    let requirement_scope = collect_downstream_requirement_scope(
        &requirement_items,
        requirement_id.as_str(),
        &requirement_dependency_map,
    );
    validate_requirement_prerequisites(
        &requirement_items,
        &requirement_scope,
        &requirement_dependency_map,
    )?;
    let mut dependency_map = work_item_dependency_map(&dependency_graph);
    let mut selected_work_items = all_work_items
        .iter()
        .cloned()
        .filter(|item| requirement_scope.contains(item.requirement_id.as_str()))
        .filter(|item| item.status != "archived")
        .filter(|item| !is_done_status(item.status.as_str()))
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
        return Err(HandlerError::bad_request(
            "该需求及其向下关联范围内没有需要执行的未完成项目任务",
        ));
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

    let mut executing_requirement_ids = BTreeSet::from([root_requirement.id.clone()]);
    for item in &selected_work_items {
        executing_requirement_ids.insert(item.requirement_id.clone());
    }
    for executing_requirement_id in &executing_requirement_ids {
        sync_requirement_execution_state(
            cfg.project_service_base_url.as_str(),
            project_sync_secret.as_str(),
            executing_requirement_id.as_str(),
            Some("in_progress"),
            Vec::new(),
            None,
            false,
        )
        .await?;
    }

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

    let plan = project_management_api_client::get_project_service_plan(
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        project.id.as_str(),
        false,
    )
    .await
    .map_err(|err| HandlerError::bad_gateway("read project plan snapshot failed", err))?;
    let requirement_items =
        parse_requirements(project_plan_array(&plan, "requirements", "requirements"));
    let Some(root_requirement) = requirement_items
        .iter()
        .find(|item| item.id == requirement_id)
        .cloned()
    else {
        return Err(HandlerError::not_found("需求不存在"));
    };
    let dependency_graph = project_plan_value(&plan, "dependency_graph", "dependencyGraph");
    let requirement_dependency_map = requirement_dependency_map(&dependency_graph);
    let requirement_scope = collect_downstream_requirement_scope(
        &requirement_items,
        requirement_id.as_str(),
        &requirement_dependency_map,
    );
    let selected_work_items =
        parse_work_items(project_plan_array(&plan, "work_items", "workItems"))
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

    let mut work_item_ids_by_requirement = BTreeMap::<String, Vec<String>>::new();
    for item in &selected_work_items {
        work_item_ids_by_requirement
            .entry(item.requirement_id.clone())
            .or_default()
            .push(item.id.clone());
    }
    let work_item_ids = work_item_ids_by_requirement
        .values()
        .flat_map(|ids| ids.iter().cloned())
        .collect::<Vec<_>>();
    let requirement_status_by_id = requirement_items
        .iter()
        .map(|item| (item.id.as_str(), item.status.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut reset_requirement_ids = BTreeSet::new();
    if root_requirement.status == "in_progress" {
        reset_requirement_ids.insert(root_requirement.id.clone());
    }
    for item in &selected_work_items {
        if requirement_status_by_id
            .get(item.requirement_id.as_str())
            .is_some_and(|status| *status == "in_progress")
        {
            reset_requirement_ids.insert(item.requirement_id.clone());
        }
    }
    for reset_requirement_id in &reset_requirement_ids {
        let requirement_work_item_ids = work_item_ids_by_requirement
            .remove(reset_requirement_id.as_str())
            .unwrap_or_default();
        sync_requirement_execution_state(
            cfg.project_service_base_url.as_str(),
            project_sync_secret.as_str(),
            reset_requirement_id.as_str(),
            Some("approved"),
            requirement_work_item_ids,
            Some("ready"),
            true,
        )
        .await?;
    }
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
