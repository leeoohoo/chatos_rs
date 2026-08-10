// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::project_management_contract::{args::ToolCallParams, tools};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::domain::visibility::{
    ensure_project_task_queryable_for_mcp, ensure_requirement_queryable_for_mcp,
};
use crate::models::*;
use crate::state::AppState;

mod agent_views;
mod conversions;
mod documents;
mod pagination;
mod project;
mod requirements;
mod tasks;

pub(crate) async fn call_tool_from_value(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    value: Value,
) -> Result<Value, String> {
    let params: ToolCallParams = decode_value(value)?;
    call_tool(state, current_user, project_id, params).await
}

async fn call_tool(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    params: ToolCallParams,
) -> Result<Value, String> {
    match params.name.as_str() {
        tools::GET_PROJECT_OVERVIEW => {
            project::get_project_overview(state, current_user, project_id).await
        }
        tools::INITIALIZE_PROJECT => {
            project::initialize_project(state, current_user, project_id, params.arguments).await
        }
        tools::LIST_REQUIREMENTS => {
            requirements::list_requirements(state, current_user, project_id, params.arguments).await
        }
        tools::CREATE_REQUIREMENT => {
            requirements::create_requirement(state, current_user, project_id, params.arguments)
                .await
        }
        tools::UPDATE_REQUIREMENT => {
            requirements::update_requirement(state, current_user, project_id, params.arguments)
                .await
        }
        tools::DELETE_REQUIREMENT => {
            requirements::delete_requirement(state, current_user, project_id, params.arguments)
                .await
        }
        tools::SET_REQUIREMENT_DEPENDENCIES => {
            requirements::set_requirement_dependencies(
                state,
                current_user,
                project_id,
                params.arguments,
            )
            .await
        }
        tools::LIST_REQUIREMENT_TECHNICAL_DOCUMENTS => {
            documents::list_requirement_technical_documents(
                state,
                current_user,
                project_id,
                params.arguments,
            )
            .await
        }
        tools::GET_REQUIREMENT_TECHNICAL_DOCUMENT => {
            documents::get_requirement_technical_document(
                state,
                current_user,
                project_id,
                params.arguments,
            )
            .await
        }
        tools::UPSERT_REQUIREMENT_TECHNICAL_DOCUMENT => {
            documents::upsert_requirement_technical_document(
                state,
                current_user,
                project_id,
                params.arguments,
            )
            .await
        }
        tools::LIST_PROJECT_TASKS => {
            tasks::list_project_tasks(state, current_user, project_id, params.arguments).await
        }
        tools::CREATE_PROJECT_TASK => {
            tasks::create_project_task(state, current_user, project_id, params.arguments).await
        }
        tools::UPDATE_PROJECT_TASK => {
            tasks::update_project_task(state, current_user, project_id, params.arguments).await
        }
        tools::DELETE_PROJECT_TASK => {
            tasks::delete_project_task(state, current_user, project_id, params.arguments).await
        }
        tools::SET_PROJECT_TASK_DEPENDENCIES => {
            tasks::set_project_task_dependencies(state, current_user, project_id, params.arguments)
                .await
        }
        tools::GET_PROJECT_DEPENDENCY_GRAPH => {
            project::get_project_dependency_graph(state, current_user, project_id).await
        }
        name => Err(format!("unknown project management MCP tool: {name}")),
    }
}

async fn require_project_access(
    state: &AppState,
    project_id: &str,
    user: &CurrentUser,
) -> Result<ProjectRecord, String> {
    validate_required("project_id", project_id)?;
    let project = state
        .store
        .get_project(project_id)
        .await?
        .ok_or_else(|| format!("项目不存在: {project_id}"))?;
    if user.can_access_owned_resource(project.owner_user_id.as_deref()) {
        Ok(project)
    } else {
        Err("无权访问该项目".to_string())
    }
}

async fn require_requirement_in_project(
    state: &AppState,
    requirement_id: &str,
    project_id: &str,
    user: &CurrentUser,
) -> Result<RequirementRecord, String> {
    validate_required("project_id", project_id)?;
    validate_required("requirement_id", requirement_id)?;
    let requirement = state
        .store
        .get_requirement(requirement_id)
        .await?
        .ok_or_else(|| format!("需求不存在: {requirement_id}"))?;
    if requirement.project_id != project_id {
        return Err(format!(
            "需求不属于当前项目，requirement_id={requirement_id}"
        ));
    }
    ensure_requirement_queryable_for_mcp(&requirement)?;
    require_project_access(state, project_id, user).await?;
    Ok(requirement)
}

