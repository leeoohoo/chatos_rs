// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::AppConfig;
use crate::http_body::{read_response_text_limited_or_message, ERROR_BODY_PREVIEW_LIMIT_BYTES};
use crate::models::{
    CreateTaskRunnerTaskFromWorkItemRequest, ProjectWorkItemRecord, TaskRunnerTaskRecord,
};

#[derive(Debug, Serialize)]
struct CreateTaskRunnerTaskRequest<'a> {
    title: String,
    description: Option<String>,
    objective: String,
    input_payload: Option<Value>,
    priority: Option<i32>,
    tags: Option<Vec<String>>,
    default_model_config_id: Option<String>,
    task_profile: Option<&'a str>,
    mcp_config: Option<TaskRunnerMcpConfig>,
    project_id: Option<&'a str>,
    source_session_id: Option<String>,
    source_user_message_id: Option<String>,
    prerequisite_task_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
struct TaskRunnerMcpConfig {
    enabled_builtin_kinds: Vec<String>,
    external_mcp_config_ids: Vec<String>,
    skill_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TaskRunnerInternalExecutionOptions {
    #[serde(default)]
    model_config_ids: Vec<String>,
    #[serde(default)]
    builtin_tool_ids: Vec<String>,
    #[serde(default)]
    external_tool_ids: Vec<String>,
    #[serde(default)]
    skill_ids: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TaskRunnerExecutionOptions {
    model_config_ids: BTreeSet<String>,
    builtin_tool_ids: BTreeSet<String>,
    external_tool_ids: BTreeSet<String>,
    skill_ids: BTreeSet<String>,
}

impl TaskRunnerExecutionOptions {
    #[cfg(test)]
    pub fn for_test(
        model_config_ids: impl IntoIterator<Item = impl Into<String>>,
        builtin_tool_ids: impl IntoIterator<Item = impl Into<String>>,
        external_tool_ids: impl IntoIterator<Item = impl Into<String>>,
        skill_ids: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            model_config_ids: model_config_ids.into_iter().map(Into::into).collect(),
            builtin_tool_ids: builtin_tool_ids.into_iter().map(Into::into).collect(),
            external_tool_ids: external_tool_ids.into_iter().map(Into::into).collect(),
            skill_ids: skill_ids.into_iter().map(Into::into).collect(),
        }
    }

    pub fn model_config_ids(&self) -> Vec<String> {
        self.model_config_ids.iter().cloned().collect()
    }

    pub fn tool_ids(&self) -> Vec<String> {
        self.builtin_tool_ids
            .iter()
            .chain(self.external_tool_ids.iter())
            .cloned()
            .collect()
    }

    pub fn skill_ids(&self) -> Vec<String> {
        self.skill_ids.iter().cloned().collect()
    }

    pub fn validate_model_config_id(&self, value: &str) -> Result<String, String> {
        let value = normalized_required("task_runner_default_model_config_id", value)?;
        if self.model_config_ids.contains(value.as_str()) {
            Ok(value)
        } else {
            Err(format!("Task Runner 模型配置不可用或无权限访问: {value}"))
        }
    }

    pub fn mcp_config_for_tool_ids(&self, values: &[String]) -> Result<Value, String> {
        let values = normalize_tool_ids(values.to_vec())?;
        let mut enabled_builtin_kinds = Vec::new();
        let mut external_mcp_config_ids = Vec::new();
        for value in values {
            if self.builtin_tool_ids.contains(value.as_str()) {
                enabled_builtin_kinds.push(value);
            } else if self.external_tool_ids.contains(value.as_str()) {
                external_mcp_config_ids.push(value);
            } else {
                return Err(format!("Task Runner 工具不可用或无权限访问: {value}"));
            }
        }
        Ok(json!({
            "enabled_builtin_kinds": enabled_builtin_kinds,
            "external_mcp_config_ids": external_mcp_config_ids,
            "skill_ids": [],
        }))
    }

    pub fn validate_skill_ids(&self, values: Vec<String>) -> Result<Vec<String>, String> {
        let values = normalize_id_list(values);
        for value in &values {
            if !self.skill_ids.contains(value.as_str()) {
                return Err(format!("Task Runner Skill 不可用或无权限访问: {value}"));
            }
        }
        Ok(values)
    }
}

pub async fn create_task_from_work_item(
    config: &AppConfig,
    access_token: &str,
    work_item: &ProjectWorkItemRecord,
    input: CreateTaskRunnerTaskFromWorkItemRequest,
) -> Result<TaskRunnerTaskRecord, String> {
    let base_url = config
        .task_runner_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "task runner base url is not configured".to_string())?;
    let endpoint = format!("{}/api/tasks", base_url.trim_end_matches('/'));
    let owner_user_id = work_item
        .owner_user_id
        .as_deref()
        .or(work_item.creator_user_id.as_deref())
        .ok_or_else(|| "project work item missing owner_user_id".to_string())?;
    let execution_options = fetch_execution_options(config, owner_user_id).await?;
    let default_model_config_id = normalized_optional(input.default_model_config_id)
        .unwrap_or_else(|| work_item.task_runner_default_model_config_id.clone());
    let default_model_config_id =
        Some(execution_options.validate_model_config_id(default_model_config_id.as_str())?);
    let mut mcp_config = task_runner_mcp_config_from_value(
        execution_options.mcp_config_for_tool_ids(&work_item.task_runner_enabled_tool_ids)?,
    )?;
    mcp_config.skill_ids =
        execution_options.validate_skill_ids(work_item.task_runner_skill_ids.clone())?;
    let source_session_id = normalized_optional(input.source_session_id);
    let source_user_message_id = normalized_optional(input.source_user_message_id);
    let task_profile = if work_item.is_planning_task {
        "chatos_plan"
    } else {
        "default"
    };
    let payload = CreateTaskRunnerTaskRequest {
        title: normalized_optional(input.title).unwrap_or_else(|| work_item.title.clone()),
        description: normalized_optional(input.description)
            .or_else(|| work_item.description.clone()),
        objective: normalized_optional(input.objective)
            .unwrap_or_else(|| default_task_objective(work_item)),
        input_payload: Some(json!({
            "source": "project_management_service",
            "project_id": work_item.project_id,
            "requirement_id": work_item.requirement_id,
            "project_work_item_id": work_item.id,
            "is_planning_task": work_item.is_planning_task,
        })),
        priority: input
            .priority
            .or_else(|| i32::try_from(work_item.priority).ok()),
        tags: Some(normalize_tags(
            input.tags.unwrap_or_else(|| work_item.tags.clone()),
        )),
        default_model_config_id,
        task_profile: Some(task_profile),
        mcp_config: Some(mcp_config),
        project_id: Some(work_item.project_id.as_str()),
        source_session_id: source_session_id.clone(),
        source_user_message_id: source_user_message_id.clone(),
        prerequisite_task_ids: input.prerequisite_task_ids.map(normalize_tags),
    };
    let client = reqwest::Client::builder()
        .timeout(config.task_runner_request_timeout)
        .build()
        .map_err(|err| format!("build task runner client failed: {err}"))?;
    let mut request = client
        .post(endpoint)
        .bearer_auth(access_token.trim())
        .json(&payload);
    if let Some(value) = source_session_id.as_deref() {
        request = request.header("X-Chatos-Session-Id", value);
    }
    if let Some(value) = source_user_message_id.as_deref() {
        request = request.header("X-Chatos-User-Message-Id", value);
    }
    let response = request
        .send()
        .await
        .map_err(|err| format!("task runner request failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        let body =
            read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
        return Err(if body.trim().is_empty() {
            format!("task runner request failed with status {status}")
        } else {
            body
        });
    }
    response
        .json::<TaskRunnerTaskRecord>()
        .await
        .map_err(|err| format!("parse task runner response failed: {err}"))
}

pub async fn fetch_execution_options(
    config: &AppConfig,
    owner_user_id: &str,
) -> Result<TaskRunnerExecutionOptions, String> {
    let base_url = task_runner_base_url(config)?;
    let owner_user_id = normalized_required("owner_user_id", owner_user_id)?;
    let mut endpoint = reqwest::Url::parse(base_url.as_str())
        .map_err(|err| format!("invalid task runner base url: {err}"))?;
    endpoint
        .path_segments_mut()
        .map_err(|_| "task runner base url cannot be used as internal endpoint".to_string())?
        .extend([
            "internal",
            "users",
            owner_user_id.as_str(),
            "execution-options",
        ]);
    let options = request_task_runner_internal_json::<TaskRunnerInternalExecutionOptions>(
        config,
        endpoint.as_str(),
    )
    .await?;
    let model_config_ids = options
        .model_config_ids
        .into_iter()
        .filter_map(|item| normalized_optional(Some(item)))
        .collect::<BTreeSet<_>>();
    let builtin_tool_ids = options
        .builtin_tool_ids
        .into_iter()
        .filter_map(|item| normalized_optional(Some(item)))
        .collect::<BTreeSet<_>>();
    let external_tool_ids = options
        .external_tool_ids
        .into_iter()
        .filter_map(|item| normalized_optional(Some(item)))
        .collect::<BTreeSet<_>>();
    let skill_ids = options
        .skill_ids
        .into_iter()
        .filter_map(|item| normalized_optional(Some(item)))
        .collect::<BTreeSet<_>>();
    Ok(TaskRunnerExecutionOptions {
        model_config_ids,
        builtin_tool_ids,
        external_tool_ids,
        skill_ids,
    })
}

fn task_runner_base_url(config: &AppConfig) -> Result<String, String> {
    config
        .task_runner_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_end_matches('/').to_string())
        .ok_or_else(|| "task runner base url is not configured".to_string())
}

