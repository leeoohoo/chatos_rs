use serde::Serialize;
use serde_json::{json, Value};

use crate::config::AppConfig;
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
    project_id: Option<&'a str>,
    prerequisite_task_ids: Option<Vec<String>>,
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
        })),
        priority: input
            .priority
            .or_else(|| i32::try_from(work_item.priority).ok()),
        tags: Some(normalize_tags(
            input.tags.unwrap_or_else(|| work_item.tags.clone()),
        )),
        default_model_config_id: normalized_optional(input.default_model_config_id),
        project_id: Some(work_item.project_id.as_str()),
        prerequisite_task_ids: input.prerequisite_task_ids.map(normalize_tags),
    };
    let client = reqwest::Client::builder()
        .timeout(config.task_runner_request_timeout)
        .build()
        .map_err(|err| format!("build task runner client failed: {err}"))?;
    let response = client
        .post(endpoint)
        .bearer_auth(access_token.trim())
        .json(&payload)
        .send()
        .await
        .map_err(|err| format!("task runner request failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
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

fn normalize_tags(values: Vec<String>) -> Vec<String> {
    let mut tags = values
        .into_iter()
        .filter_map(|value| normalized_optional(Some(value)))
        .collect::<Vec<_>>();
    tags.sort();
    tags.dedup();
    tags
}