async fn require_project_task_in_project(
    state: &AppState,
    project_task_id: &str,
    project_id: &str,
    user: &CurrentUser,
) -> Result<ProjectWorkItemRecord, String> {
    validate_required("project_id", project_id)?;
    validate_required("project_task_id", project_task_id)?;
    let item = match state.store.get_work_item(project_task_id).await? {
        Some(item) => item,
        None => {
            let candidates = state
                .store
                .list_work_items_by_project(project_id, None, None, None)
                .await?;
            let resolved_id = unique_nearby_project_task_id(project_task_id, &candidates)
                .map(str::to_string)
                .ok_or_else(|| format!("项目任务不存在: {project_task_id}"))?;
            tracing::warn!(
                supplied_project_task_id = project_task_id,
                resolved_project_task_id = resolved_id.as_str(),
                project_id,
                "resolved a mistyped project task UUID inside the authorized project boundary"
            );
            candidates
                .into_iter()
                .find(|candidate| candidate.id == resolved_id)
                .ok_or_else(|| format!("项目任务不存在: {project_task_id}"))?
        }
    };
    if item.project_id != project_id {
        return Err(format!(
            "项目任务不属于当前项目，project_task_id={project_task_id}"
        ));
    }
    ensure_project_task_queryable_for_mcp(&item)?;
    let requirement = state
        .store
        .get_requirement(&item.requirement_id)
        .await?
        .ok_or_else(|| format!("项目任务不存在: {project_task_id}"))?;
    if requirement.project_id != project_id {
        return Err(format!("项目任务不存在: {project_task_id}"));
    }
    if ensure_requirement_queryable_for_mcp(&requirement).is_err() {
        return Err(format!("项目任务不存在: {project_task_id}"));
    }
    require_project_access(state, project_id, user).await?;
    Ok(item)
}

async fn resolve_project_task_ids_in_project(
    state: &AppState,
    project_task_ids: Vec<String>,
    project_id: &str,
    user: &CurrentUser,
) -> Result<Vec<String>, String> {
    let mut resolved = Vec::with_capacity(project_task_ids.len());
    for project_task_id in project_task_ids {
        let item =
            require_project_task_in_project(state, &project_task_id, project_id, user).await?;
        if !resolved.iter().any(|existing| existing == &item.id) {
            resolved.push(item.id);
        }
    }
    Ok(resolved)
}

fn unique_nearby_project_task_id<'a>(
    supplied_id: &str,
    candidates: &'a [ProjectWorkItemRecord],
) -> Option<&'a str> {
    const MAX_DISTANCE: usize = 2;

    let mut best: Option<(&str, usize)> = None;
    let mut best_is_ambiguous = false;
    for candidate in candidates {
        let Some(distance) = uuid_damerau_levenshtein(supplied_id, candidate.id.as_str()) else {
            continue;
        };
        if distance > MAX_DISTANCE {
            continue;
        }
        match best {
            None => {
                best = Some((candidate.id.as_str(), distance));
                best_is_ambiguous = false;
            }
            Some((_, best_distance)) if distance < best_distance => {
                best = Some((candidate.id.as_str(), distance));
                best_is_ambiguous = false;
            }
            Some((_, best_distance)) if distance == best_distance => {
                best_is_ambiguous = true;
            }
            Some(_) => {}
        }
    }
    best.filter(|_| !best_is_ambiguous).map(|(id, _)| id)
}

fn uuid_damerau_levenshtein(left: &str, right: &str) -> Option<usize> {
    let left = Uuid::parse_str(left)
        .ok()?
        .simple()
        .to_string()
        .into_bytes();
    let right = Uuid::parse_str(right)
        .ok()?
        .simple()
        .to_string()
        .into_bytes();
    let mut distance = vec![vec![0usize; right.len() + 1]; left.len() + 1];
    for (index, row) in distance.iter_mut().enumerate() {
        row[0] = index;
    }
    for index in 0..=right.len() {
        distance[0][index] = index;
    }
    for left_index in 1..=left.len() {
        for right_index in 1..=right.len() {
            let substitution_cost = usize::from(left[left_index - 1] != right[right_index - 1]);
            let mut value = distance[left_index - 1][right_index]
                .saturating_add(1)
                .min(distance[left_index][right_index - 1].saturating_add(1))
                .min(distance[left_index - 1][right_index - 1].saturating_add(substitution_cost));
            if left_index > 1
                && right_index > 1
                && left[left_index - 1] == right[right_index - 2]
                && left[left_index - 2] == right[right_index - 1]
            {
                value = value.min(distance[left_index - 2][right_index - 2].saturating_add(1));
            }
            distance[left_index][right_index] = value;
        }
    }
    Some(distance[left.len()][right.len()])
}