async fn request_task_runner_internal_json<T>(
    config: &AppConfig,
    endpoint: &str,
) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let secret = config
        .task_runner_internal_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "task runner internal secret is not configured".to_string())?;
    let client = reqwest::Client::builder()
        .timeout(config.task_runner_request_timeout)
        .build()
        .map_err(|err| format!("build task runner client failed: {err}"))?;
    let response = client
        .get(endpoint)
        .header("X-Task-Runner-Internal-Secret", secret)
        .send()
        .await
        .map_err(|err| format!("task runner request failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        let body =
            read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
        return Err(if body.trim().is_empty() {
            format!("task runner request failed with status {status}")
        } else {
            body
        });
    }
    response
        .json::<T>()
        .await
        .map_err(|err| format!("parse task runner response failed: {err}"))
}

fn task_runner_mcp_config_from_value(value: Value) -> Result<TaskRunnerMcpConfig, String> {
    Ok(TaskRunnerMcpConfig {
        enabled_builtin_kinds: value
            .get("enabled_builtin_kinds")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        external_mcp_config_ids: value
            .get("external_mcp_config_ids")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        skill_ids: value
            .get("skill_ids")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
    })
}

pub fn normalize_tool_ids(values: Vec<String>) -> Result<Vec<String>, String> {
    let out = normalize_id_list(values);
    if out.is_empty() {
        return Err("task_runner_enabled_tool_ids is required".to_string());
    }
    Ok(out)
}

