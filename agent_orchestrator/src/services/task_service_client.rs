use std::time::Duration;

use once_cell::sync::Lazy;
use reqwest::RequestBuilder;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::services::memory_server_client::current_access_token;

static TASK_SERVICE_HTTP: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

fn client() -> &'static reqwest::Client {
    &TASK_SERVICE_HTTP
}

fn build_url(path: &str) -> String {
    format!(
        "{}{}",
        Config::get().task_service_base_url.trim_end_matches('/'),
        path
    )
}

fn build_internal_url(path: &str) -> String {
    build_url(&format!("/internal{}", path))
}

fn timeout_duration() -> Duration {
    Duration::from_millis(Config::get().task_service_request_timeout_ms.max(300) as u64)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskContextAssetRefDto {
    pub asset_type: String,
    pub asset_id: String,
    pub display_name: Option<String>,
    pub source_type: Option<String>,
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskExecutionResultContractDto {
    pub result_required: bool,
    pub preferred_format: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskPlanningSnapshotDto {
    #[serde(default)]
    pub contact_authorized_builtin_mcp_ids: Vec<String>,
    pub selected_model_config_id: Option<String>,
    pub source_user_goal_summary: Option<String>,
    pub source_constraints_summary: Option<String>,
    pub planned_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskHandoffPayloadDto {
    pub task_id: String,
    pub task_plan_id: Option<String>,
    pub handoff_kind: String,
    pub summary: String,
    pub result_summary: Option<String>,
    #[serde(default)]
    pub key_changes: Vec<String>,
    #[serde(default)]
    pub changed_files: Vec<String>,
    #[serde(default)]
    pub executed_commands: Vec<String>,
    #[serde(default)]
    pub verification_suggestions: Vec<String>,
    #[serde(default)]
    pub open_risks: Vec<String>,
    #[serde(default)]
    pub artifact_refs: Vec<String>,
    #[serde(default)]
    pub checkpoint_message_ids: Vec<String>,
    pub result_brief_id: Option<String>,
    pub generated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskResultBriefDto {
    pub id: String,
    pub task_id: String,
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub source_session_id: Option<String>,
    pub source_turn_id: Option<String>,
    pub task_title: String,
    pub task_status: String,
    pub result_summary: String,
    pub result_format: Option<String>,
    pub result_message_id: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TaskServiceAuth {
    ServiceToken(String),
    Bearer(String),
    None,
}

fn pick_auth(access_token: Option<String>, service_token: &str) -> TaskServiceAuth {
    let trimmed_service_token = service_token.trim();
    if !trimmed_service_token.is_empty() {
        return TaskServiceAuth::ServiceToken(trimmed_service_token.to_string());
    }

    match access_token {
        Some(token) => {
            let trimmed_access_token = token.trim();
            if trimmed_access_token.is_empty() {
                TaskServiceAuth::None
            } else {
                TaskServiceAuth::Bearer(trimmed_access_token.to_string())
            }
        }
        None => TaskServiceAuth::None,
    }
}

fn apply_auth(req: RequestBuilder) -> RequestBuilder {
    match pick_auth(
        current_access_token(),
        Config::get().task_service_service_token.as_str(),
    ) {
        TaskServiceAuth::ServiceToken(token) => req.header("X-Service-Token", token),
        TaskServiceAuth::Bearer(token) => req.bearer_auth(token),
        TaskServiceAuth::None => req,
    }
}

async fn send_json<T: for<'de> Deserialize<'de>>(req: RequestBuilder) -> Result<T, String> {
    let resp = apply_auth(req).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("status={} detail={}", status, detail));
    }
    resp.json::<T>().await.map_err(|e| e.to_string())
}

async fn send_optional_json<T: for<'de> Deserialize<'de>>(
    req: RequestBuilder,
) -> Result<Option<T>, String> {
    let resp = apply_auth(req).send().await.map_err(|e| e.to_string())?;
    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("status={} detail={}", status, detail));
    }
    resp.json::<T>().await.map(Some).map_err(|e| e.to_string())
}

async fn send_delete_result(req: RequestBuilder) -> Result<bool, String> {
    let resp = apply_auth(req).send().await.map_err(|e| e.to_string())?;
    if resp.status().as_u16() == 404 {
        return Ok(false);
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("status={} detail={}", status, detail));
    }
    Ok(true)
}

#[derive(Debug, Clone, Deserialize)]
struct ListResponse<T> {
    items: Vec<T>,
}

#[derive(Debug, Clone, Deserialize)]
struct ItemResponse<T> {
    item: Option<T>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskExecutionScopeDto {
    pub scope_key: String,
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub latest_session_id: Option<String>,
    pub latest_task_id: Option<String>,
    pub latest_task_updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateTaskRequestDto {
    pub user_id: Option<String>,
    pub contact_agent_id: String,
    pub project_id: String,
    pub task_plan_id: Option<String>,
    pub task_ref: Option<String>,
    pub task_kind: Option<String>,
    #[serde(default)]
    pub depends_on_task_ids: Vec<String>,
    #[serde(default)]
    pub verification_of_task_ids: Vec<String>,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    pub project_root: Option<String>,
    pub remote_connection_id: Option<String>,
    pub session_id: Option<String>,
    pub conversation_turn_id: Option<String>,
    pub source_message_id: Option<String>,
    pub model_config_id: Option<String>,
    pub title: String,
    pub content: String,
    pub priority: Option<String>,
    pub confirm_note: Option<String>,
    pub execution_note: Option<String>,
    #[serde(default)]
    pub planned_builtin_mcp_ids: Vec<String>,
    #[serde(default)]
    pub planned_context_assets: Vec<TaskContextAssetRefDto>,
    pub execution_result_contract: Option<TaskExecutionResultContractDto>,
    pub planning_snapshot: Option<TaskPlanningSnapshotDto>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct UpdateTaskRequestDto {
    pub title: Option<String>,
    pub content: Option<String>,
    pub priority: Option<String>,
    pub status: Option<String>,
    pub task_ref: Option<Option<String>>,
    pub task_kind: Option<Option<String>>,
    pub depends_on_task_ids: Option<Vec<String>>,
    pub verification_of_task_ids: Option<Vec<String>>,
    pub acceptance_criteria: Option<Vec<String>>,
    pub blocked_reason: Option<Option<String>>,
    pub confirm_note: Option<String>,
    pub execution_note: Option<String>,
    pub project_root: Option<Option<String>>,
    pub remote_connection_id: Option<Option<String>>,
    pub planned_builtin_mcp_ids: Option<Vec<String>>,
    pub planned_context_assets: Option<Vec<TaskContextAssetRefDto>>,
    pub execution_result_contract: Option<TaskExecutionResultContractDto>,
    pub planning_snapshot: Option<TaskPlanningSnapshotDto>,
    pub handoff_payload: Option<Option<TaskHandoffPayloadDto>>,
    pub model_config_id: Option<Option<String>>,
    pub queue_position: Option<i64>,
    pub pause_reason: Option<Option<String>>,
    pub last_checkpoint_summary: Option<Option<String>>,
    pub last_checkpoint_message_id: Option<Option<String>>,
    pub resume_note: Option<Option<String>>,
    pub result_summary: Option<Option<String>>,
    pub result_message_id: Option<Option<String>>,
    pub last_error: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfirmTaskRequestDto {
    pub user_id: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PauseTaskRequestDto {
    pub user_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StopTaskRequestDto {
    pub user_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResumeTaskRequestDto {
    pub user_id: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AckPauseTaskRequestDto {
    pub checkpoint_summary: Option<String>,
    pub checkpoint_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AckStopTaskRequestDto {
    pub result_summary: Option<String>,
    pub result_message_id: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchedulerRequestDto {
    pub user_id: Option<String>,
    pub contact_agent_id: String,
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AckAllDoneRequestDto {
    pub user_id: Option<String>,
    pub contact_agent_id: String,
    pub project_id: String,
    pub ack_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskRecordDto {
    pub id: String,
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub scope_key: String,
    pub task_plan_id: Option<String>,
    pub task_ref: Option<String>,
    pub task_kind: Option<String>,
    #[serde(default)]
    pub depends_on_task_ids: Vec<String>,
    #[serde(default)]
    pub verification_of_task_ids: Vec<String>,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    pub blocked_reason: Option<String>,
    pub project_root: Option<String>,
    pub remote_connection_id: Option<String>,
    pub session_id: Option<String>,
    pub conversation_turn_id: Option<String>,
    pub source_message_id: Option<String>,
    pub model_config_id: Option<String>,
    pub title: String,
    pub content: String,
    pub priority: String,
    pub status: String,
    pub confirm_note: Option<String>,
    pub execution_note: Option<String>,
    #[serde(default)]
    pub planned_builtin_mcp_ids: Vec<String>,
    #[serde(default)]
    pub planned_context_assets: Vec<TaskContextAssetRefDto>,
    pub execution_result_contract: Option<TaskExecutionResultContractDto>,
    pub planning_snapshot: Option<TaskPlanningSnapshotDto>,
    pub handoff_payload: Option<TaskHandoffPayloadDto>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub confirmed_at: Option<String>,
    pub started_at: Option<String>,
    pub paused_at: Option<String>,
    pub pause_reason: Option<String>,
    pub last_checkpoint_summary: Option<String>,
    pub last_checkpoint_message_id: Option<String>,
    pub resume_note: Option<String>,
    pub finished_at: Option<String>,
    pub last_error: Option<String>,
    pub result_summary: Option<String>,
    pub result_message_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchedulerDecisionDto {
    pub decision: String,
    pub task: Option<TaskRecordDto>,
}

pub async fn create_task(req_body: &CreateTaskRequestDto) -> Result<TaskRecordDto, String> {
    let path = if current_access_token().is_some() {
        build_url("/tasks")
    } else {
        build_internal_url("/tasks")
    };
    let req = client()
        .post(path.as_str())
        .timeout(timeout_duration())
        .json(req_body);
    send_json(req).await
}

pub async fn list_tasks(
    user_id: Option<&str>,
    contact_agent_id: Option<&str>,
    project_id: Option<&str>,
    session_id: Option<&str>,
    conversation_turn_id: Option<&str>,
    status: Option<&str>,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<TaskRecordDto>, String> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(v) = user_id {
        params.push(("user_id".to_string(), v.to_string()));
    }
    if let Some(v) = contact_agent_id {
        params.push(("contact_agent_id".to_string(), v.to_string()));
    }
    if let Some(v) = project_id {
        params.push(("project_id".to_string(), v.to_string()));
    }
    if let Some(v) = session_id {
        params.push(("session_id".to_string(), v.to_string()));
    }
    if let Some(v) = conversation_turn_id {
        params.push(("conversation_turn_id".to_string(), v.to_string()));
    }
    if let Some(v) = status {
        params.push(("status".to_string(), v.to_string()));
    }
    if let Some(v) = limit {
        params.push(("limit".to_string(), v.max(1).to_string()));
    }
    if offset > 0 {
        params.push(("offset".to_string(), offset.to_string()));
    }
    let path = if current_access_token().is_some() {
        build_url("/tasks")
    } else {
        build_internal_url("/tasks")
    };
    let req = client()
        .get(path.as_str())
        .timeout(timeout_duration())
        .query(&params);
    let resp: ListResponse<TaskRecordDto> = send_json(req).await?;
    Ok(resp.items)
}

pub async fn get_task(task_id: &str) -> Result<Option<TaskRecordDto>, String> {
    let path = if current_access_token().is_some() {
        build_url(&format!("/tasks/{}", urlencoding::encode(task_id)))
    } else {
        build_internal_url(&format!("/tasks/{}", urlencoding::encode(task_id)))
    };
    let req = client().get(path.as_str()).timeout(timeout_duration());
    send_optional_json(req).await
}

pub async fn get_task_result_brief(task_id: &str) -> Result<Option<TaskResultBriefDto>, String> {
    let path = build_internal_url(&format!(
        "/tasks/{}/result-brief",
        urlencoding::encode(task_id)
    ));
    let req = client().get(path.as_str()).timeout(timeout_duration());
    let resp: ItemResponse<TaskResultBriefDto> = send_json(req).await?;
    Ok(resp.item)
}

pub async fn update_task(
    task_id: &str,
    req_body: &UpdateTaskRequestDto,
) -> Result<Option<TaskRecordDto>, String> {
    let req = client()
        .patch(build_url(&format!("/tasks/{}", urlencoding::encode(task_id))).as_str())
        .timeout(timeout_duration())
        .json(req_body);
    send_optional_json(req).await
}

pub async fn update_task_internal(
    task_id: &str,
    req_body: &UpdateTaskRequestDto,
) -> Result<Option<TaskRecordDto>, String> {
    let req = client()
        .patch(build_internal_url(&format!("/tasks/{}", urlencoding::encode(task_id))).as_str())
        .timeout(timeout_duration())
        .json(req_body);
    send_optional_json(req).await
}

pub async fn delete_task(task_id: &str) -> Result<bool, String> {
    let req = client()
        .delete(build_url(&format!("/tasks/{}", urlencoding::encode(task_id))).as_str())
        .timeout(timeout_duration());
    send_delete_result(req).await
}

pub async fn confirm_task(
    task_id: &str,
    req_body: &ConfirmTaskRequestDto,
) -> Result<Option<TaskRecordDto>, String> {
    let req = client()
        .post(
            build_internal_url(&format!("/tasks/{}/confirm", urlencoding::encode(task_id)))
                .as_str(),
        )
        .timeout(timeout_duration())
        .json(req_body);
    send_optional_json(req).await
}

pub async fn scheduler_next(
    req_body: &SchedulerRequestDto,
) -> Result<SchedulerDecisionDto, String> {
    let req = client()
        .post(build_internal_url("/scheduler/next").as_str())
        .timeout(timeout_duration())
        .json(req_body);
    send_json(req).await
}

pub async fn request_pause_task(
    task_id: &str,
    req_body: &PauseTaskRequestDto,
) -> Result<Option<TaskRecordDto>, String> {
    let req = client()
        .post(
            build_internal_url(&format!(
                "/tasks/{}/request-pause",
                urlencoding::encode(task_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration())
        .json(req_body);
    send_optional_json(req).await
}

pub async fn request_stop_task(
    task_id: &str,
    req_body: &StopTaskRequestDto,
) -> Result<Option<TaskRecordDto>, String> {
    let req = client()
        .post(
            build_internal_url(&format!(
                "/tasks/{}/request-stop",
                urlencoding::encode(task_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration())
        .json(req_body);
    send_optional_json(req).await
}

pub async fn resume_task(
    task_id: &str,
    req_body: &ResumeTaskRequestDto,
) -> Result<Option<TaskRecordDto>, String> {
    let req = client()
        .post(
            build_internal_url(&format!("/tasks/{}/resume", urlencoding::encode(task_id))).as_str(),
        )
        .timeout(timeout_duration())
        .json(req_body);
    send_optional_json(req).await
}

pub async fn ack_pause_task(
    task_id: &str,
    req_body: &AckPauseTaskRequestDto,
) -> Result<Option<TaskRecordDto>, String> {
    let req = client()
        .post(
            build_internal_url(&format!(
                "/tasks/{}/ack-pause",
                urlencoding::encode(task_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration())
        .json(req_body);
    send_optional_json(req).await
}

pub async fn ack_stop_task(
    task_id: &str,
    req_body: &AckStopTaskRequestDto,
) -> Result<Option<TaskRecordDto>, String> {
    let req = client()
        .post(
            build_internal_url(&format!("/tasks/{}/ack-stop", urlencoding::encode(task_id)))
                .as_str(),
        )
        .timeout(timeout_duration())
        .json(req_body);
    send_optional_json(req).await
}

pub async fn list_scheduler_scopes(
    user_id: Option<&str>,
    limit: Option<i64>,
) -> Result<Vec<TaskExecutionScopeDto>, String> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(v) = user_id {
        params.push(("user_id".to_string(), v.to_string()));
    }
    if let Some(v) = limit {
        params.push(("limit".to_string(), v.max(1).to_string()));
    }
    let req = client()
        .get(build_internal_url("/scheduler/scopes").as_str())
        .timeout(timeout_duration())
        .query(&params);
    let resp: ListResponse<TaskExecutionScopeDto> = send_json(req).await?;
    Ok(resp.items)
}

pub async fn ack_all_done(req_body: &AckAllDoneRequestDto) -> Result<serde_json::Value, String> {
    let req = client()
        .post(build_internal_url("/scheduler/all-done/ack").as_str())
        .timeout(timeout_duration())
        .json(req_body);
    send_json(req).await
}

#[cfg(test)]
mod tests {
    use super::{pick_auth, TaskServiceAuth};

    #[test]
    fn prefers_service_token_over_forwarded_access_token() {
        let selected = pick_auth(Some("user-access-token".to_string()), "service-token");

        assert_eq!(
            selected,
            TaskServiceAuth::ServiceToken("service-token".to_string())
        );
    }

    #[test]
    fn falls_back_to_bearer_when_service_token_is_missing() {
        let selected = pick_auth(Some(" user-access-token ".to_string()), "   ");

        assert_eq!(
            selected,
            TaskServiceAuth::Bearer("user-access-token".to_string())
        );
    }

    #[test]
    fn omits_auth_when_both_tokens_are_empty() {
        assert_eq!(pick_auth(None, ""), TaskServiceAuth::None);
        assert_eq!(
            pick_auth(Some("   ".to_string()), "   "),
            TaskServiceAuth::None
        );
    }
}