fn ensure_project_writable(project: &ProjectRecord) -> Result<(), String> {
    if project.status == ProjectStatus::Archived {
        Err("项目已归档，不能继续写入".to_string())
    } else {
        Ok(())
    }
}

fn ensure_requirement_mutable_for_mcp(requirement: &RequirementRecord) -> Result<(), String> {
    if requirement.status == RequirementStatus::Done {
        Err(format!(
            "需求已完成，不能通过 MCP 修改、删除或追加内容；如有相似的新需求，请新建需求。requirement_id={}",
            requirement.id
        ))
    } else {
        Ok(())
    }
}

fn ensure_project_task_mutable_for_mcp(item: &ProjectWorkItemRecord) -> Result<(), String> {
    if item.status == ProjectWorkItemStatus::Done {
        Err(format!(
            "项目任务已完成，不能通过 MCP 修改、删除或调整依赖；如有相似的新工作，请新建项目任务。project_task_id={}",
            item.id
        ))
    } else {
        Ok(())
    }
}

fn tool_text_result(payload: Value) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
            }
        ],
        "isError": false
    })
}

fn decode_value<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use crate::config::AppConfig;
    use crate::models::UserRole;

    fn project_task_candidate(id: &str) -> ProjectWorkItemRecord {
        ProjectWorkItemRecord {
            id: id.to_string(),
            project_id: "project-1".to_string(),
            requirement_id: "requirement-1".to_string(),
            title: "Task".to_string(),
            description: None,
            status: ProjectWorkItemStatus::Todo,
            priority: 0,
            assignee_user_id: None,
            estimate_points: None,
            due_at: None,
            sort_order: 0,
            tags: Vec::new(),
            is_planning_task: false,
            creator_user_id: Some("user-1".to_string()),
            creator_username: Some("owner".to_string()),
            creator_display_name: Some("Owner".to_string()),
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            archived_at: None,
        }
    }

    #[test]
    fn uniquely_corrects_the_observed_uuid_copy_error_inside_one_project() {
        let candidates = vec![
            project_task_candidate("2f45ed1e-d368-434c-ada5-e49405a38ad0"),
            project_task_candidate("93fc86c6-2d25-4bf8-9f2d-aef194c3e76a"),
        ];

        assert_eq!(
            uuid_damerau_levenshtein(
                "2f45ed1e-d368-434e-ad5a-e49405a38ad0",
                "2f45ed1e-d368-434c-ada5-e49405a38ad0",
            ),
            Some(2)
        );
        assert_eq!(
            unique_nearby_project_task_id("2f45ed1e-d368-434e-ad5a-e49405a38ad0", &candidates,),
            Some("2f45ed1e-d368-434c-ada5-e49405a38ad0")
        );
    }

    #[test]
    fn refuses_ambiguous_or_non_uuid_project_task_corrections() {
        let candidates = vec![
            project_task_candidate("00000000-0000-4000-8000-000000000001"),
            project_task_candidate("00000000-0000-4000-8000-000000000002"),
        ];

        assert_eq!(
            unique_nearby_project_task_id("00000000-0000-4000-8000-000000000000", &candidates,),
            None
        );
        assert_eq!(
            unique_nearby_project_task_id("not-a-uuid", &candidates),
            None
        );
    }

    async fn test_state() -> AppState {
        let base_url = std::env::var("PROJECT_SERVICE_TEST_MONGODB_BASE_URL")
            .unwrap_or_else(|_| "mongodb://admin:admin@127.0.0.1:27018".to_string());
        let database = format!("project_management_mcp_tools_{}", Uuid::new_v4().simple());
        AppState::new(AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            otlp_endpoint: "http://127.0.0.1:4317".to_string(),
            otlp_trace_sample_ratio: 0.0,
            otlp_export_timeout: Duration::from_secs(1),
            database_url: format!(
                "{}/{database}?authSource=admin",
                base_url.trim_end_matches('/')
            ),
            mcp_result_rabbitmq_url: "amqp://127.0.0.1:1/%2f".to_string(),
            mcp_result_queue_prefix: "project_service.mcp.results.test".to_string(),
            user_service_base_url: "http://127.0.0.1:1".to_string(),
            user_service_internal_base_url: "https://127.0.0.1:1".to_string(),
            user_service_internal_http_client: reqwest::Client::new(),
            user_service_request_timeout: Duration::from_millis(300),
            user_service_internal_secret: None,
            local_connector_service_base_url: "http://127.0.0.1:1".to_string(),
            local_connector_http_client: reqwest::Client::new(),
            local_connector_service_request_timeout: Duration::from_millis(300),
            memory_engine_base_url: "http://127.0.0.1:1/api/memory-engine/v1".to_string(),
            memory_engine_http_client: reqwest::Client::new(),
            memory_engine_source_id: "project_management_agent".to_string(),
            memory_engine_internal_api_secret: None,
            memory_engine_request_timeout: Duration::from_millis(300),
            sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_http_client: reqwest::Client::new(),
            sandbox_manager_client_id: None,
            sandbox_manager_client_key: None,
            sandbox_image_mcp_request_timeout: Duration::from_millis(300),
            cloud_project_import_enabled: true,
            cloud_project_max_zip_bytes: 1024 * 1024,
            cloud_project_max_unpacked_bytes: 1024 * 1024,
            cloud_project_max_files: 100,
            cloud_project_git_timeout: Duration::from_millis(300),
            environment_analysis_timeout: Duration::from_secs(60),
            environment_analysis_stale_after: Duration::from_secs(60),
            task_runner_base_url: None,
            task_runner_request_timeout: Duration::from_millis(300),
            task_runner_internal_secret: None,
            sync_secret: None,
            internal_api_secrets: HashMap::new(),
            require_signed_internal_requests: false,
        })
        .await
        .expect("test state")
    }

    fn test_user() -> CurrentUser {
        CurrentUser {
            principal_type: "human_user".to_string(),
            id: "user-1".to_string(),
            username: "owner".to_string(),
            display_name: "Owner".to_string(),
            role: UserRole::Agent,
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
        }
    }

    async fn create_project(state: &AppState, user: &CurrentUser) -> ProjectRecord {
        state
            .store
            .create_project(
                CreateProjectRequest {
                    name: "Project".to_string(),
                    root_path: None,
                    git_url: None,
                    description: None,
                    sandbox_enabled: None,
                    source_type: None,
                    cloud_import_source: None,
                    import_status: None,
                    source_git_url: None,
                },
                user,
            )
            .await
            .expect("create project")
    }

    async fn create_requirement(
        state: &AppState,
        user: &CurrentUser,
        project_id: &str,
        title: &str,
    ) -> RequirementRecord {
        state
            .store
            .create_requirement(
                project_id,
                CreateRequirementRequest {
                    parent_requirement_id: None,
                    requirement_type: None,
                    title: title.to_string(),
                    summary: None,
                    detail: None,
                    business_value: None,
                    acceptance_criteria: None,
                    source: None,
                    priority: None,
                    status: None,
                    assignee_user_id: None,
                },
                user,
            )
            .await
            .expect("create requirement")
    }

    async fn add_technical_document(state: &AppState, user: &CurrentUser, requirement_id: &str) {
        state
            .store
            .create_requirement_document(
                requirement_id,
                UpsertRequirementDocumentRequest {
                    doc_type: None,
                    title: None,
                    format: None,
                    content: "Technical overview".to_string(),
                },
                user,
            )
            .await
            .expect("create requirement document");
    }

    async fn create_project_task(
        state: &AppState,
        user: &CurrentUser,
        requirement: &RequirementRecord,
    ) -> ProjectWorkItemRecord {
        add_technical_document(state, user, &requirement.id).await;
        state
            .store
            .create_work_item(
                requirement,
                CreateProjectWorkItemRequest {
                    title: "Full Maven build".to_string(),
                    description: None,
                    status: None,
                    priority: None,
                    assignee_user_id: None,
                    estimate_points: None,
                    due_at: None,
                    sort_order: None,
                    tags: None,
                    is_planning_task: false,
                },
                user,
            )
            .await
            .expect("create work item")
    }

    async fn call_test_tool(
        state: &AppState,
        user: &CurrentUser,
        project_id: &str,
        name: &str,
        arguments: Value,
    ) -> Result<Value, String> {
        call_tool(
            state,
            user,
            project_id,
            ToolCallParams {
                name: name.to_string(),
                arguments,
            },
        )
        .await
    }

    fn assert_done_record_rejected(result: Result<Value, String>) {
        let error = result.expect_err("done record mutation rejected");
        assert!(
            error.contains("已完成") || error.contains("done"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    #[ignore = "requires MongoDB"]
    async fn mcp_rejects_mutating_done_requirement() {
        let state = test_state().await;
        let user = test_user();
        let project = create_project(&state, &user).await;
        let requirement = create_requirement(&state, &user, &project.id, "Requirement").await;
        state
            .store
            .update_requirement(
                &requirement.id,
                UpdateRequirementRequest {
                    status: Some(RequirementStatus::Done),
                    ..Default::default()
                },
            )
            .await
            .expect("mark requirement done");

        assert_done_record_rejected(
            call_test_tool(
                &state,
                &user,
                &project.id,
                tools::UPDATE_REQUIREMENT,
                json!({
                    "requirement_id": requirement.id,
                    "patch": { "title": "Changed" }
                }),
            )
            .await,
        );
        assert_done_record_rejected(
            call_test_tool(
                &state,
                &user,
                &project.id,
                tools::UPSERT_REQUIREMENT_TECHNICAL_DOCUMENT,
                json!({
                    "requirement_id": requirement.id,
                    "content": "Updated technical overview"
                }),
            )
            .await,
        );
        assert_done_record_rejected(
            call_test_tool(
                &state,
                &user,
                &project.id,
                tools::CREATE_PROJECT_TASK,
                json!({
                    "requirement_id": requirement.id,
                    "title": "Full Maven build"
                }),
            )
            .await,
        );
        assert_done_record_rejected(
            call_test_tool(
                &state,
                &user,
                &project.id,
                tools::DELETE_REQUIREMENT,
                json!({ "requirement_id": requirement.id }),
            )
            .await,
        );
    }

    #[tokio::test]
    #[ignore = "requires MongoDB"]
    async fn mcp_rejects_attaching_new_records_to_done_requirement() {
        let state = test_state().await;
        let user = test_user();
        let project = create_project(&state, &user).await;
        let done_parent = create_requirement(&state, &user, &project.id, "Done parent").await;
        let child = create_requirement(&state, &user, &project.id, "Open child").await;
        state
            .store
            .update_requirement(
                &done_parent.id,
                UpdateRequirementRequest {
                    status: Some(RequirementStatus::Done),
                    ..Default::default()
                },
            )
            .await
            .expect("mark parent done");

        assert_done_record_rejected(
            call_test_tool(
                &state,
                &user,
                &project.id,
                tools::CREATE_REQUIREMENT,
                json!({
                    "parent_requirement_id": done_parent.id,
                    "title": "New child under done parent"
                }),
            )
            .await,
        );
        assert_done_record_rejected(
            call_test_tool(
                &state,
                &user,
                &project.id,
                tools::UPDATE_REQUIREMENT,
                json!({
                    "requirement_id": child.id,
                    "patch": { "parent_requirement_id": done_parent.id }
                }),
            )
            .await,
        );
    }

    #[tokio::test]
    #[ignore = "requires MongoDB"]
    async fn mcp_rejects_mutating_done_project_task() {
        let state = test_state().await;
        let user = test_user();
        let project = create_project(&state, &user).await;
        let requirement = create_requirement(&state, &user, &project.id, "Requirement").await;
        let item = create_project_task(&state, &user, &requirement).await;
        state
            .store
            .update_work_item(
                &item.id,
                UpdateProjectWorkItemRequest {
                    status: Some(ProjectWorkItemStatus::Done),
                    ..Default::default()
                },
            )
            .await
            .expect("mark work item done");

        assert_done_record_rejected(
            call_test_tool(
                &state,
                &user,
                &project.id,
                tools::UPDATE_PROJECT_TASK,
                json!({
                    "project_task_id": item.id,
                    "patch": { "title": "Changed" }
                }),
            )
            .await,
        );
        assert_done_record_rejected(
            call_test_tool(
                &state,
                &user,
                &project.id,
                tools::SET_PROJECT_TASK_DEPENDENCIES,
                json!({
                    "project_task_id": item.id,
                    "prerequisite_project_task_ids": []
                }),
            )
            .await,
        );
        assert_done_record_rejected(
            call_test_tool(
                &state,
                &user,
                &project.id,
                tools::DELETE_PROJECT_TASK,
                json!({ "project_task_id": item.id }),
            )
            .await,
        );
    }
}
