use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::OnceLock;

static TASK_RUNNER_HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct UserServiceTaskRunnerExchange {
    pub base_url: String,
    pub access_token: String,
    pub task_runner_agent_account_id: String,
    pub contact_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserServiceTaskRunnerTokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct TaskRunnerSkillResponse {
    content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TaskRunnerTaskRecord {
    pub id: String,
    pub status: String,
    pub last_run_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TaskRunnerMcpConfigRequest {
    pub enabled_builtin_kinds: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builtin_prompt_locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_dir: Option<String>,
    pub external_mcp_config_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CreateTaskRunnerTaskRequest {
    pub title: String,
    pub description: Option<String>,
    pub objective: String,
    pub input_payload: Option<Value>,
    pub status: Option<String>,
    pub priority: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub default_model_config_id: Option<String>,
    pub project_id: Option<String>,
    pub task_profile: Option<String>,
    pub schedule: Option<TaskRunnerTaskScheduleRequest>,
    pub mcp_config: Option<TaskRunnerMcpConfigRequest>,
    pub prerequisite_task_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TaskRunnerTaskScheduleRequest {
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CancelTaskRunnerTaskRequest {
    pub reason: String,
    pub replacement_task_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TaskRunnerMcpCatalogEntry {
    kind: String,
    config_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TaskRunnerExternalMcpConfig {
    id: String,
    enabled: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TaskRunnerExecutionOptions {
    builtin_tool_ids: BTreeSet<String>,
    external_tool_ids: BTreeSet<String>,
}

impl TaskRunnerExecutionOptions {
    pub fn mcp_config_for_tool_ids(
        &self,
        values: &[String],
    ) -> Result<TaskRunnerMcpConfigRequest, String> {
        let values = normalize_tool_ids(values);
        if values.is_empty() {
            return Err("task_runner_enabled_tool_ids is required".to_string());
        }
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
        Ok(TaskRunnerMcpConfigRequest {
            enabled_builtin_kinds,
            builtin_prompt_locale: None,
            workspace_dir: None,
            external_mcp_config_ids,
        })
    }
}

pub async fn exchange_task_runner_token_via_user_service(
    request: &UserServiceTaskRunnerExchange,
) -> Result<String, String> {
    let endpoint = format!(
        "{}/api/token/exchange/task-runner",
        request.base_url.trim().trim_end_matches('/')
    );
    let response = task_runner_http_client()
        .post(endpoint)
        .bearer_auth(request.access_token.trim())
        .json(&serde_json::json!({
            "task_runner_agent_account_id": request.task_runner_agent_account_id,
            "contact_id": request.contact_id,
        }))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "User service task runner token exchange failed: {status} {body}"
        ));
    }
    let payload = response
        .json::<UserServiceTaskRunnerTokenResponse>()
        .await
        .map_err(|err| err.to_string())?;
    let token = payload.access_token.trim();
    if token.is_empty() {
        return Err("User service task runner token exchange returned empty token".to_string());
    }
    Ok(token.to_string())
}

pub async fn fetch_task_runner_skill(
    base_url: &str,
    lang: &str,
    profile: Option<&str>,
) -> Result<String, String> {
    let normalized_lang = match lang.trim() {
        "en" | "en-US" | "english" => "en-US",
        _ => "zh-CN",
    };
    let endpoint = format!(
        "{}/api/skills/task-runner",
        base_url.trim().trim_end_matches('/')
    );
    let mut request = task_runner_http_client()
        .get(endpoint)
        .query(&[("lang", normalized_lang)]);
    if let Some(profile) = profile.map(str::trim).filter(|value| !value.is_empty()) {
        request = request.query(&[("profile", profile)]);
    }
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Task Runner skill request failed: {status} {body}"));
    }
    let payload = response
        .json::<TaskRunnerSkillResponse>()
        .await
        .map_err(|err| err.to_string())?;
    let content = payload.content.trim();
    if content.is_empty() {
        return Err("Task Runner skill request returned empty content".to_string());
    }
    Ok(content.to_string())
}

pub async fn fetch_task_runner_execution_options(
    base_url: &str,
    access_token: &str,
) -> Result<TaskRunnerExecutionOptions, String> {
    let catalog: Vec<TaskRunnerMcpCatalogEntry> = task_runner_json(
        base_url,
        access_token,
        reqwest::Method::GET,
        "/api/mcp/tools",
        None::<&()>,
    )
    .await?;
    let external_configs: Vec<TaskRunnerExternalMcpConfig> = task_runner_json(
        base_url,
        access_token,
        reqwest::Method::GET,
        "/api/external-mcp-configs",
        None::<&()>,
    )
    .await
    .unwrap_or_default();

    let mut builtin_tool_ids = BTreeSet::new();
    for item in catalog {
        if let Some(kind) = normalize_optional(Some(item.kind)) {
            builtin_tool_ids.insert(kind);
        }
        if let Some(config_id) = item
            .config_id
            .and_then(|value| normalize_optional(Some(value)))
        {
            builtin_tool_ids.insert(config_id);
        }
    }
    let external_tool_ids = external_configs
        .into_iter()
        .filter(|item| item.enabled)
        .filter_map(|item| normalize_optional(Some(item.id)))
        .collect::<BTreeSet<_>>();

    Ok(TaskRunnerExecutionOptions {
        builtin_tool_ids,
        external_tool_ids,
    })
}

pub async fn create_task_runner_task(
    base_url: &str,
    access_token: &str,
    user_access_token: Option<&str>,
    source_session_id: Option<&str>,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
    request: &CreateTaskRunnerTaskRequest,
) -> Result<TaskRunnerTaskRecord, String> {
    let mut builder =
        task_runner_request(base_url, access_token, reqwest::Method::POST, "/api/tasks")
            .json(request);
    if let Some(value) = normalize_optional(source_session_id.map(ToOwned::to_owned)) {
        builder = builder.header("X-Chatos-Session-Id", value);
    }
    if let Some(value) = normalize_optional(source_user_message_id.map(ToOwned::to_owned)) {
        builder = builder.header("X-Chatos-User-Message-Id", value);
    }
    if let Some(value) = normalize_optional(source_turn_id.map(ToOwned::to_owned)) {
        builder = builder.header("X-Chatos-Turn-Id", value);
    }
    if let Some(value) = normalize_optional(user_access_token.map(ToOwned::to_owned)) {
        builder = builder.header("X-Chatos-User-Authorization", format!("Bearer {value}"));
    }
    send_task_runner_response(builder).await
}

pub async fn get_task_runner_task(
    base_url: &str,
    access_token: &str,
    task_id: &str,
) -> Result<TaskRunnerTaskRecord, String> {
    let path = format!("/api/tasks/{}", urlencoding::encode(task_id.trim()));
    task_runner_json(
        base_url,
        access_token,
        reqwest::Method::GET,
        path.as_str(),
        None::<&()>,
    )
    .await
}

pub async fn cancel_task_runner_task(
    base_url: &str,
    access_token: &str,
    user_access_token: Option<&str>,
    task_id: &str,
    request: &CancelTaskRunnerTaskRequest,
) -> Result<Value, String> {
    let path = format!("/api/tasks/{}/cancel", urlencoding::encode(task_id.trim()));
    let mut builder =
        task_runner_request(base_url, access_token, reqwest::Method::POST, path.as_str())
            .json(request);
    if let Some(value) = normalize_optional(user_access_token.map(ToOwned::to_owned)) {
        builder = builder.header("X-Chatos-User-Authorization", format!("Bearer {value}"));
    }
    send_task_runner_response(builder).await
}

#[derive(Debug, Default, Serialize)]
pub struct SubmitTaskRunnerPromptRequest {
    pub values: Option<Value>,
    pub selection: Option<Value>,
    pub reason: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct CancelTaskRunnerPromptRequest {
    pub reason: Option<String>,
}

pub async fn submit_task_runner_prompt(
    base_url: &str,
    access_token: &str,
    prompt_id: &str,
    request: &SubmitTaskRunnerPromptRequest,
) -> Result<Value, String> {
    let endpoint = format!(
        "{}/api/prompts/{}/submit",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(prompt_id.trim())
    );
    send_json(
        task_runner_http_client()
            .post(endpoint)
            .bearer_auth(access_token.trim())
            .json(request),
    )
    .await
}

pub async fn cancel_task_runner_prompt(
    base_url: &str,
    access_token: &str,
    prompt_id: &str,
    request: &CancelTaskRunnerPromptRequest,
) -> Result<Value, String> {
    let endpoint = format!(
        "{}/api/prompts/{}/cancel",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(prompt_id.trim())
    );
    send_json(
        task_runner_http_client()
            .post(endpoint)
            .bearer_auth(access_token.trim())
            .json(request),
    )
    .await
}

async fn send_json<T: for<'de> Deserialize<'de>>(
    request: reqwest::RequestBuilder,
) -> Result<T, String> {
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Task Runner request failed: {status} {body}"));
    }
    response.json::<T>().await.map_err(|err| err.to_string())
}

async fn task_runner_json<T, B>(
    base_url: &str,
    access_token: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<&B>,
) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
    B: Serialize + ?Sized,
{
    let mut request = task_runner_request(base_url, access_token, method, path);
    if let Some(body) = body {
        request = request.json(body);
    }
    send_task_runner_response(request).await
}

fn task_runner_request(
    base_url: &str,
    access_token: &str,
    method: reqwest::Method,
    path: &str,
) -> reqwest::RequestBuilder {
    let endpoint = format!("{}{}", base_url.trim().trim_end_matches('/'), path);
    task_runner_http_client()
        .request(method, endpoint)
        .bearer_auth(access_token.trim())
}

async fn send_task_runner_response<T: for<'de> Deserialize<'de>>(
    request: reqwest::RequestBuilder,
) -> Result<T, String> {
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Task Runner request failed: {status} {body}"));
    }
    response.json::<T>().await.map_err(|err| err.to_string())
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn task_runner_http_client() -> &'static reqwest::Client {
    TASK_RUNNER_HTTP_CLIENT.get_or_init(reqwest::Client::new)
}

fn normalize_tool_ids(values: &[String]) -> Vec<String> {
    let mut out = values
        .iter()
        .filter_map(|value| normalize_optional(Some(value.clone())))
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

async fn get_internal_json(
    base_url: &str,
    path: &str,
    query: &[(&str, &str)],
) -> Result<Value, String> {
    let endpoint = format!("{}{}", base_url.trim().trim_end_matches('/'), path);
    let response = task_runner_http_client()
        .get(endpoint)
        .query(query)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Task Runner internal request failed: {status} {body}"
        ));
    }
    response
        .json::<Value>()
        .await
        .map_err(|err| err.to_string())
}

async fn post_internal_json<T: Serialize + ?Sized>(
    base_url: &str,
    path: &str,
    body: &T,
) -> Result<Value, String> {
    let endpoint = format!("{}{}", base_url.trim().trim_end_matches('/'), path);
    let response = task_runner_http_client()
        .post(endpoint)
        .json(body)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Task Runner internal request failed: {status} {body}"
        ));
    }
    response
        .json::<Value>()
        .await
        .map_err(|err| err.to_string())
}

