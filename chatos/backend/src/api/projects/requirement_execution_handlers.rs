// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{extract::Path, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

use crate::api::chat_stream_common::ChatStreamRequest;
use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::project_access::ensure_owned_project;
use crate::modules::conversation_runtime::chat_usecase::{run_chat_usecase, RunChatUsecaseInput};
use crate::services::{access_token_scope, project_management_api_client, task_runner_api_client};

use super::requirement_execution::{
    add_requirement_work_item_dependencies, collect_requirement_execution_scope,
    create_execution_message, ensure_requirement_execution_not_active, is_done_status,
    load_execution_links_for_work_items, mark_execution_messages_for_stop, parse_requirements,
    parse_work_items, project_plan_array, project_plan_value, requirement_dependency_map,
    resolve_or_create_execution_session, select_contact_runtime, sync_execution_link_status,
    sync_requirement_execution_state, task_runner_callback_event_for_status,
    task_runner_status_is_active, task_runner_status_is_success, topological_work_item_order,
    validate_requirement_prerequisites, value_string, work_item_dependency_map, HandlerError,
    RequirementPlanItem, WorkItemPlanItem,
};

#[derive(Debug, Default, Deserialize)]
pub(super) struct ExecuteRequirementRequest {
    contact_id: Option<String>,
    #[serde(default, alias = "includePrerequisiteDependents")]
    include_prerequisite_dependents: bool,
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
    let requirement_scope = collect_requirement_execution_scope(
        &requirement_items,
        requirement_id.as_str(),
        &requirement_dependency_map,
        req.include_prerequisite_dependents,
    );
    validate_requirement_prerequisites(
        &requirement_items,
        &requirement_scope,
        &requirement_dependency_map,
    )?;
    let mut dependency_map = work_item_dependency_map(&dependency_graph);
    let mut selected_work_items = all_work_items
        .iter()
        .filter(|item| requirement_scope.contains(item.requirement_id.as_str()))
        .filter(|item| item.status != "archived")
        .filter(|item| !is_done_status(item.status.as_str()))
        .cloned()
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
            "该需求执行范围内没有需要执行的未完成项目任务",
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
    let requirement_documents = load_requirement_documents_for_scope(
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        &requirement_scope,
    )
    .await?;
    let planner_prompt = build_requirement_execution_planner_prompt(
        project.id.as_str(),
        &root_requirement,
        &requirement_items,
        &requirement_scope,
        &selected_work_items,
        &creation_order,
        &dependency_map,
        &requirement_documents,
    );
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
        planner_prompt.clone(),
    )
    .await?;

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

    let execution_group_id = message.id.clone();
    let chat_req = ChatStreamRequest {
        conversation_id: Some(session.id.clone()),
        content: Some(planner_prompt),
        model_config_id: None,
        ai_model_config: None,
        user_id: Some(auth.user_id.clone()),
        attachments: None,
        reasoning_enabled: None,
        plan_mode: false,
        turn_id: Some(execution_group_id.clone()),
        contact_agent_id: Some(contact_runtime.contact.agent_id.clone()),
        project_id: Some(project.id.clone()),
        project_root: Some(project.root_path.clone()),
        workspace_root: Some(project.root_path.clone()),
        remote_connection_id: None,
        user_message_id: Some(execution_group_id.clone()),
        project_requirement_execution_planner: true,
    };
    access_token_scope::spawn_with_current_access_token(run_chat_usecase(RunChatUsecaseInput {
        sender: None,
        req: chat_req,
    }));

    Ok(json!({
        "success": true,
        "status": "planning_started",
        "project_id": project.id,
        "requirement_id": requirement_id,
        "contact_id": contact_runtime.contact.contact_id,
        "conversation_id": session.id,
        "message_id": execution_group_id.clone(),
        "message": message,
        "execution_group_id": execution_group_id,
        "planner_agent_key": "project_requirement_execution_planner_agent",
        "plan_mode_enabled": false,
    }))
}

async fn load_requirement_documents_for_scope(
    base_url: &str,
    access_token: &str,
    requirement_scope: &BTreeSet<String>,
) -> Result<BTreeMap<String, Value>, HandlerError> {
    let mut out = BTreeMap::new();
    for requirement_id in requirement_scope {
        let documents = project_management_api_client::list_project_service_requirement_documents(
            base_url,
            access_token,
            requirement_id.as_str(),
        )
        .await
        .map_err(|err| HandlerError::bad_gateway("读取需求技术文档失败", err))?;
        out.insert(requirement_id.clone(), documents);
    }
    Ok(out)
}