pub fn normalize_id_list(values: Vec<String>) -> Vec<String> {
    let mut out = values
        .into_iter()
        .filter_map(|value| normalized_optional(Some(value)))
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

pub fn normalized_required(field: &str, value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(value.to_string())
    }
}

fn default_task_objective(work_item: &ProjectWorkItemRecord) -> String {
    match work_item
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(description) => format!("完成项目工作项：{}\n\n{}", work_item.title, description),
        None => format!("完成项目工作项：{}", work_item.title),
    }
}

fn normalized_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProjectWorkItemStatus;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::{routing::get, routing::post, Json, Router};
    use serde_json::{json, Value};
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[derive(Debug, Default)]
    struct CapturedRequest {
        path: Option<String>,
        post_path: Option<String>,
        internal_secret: Option<String>,
        authorization: Option<String>,
        request_body: Option<Value>,
    }

    #[derive(Clone)]
    struct TestServerState {
        captured: Arc<Mutex<CapturedRequest>>,
        body: Value,
    }

    async fn start_test_server(
        captured: Arc<Mutex<CapturedRequest>>,
        body: Value,
    ) -> (String, tokio::task::JoinHandle<()>) {
        async fn handler(
            State(state): State<TestServerState>,
            uri: axum::http::Uri,
            headers: HeaderMap,
        ) -> (StatusCode, Json<Value>) {
            let mut captured = state.captured.lock().await;
            captured.path = Some(uri.path().to_string());
            captured.internal_secret = headers
                .get("x-task-runner-internal-secret")
                .and_then(|value| value.to_str().ok())
                .map(ToOwned::to_owned);
            (StatusCode::OK, Json(state.body.clone()))
        }

        let app = Router::new()
            .route(
                "/internal/users/{owner_user_id}/execution-options",
                get(handler),
            )
            .with_state(TestServerState { captured, body });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("read test server addr");
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{addr}"), handle)
    }

    async fn start_create_task_test_server(
        captured: Arc<Mutex<CapturedRequest>>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        async fn execution_options_handler(
            State(state): State<TestServerState>,
            uri: axum::http::Uri,
            headers: HeaderMap,
        ) -> (StatusCode, Json<Value>) {
            let mut captured = state.captured.lock().await;
            captured.path = Some(uri.path().to_string());
            captured.internal_secret = headers
                .get("x-task-runner-internal-secret")
                .and_then(|value| value.to_str().ok())
                .map(ToOwned::to_owned);
            (
                StatusCode::OK,
                Json(json!({
                    "model_config_ids": ["model-1"],
                    "builtin_tool_ids": ["builtin-code"],
                    "external_tool_ids": ["external-docs"],
                    "skill_ids": ["skill-plan"]
                })),
            )
        }

        async fn create_task_handler(
            State(state): State<TestServerState>,
            uri: axum::http::Uri,
            headers: HeaderMap,
            Json(body): Json<Value>,
        ) -> (StatusCode, Json<Value>) {
            let mut captured = state.captured.lock().await;
            captured.post_path = Some(uri.path().to_string());
            captured.authorization = headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .map(ToOwned::to_owned);
            captured.request_body = Some(body);
            (
                StatusCode::OK,
                Json(json!({
                    "id": "task-runner-task-1",
                    "title": "继续规划",
                    "status": "ready",
                    "project_id": "project-1",
                    "last_run_id": null,
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z"
                })),
            )
        }

        let state = TestServerState {
            captured,
            body: json!({}),
        };
        let app = Router::new()
            .route(
                "/internal/users/{owner_user_id}/execution-options",
                get(execution_options_handler),
            )
            .route("/api/tasks", post(create_task_handler))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("read test server addr");
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{addr}"), handle)
    }

    #[tokio::test]
    async fn fetch_execution_options_uses_owner_scoped_internal_endpoint() {
        let captured = Arc::new(Mutex::new(CapturedRequest::default()));
        let (base_url, handle) = start_test_server(
            captured.clone(),
            json!({
                "model_config_ids": ["model-1"],
                "builtin_tool_ids": ["CodeMaintainerRead", "builtin_code_maintainer_read"],
                "external_tool_ids": ["external-1"]
            }),
        )
        .await;

        let options = fetch_execution_options(
            &AppConfig {
                host: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: 0,
                database_url: "sqlite::memory:".to_string(),
                user_service_base_url: "http://127.0.0.1:39190".to_string(),
                user_service_request_timeout: std::time::Duration::from_millis(1_000),
                task_runner_base_url: Some(base_url),
                task_runner_request_timeout: std::time::Duration::from_millis(1_000),
                task_runner_internal_secret: Some("internal-secret".to_string()),
                sync_secret: None,
            },
            "owner-1",
        )
        .await
        .expect("fetch execution options");

        assert_eq!(
            options
                .validate_model_config_id("model-1")
                .expect("model id"),
            "model-1"
        );
        assert!(options
            .mcp_config_for_tool_ids(&["CodeMaintainerRead".to_string(), "external-1".to_string()])
            .is_ok());
        let captured = captured.lock().await;
        assert_eq!(
            captured.path.as_deref(),
            Some("/internal/users/owner-1/execution-options")
        );
        assert_eq!(captured.internal_secret.as_deref(), Some("internal-secret"));

        handle.abort();
    }

    #[tokio::test]
    async fn fetch_execution_options_encodes_owner_id_path_segment() {
        let captured = Arc::new(Mutex::new(CapturedRequest::default()));
        let (base_url, handle) = start_test_server(
            captured.clone(),
            json!({
                "model_config_ids": ["model-1"],
                "builtin_tool_ids": [],
                "external_tool_ids": []
            }),
        )
        .await;

        fetch_execution_options(
            &AppConfig {
                host: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: 0,
                database_url: "sqlite::memory:".to_string(),
                user_service_base_url: "http://127.0.0.1:39190".to_string(),
                user_service_request_timeout: std::time::Duration::from_millis(1_000),
                task_runner_base_url: Some(base_url),
                task_runner_request_timeout: std::time::Duration::from_millis(1_000),
                task_runner_internal_secret: Some("internal-secret".to_string()),
                sync_secret: None,
            },
            "owner/one",
        )
        .await
        .expect("fetch execution options");

        let captured = captured.lock().await;
        assert_eq!(
            captured.path.as_deref(),
            Some("/internal/users/owner%2Fone/execution-options")
        );

        handle.abort();
    }

    #[tokio::test]
    async fn create_task_from_planning_work_item_uses_plan_profile() {
        let captured = Arc::new(Mutex::new(CapturedRequest::default()));
        let (base_url, handle) = start_create_task_test_server(captured.clone()).await;

        let task = create_task_from_work_item(
            &AppConfig {
                host: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: 0,
                database_url: "sqlite::memory:".to_string(),
                user_service_base_url: "http://127.0.0.1:39190".to_string(),
                user_service_request_timeout: std::time::Duration::from_millis(1_000),
                task_runner_base_url: Some(base_url),
                task_runner_request_timeout: std::time::Duration::from_millis(1_000),
                task_runner_internal_secret: Some("internal-secret".to_string()),
                sync_secret: None,
            },
            "runner-token",
            &ProjectWorkItemRecord {
                id: "work-item-1".to_string(),
                project_id: "project-1".to_string(),
                requirement_id: "req-1".to_string(),
                title: "继续规划".to_string(),
                description: Some("继续拆解后续工作".to_string()),
                task_runner_default_model_config_id: "model-1".to_string(),
                task_runner_enabled_tool_ids: vec!["builtin-code".to_string()],
                task_runner_skill_ids: vec!["skill-plan".to_string()],
                status: ProjectWorkItemStatus::Todo,
                priority: 5,
                assignee_user_id: None,
                estimate_points: None,
                due_at: None,
                sort_order: 0,
                tags: vec!["planning".to_string()],
                is_planning_task: true,
                creator_user_id: Some("owner-1".to_string()),
                creator_username: None,
                creator_display_name: None,
                owner_user_id: Some("owner-1".to_string()),
                owner_username: None,
                owner_display_name: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
                archived_at: None,
            },
            CreateTaskRunnerTaskFromWorkItemRequest::default(),
        )
        .await
        .expect("create task");

        assert_eq!(task.id, "task-runner-task-1");
        let captured = captured.lock().await;
        assert_eq!(captured.post_path.as_deref(), Some("/api/tasks"));
        assert_eq!(
            captured.authorization.as_deref(),
            Some("Bearer runner-token")
        );
        let body = captured.request_body.as_ref().expect("request body");
        assert_eq!(
            body.get("task_profile").and_then(Value::as_str),
            Some("chatos_plan")
        );
        assert_eq!(
            body.pointer("/input_payload/is_planning_task")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            body.pointer("/mcp_config/skill_ids")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            vec![json!("skill-plan")]
        );

        handle.abort();
    }
}

fn normalize_tags(values: Vec<String>) -> Vec<String> {
    let mut tags = values
        .into_iter()
        .filter_map(|value| normalized_optional(Some(value)))
        .collect::<Vec<_>>();
    tags.sort();
    tags.dedup();
    tags
}