#[derive(Debug, Serialize)]
struct SessionActiveMessageTasksRequest<'a> {
    source_session_id: &'a str,
    source_user_message_ids: &'a [String],
    source_turn_ids: &'a [String],
}

pub async fn list_session_active_message_tasks(
    base_url: &str,
    source_session_id: &str,
    source_user_message_ids: &[String],
    source_turn_ids: &[String],
) -> Result<Value, String> {
    post_internal_json(
        base_url,
        "/internal/chatos/session-active-message-tasks",
        &SessionActiveMessageTasksRequest {
            source_session_id,
            source_user_message_ids,
            source_turn_ids,
        },
    )
    .await
}

pub async fn list_message_tasks(
    base_url: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
) -> Result<Value, String> {
    let mut query = vec![("source_session_id", source_session_id)];
    if let Some(source_user_message_id) = source_user_message_id {
        query.push(("source_user_message_id", source_user_message_id));
    }
    if let Some(source_turn_id) = source_turn_id {
        query.push(("source_turn_id", source_turn_id));
    }
    get_internal_json(base_url, "/internal/chatos/message-tasks", query.as_slice()).await
}

pub async fn get_message_task_graph(
    base_url: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
) -> Result<Value, String> {
    let mut query = vec![("source_session_id", source_session_id)];
    if let Some(source_user_message_id) = source_user_message_id {
        query.push(("source_user_message_id", source_user_message_id));
    }
    if let Some(source_turn_id) = source_turn_id {
        query.push(("source_turn_id", source_turn_id));
    }
    get_internal_json(base_url, "/internal/chatos/message-graph", query.as_slice()).await
}