fn build_requirement_execution_planner_prompt(
    project_id: &str,
    root_requirement: &RequirementPlanItem,
    requirement_items: &[RequirementPlanItem],
    requirement_scope: &BTreeSet<String>,
    selected_work_items: &[WorkItemPlanItem],
    creation_order: &[String],
    dependency_map: &BTreeMap<String, Vec<String>>,
    requirement_documents: &BTreeMap<String, Value>,
) -> String {
    let scoped_requirements = requirement_items
        .iter()
        .filter(|item| requirement_scope.contains(item.id.as_str()))
        .map(|item| {
            json!({
                "id": item.id.as_str(),
                "title": item.title.as_str(),
                "status": item.status.as_str(),
                "parent_requirement_id": item.parent_requirement_id.as_deref(),
            })
        })
        .collect::<Vec<_>>();
    let work_items = selected_work_items
        .iter()
        .map(|item| {
            json!({
                "id": item.id.as_str(),
                "requirement_id": item.requirement_id.as_str(),
                "title": item.title.as_str(),
                "description": item.description.as_deref(),
                "status": item.status.as_str(),
                "priority": item.priority,
                "tags": &item.tags,
                "is_planning_task": item.is_planning_task,
                "prerequisite_project_task_ids": dependency_map
                    .get(item.id.as_str())
                    .cloned()
                    .unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();
    let payload = json!({
        "project_id": project_id,
        "requirement": {
            "id": root_requirement.id.as_str(),
            "title": root_requirement.title.as_str(),
            "status": root_requirement.status.as_str(),
        },
        "requirements_in_execution_scope": scoped_requirements,
        "selected_project_tasks": work_items,
        "recommended_project_task_creation_order": creation_order,
        "technical_documents_by_requirement": requirement_documents,
    });
    let payload = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
    format!(
        r#"你是项目需求执行规划 Agent。你的任务是根据下面的需求、技术文档、项目任务和依赖关系，把项目任务拆成具体的 Task Runner 执行任务。

硬性规则：
1. 只能通过 `create_project_execution_tasks` 创建执行任务。
2. 每个创建的执行任务都必须填写 `project_task_id`，绑定到对应项目任务。
3. 一个项目任务可以拆成多个执行任务；不要保持项目任务和执行任务的一对一假设。
4. 不要调用任何工具把项目任务或需求直接改成 done/failed/blocked；执行完成后的状态传播由程序回调处理。
5. 创建执行任务时要设置清晰的 title、objective、description，并用 prerequisite_refs 表达执行任务之间的先后关系。
6. `create_project_execution_tasks.project_id` 必须使用 `{project_id}`，`requirement_id` 必须使用 `{requirement_id}`。
7. 如果某个项目任务无法拆分或无法创建执行任务，最终总结里说明原因，但不要伪造完成状态。

项目执行上下文 JSON：

```json
{payload}
```
"#,
        project_id = project_id,
        requirement_id = root_requirement.id,
        payload = payload,
    )
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
    let requirement_scope = collect_requirement_execution_scope(
        &requirement_items,
        requirement_id.as_str(),
        &requirement_dependency_map,
        false,
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
    let mut links = load_execution_links_for_work_items(
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        &selected_work_items,
    )
    .await?;
    for link in links
        .iter_mut()
        .filter(|link| task_runner_status_is_active(link.task_runner_status.as_deref()))
    {
        let task = task_runner_api_client::get_task_runner_task(
            contact_runtime.task_runner_base_url.as_str(),
            contact_runtime.task_runner_agent_token.as_str(),
            link.task_runner_task_id.as_str(),
        )
        .await
        .map_err(|err| HandlerError::bad_gateway("校验 Task Runner 任务状态失败", err))?;
        link.task_runner_status = Some(task.status.clone());
        sync_execution_link_status(
            cfg.project_service_base_url.as_str(),
            project_sync_secret.as_str(),
            link,
            task.status.as_str(),
            task_runner_callback_event_for_status(task.status.as_str()),
        )
        .await?;
    }
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
