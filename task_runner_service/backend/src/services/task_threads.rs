// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use memory_engine_sdk::SdkUpsertThreadRequest;
use serde_json::json;

use crate::config::AppConfig;
use crate::models::TaskRecord;

use super::TaskStatusExt;

pub(super) async fn ensure_task_thread_for_config(
    config: &AppConfig,
    task: &TaskRecord,
) -> Result<(), String> {
    let Some(client) = config.memory_client()? else {
        return Ok(());
    };
    client
        .upsert_thread(
            &task.memory_thread_id,
            &SdkUpsertThreadRequest {
                tenant_id: task.tenant_id.clone(),
                subject_id: task.subject_id.clone(),
                thread_type: "task".to_string(),
                external_thread_id: Some(task.id.clone()),
                title: Some(task.title.clone()),
                labels: Some(vec![
                    "task_runner".to_string(),
                    format!("task_status:{}", task.status.status_string()),
                ]),
                metadata: Some(json!({
                    "task_id": task.id,
                    "service": "task_runner_service",
                })),
                status: Some("active".to_string()),
                created_at: None,
                updated_at: None,
                archived_at: None,
            },
        )
        .await
        .map(|_| ())
}