pub async fn get_message_task(
    base_url: &str,
    task_id: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
) -> Result<Value, String> {
    let path = format!(
        "/internal/chatos/message-tasks/{}",
        urlencoding::encode(task_id.trim())
    );
    let mut query = vec![("source_session_id", source_session_id)];
    if let Some(source_user_message_id) = source_user_message_id {
        query.push(("source_user_message_id", source_user_message_id));
    }
    if let Some(source_turn_id) = source_turn_id {
        query.push(("source_turn_id", source_turn_id));
    }
    get_internal_json(base_url, path.as_str(), query.as_slice()).await
}

pub async fn get_message_run(
    base_url: &str,
    run_id: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
) -> Result<Value, String> {
    let path = format!(
        "/internal/chatos/message-runs/{}",
        urlencoding::encode(run_id.trim())
    );
    let mut query = vec![("source_session_id", source_session_id)];
    if let Some(source_user_message_id) = source_user_message_id {
        query.push(("source_user_message_id", source_user_message_id));
    }
    if let Some(source_turn_id) = source_turn_id {
        query.push(("source_turn_id", source_turn_id));
    }
    get_internal_json(base_url, path.as_str(), query.as_slice()).await
}

pub async fn get_message_graph_run(
    base_url: &str,
    run_id: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
) -> Result<Value, String> {
    let path = format!(
        "/internal/chatos/message-graph/runs/{}",
        urlencoding::encode(run_id.trim())
    );
    let mut query = vec![("source_session_id", source_session_id)];
    if let Some(source_user_message_id) = source_user_message_id {
        query.push(("source_user_message_id", source_user_message_id));
    }
    if let Some(source_turn_id) = source_turn_id {
        query.push(("source_turn_id", source_turn_id));
    }
    get_internal_json(base_url, path.as_str(), query.as_slice()).await
}

#[cfg(test)]
mod tests {
    use super::{
        exchange_task_runner_token_via_user_service, fetch_task_runner_skill,
        UserServiceTaskRunnerExchange,
    };
    use axum::extract::State;
    use axum::http::{header::AUTHORIZATION, HeaderMap, StatusCode};
    use axum::{routing::get, routing::post, Json, Router};
    use serde_json::{json, Value};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[derive(Debug, Default)]
    struct CapturedExchange {
        authorization: Option<String>,
        body: Option<Value>,
    }

    #[derive(Debug, Default)]
    struct CapturedSkillRequest {
        lang: Option<String>,
        profile: Option<String>,
    }

    #[derive(Clone)]
    struct ExchangeServerState {
        captured: Arc<Mutex<CapturedExchange>>,
        response_status: StatusCode,
        response_body: Value,
    }

    #[derive(Clone)]
    struct SkillServerState {
        captured: Arc<Mutex<CapturedSkillRequest>>,
        response_status: StatusCode,
        response_body: Value,
    }

    async fn start_test_server(
        captured: Arc<Mutex<CapturedExchange>>,
        status: StatusCode,
        body: Value,
    ) -> (String, tokio::task::JoinHandle<()>) {
        async fn handler(
            State(state): State<ExchangeServerState>,
            headers: HeaderMap,
            Json(payload): Json<Value>,
        ) -> (StatusCode, Json<Value>) {
            let mut captured = state.captured.lock().await;
            captured.authorization = headers
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .map(ToOwned::to_owned);
            captured.body = Some(payload);
            (state.response_status, Json(state.response_body))
        }

        let app = Router::new()
            .route("/api/token/exchange/task-runner", post(handler))
            .with_state(ExchangeServerState {
                captured,
                response_status: status,
                response_body: body,
            });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("read test server addr");
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{addr}"), handle)
    }

    async fn start_skill_test_server(
        captured: Arc<Mutex<CapturedSkillRequest>>,
        status: StatusCode,
        body: Value,
    ) -> (String, tokio::task::JoinHandle<()>) {
        async fn handler(
            State(state): State<SkillServerState>,
            query: axum::extract::Query<std::collections::HashMap<String, String>>,
        ) -> (StatusCode, Json<Value>) {
            let mut captured = state.captured.lock().await;
            captured.lang = query.get("lang").cloned();
            captured.profile = query.get("profile").cloned();
            (state.response_status, Json(state.response_body))
        }

        let app = Router::new()
            .route("/api/skills/task-runner", get(handler))
            .with_state(SkillServerState {
                captured,
                response_status: status,
                response_body: body,
            });
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
    async fn exchange_task_runner_token_via_user_service_sends_bearer_and_body() {
        let captured = Arc::new(Mutex::new(CapturedExchange::default()));
        let (base_url, handle) = start_test_server(
            captured.clone(),
            StatusCode::OK,
            json!({ "access_token": "task-runner-token" }),
        )
        .await;

        let token = exchange_task_runner_token_via_user_service(&UserServiceTaskRunnerExchange {
            base_url,
            access_token: "human-user-token".to_string(),
            task_runner_agent_account_id: "agent-123".to_string(),
            contact_id: Some("contact-456".to_string()),
        })
        .await
        .expect("exchange response");

        assert_eq!(token, "task-runner-token");
        let captured = captured.lock().await;
        assert_eq!(
            captured.authorization.as_deref(),
            Some("Bearer human-user-token")
        );
        assert_eq!(
            captured
                .body
                .as_ref()
                .and_then(|value| value.get("task_runner_agent_account_id"))
                .and_then(Value::as_str),
            Some("agent-123")
        );
        assert_eq!(
            captured
                .body
                .as_ref()
                .and_then(|value| value.get("contact_id"))
                .and_then(Value::as_str),
            Some("contact-456")
        );

        handle.abort();
    }

    #[tokio::test]
    async fn exchange_task_runner_token_via_user_service_surfaces_remote_error() {
        let captured = Arc::new(Mutex::new(CapturedExchange::default()));
        let (base_url, handle) = start_test_server(
            captured,
            StatusCode::FORBIDDEN,
            json!({ "error": "owner mismatch" }),
        )
        .await;

        let error = exchange_task_runner_token_via_user_service(&UserServiceTaskRunnerExchange {
            base_url,
            access_token: "human-user-token".to_string(),
            task_runner_agent_account_id: "agent-123".to_string(),
            contact_id: None,
        })
        .await
        .expect_err("expected remote error");

        assert!(error.contains("403"));
        assert!(error.contains("owner mismatch"));

        handle.abort();
    }

    #[tokio::test]
    async fn fetch_task_runner_skill_includes_profile_query() {
        let captured = Arc::new(Mutex::new(CapturedSkillRequest::default()));
        let (base_url, handle) = start_skill_test_server(
            captured.clone(),
            StatusCode::OK,
            json!({ "content": "plan skill" }),
        )
        .await;

        let content = fetch_task_runner_skill(&base_url, "zh-CN", Some("chatos_plan"))
            .await
            .expect("fetch skill");

        assert_eq!(content, "plan skill");
        let captured = captured.lock().await;
        assert_eq!(captured.lang.as_deref(), Some("zh-CN"));
        assert_eq!(captured.profile.as_deref(), Some("chatos_plan"));

        handle.abort();
    }

    #[tokio::test]
    async fn fetch_task_runner_skill_normalizes_english_locale_without_profile() {
        let captured = Arc::new(Mutex::new(CapturedSkillRequest::default()));
        let (base_url, handle) = start_skill_test_server(
            captured.clone(),
            StatusCode::OK,
            json!({ "content": "default skill" }),
        )
        .await;

        let content = fetch_task_runner_skill(&base_url, "english", None)
            .await
            .expect("fetch skill");

        assert_eq!(content, "default skill");
        let captured = captured.lock().await;
        assert_eq!(captured.lang.as_deref(), Some("en-US"));
        assert!(captured.profile.is_none());

        handle.abort();
    }
}
