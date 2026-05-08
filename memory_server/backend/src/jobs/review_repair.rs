use crate::services::memory_engine_client;

use super::review_repair_types::{ScopedReviewRepairStatus, ScopedSummaryRunResult};

fn build_scope_label(
    project_id: &str,
    contact_id: Option<&str>,
    agent_id: Option<&str>,
) -> Option<String> {
    if let Some(contact_id) = contact_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(format!("contact_project:{}:{}", contact_id, project_id))
    } else {
        agent_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|agent_id| format!("agent_project:{}:{}", agent_id, project_id))
    }
}

pub async fn run_once_for_scope(
    config: &crate::config::AppConfig,
    user_id: &str,
    project_id: &str,
    contact_id: Option<&str>,
    agent_id: Option<&str>,
) -> Result<ScopedSummaryRunResult, String> {
    let Some(scope_label) = build_scope_label(project_id, contact_id, agent_id) else {
        return Ok(ScopedSummaryRunResult {
            processed_sessions: 0,
            summarized_sessions: 0,
            generated_summaries: 0,
            marked_messages: 0,
            failed_sessions: 0,
            pending_message_count: 0,
            project_id: project_id.to_string(),
            contact_id: contact_id.map(ToOwned::to_owned),
            agent_id: agent_id.map(ToOwned::to_owned),
            mode: "review_repair".to_string(),
        });
    };

    let result = memory_engine_client::run_review_repair_scope(
        config,
        user_id,
        scope_label.as_str(),
        5_000,
        50,
    )
    .await?;

    Ok(ScopedSummaryRunResult {
        processed_sessions: result.processed_threads,
        summarized_sessions: result.summarized_threads,
        generated_summaries: result.generated_summaries,
        marked_messages: 0,
        failed_sessions: 0,
        pending_message_count: result.pending_record_count,
        project_id: project_id.to_string(),
        contact_id: contact_id.map(ToOwned::to_owned),
        agent_id: agent_id.map(ToOwned::to_owned),
        mode: "review_repair".to_string(),
    })
}

pub async fn get_status_for_scope(
    config: &crate::config::AppConfig,
    user_id: &str,
    project_id: &str,
    contact_id: Option<&str>,
    agent_id: Option<&str>,
) -> Result<ScopedReviewRepairStatus, String> {
    let Some(scope_label) = build_scope_label(project_id, contact_id, agent_id) else {
        return Ok(ScopedReviewRepairStatus {
            running: false,
            running_job_count: 0,
            pending_message_count: 0,
            scope_session_count: 0,
            project_id: project_id.to_string(),
            contact_id: contact_id.map(ToOwned::to_owned),
            agent_id: agent_id.map(ToOwned::to_owned),
            job_type: "memory_engine_thread_repair".to_string(),
        });
    };

    let status = memory_engine_client::get_review_repair_scope_status(
        config,
        user_id,
        scope_label.as_str(),
        5_000,
    )
    .await?;

    Ok(ScopedReviewRepairStatus {
        running: status.running,
        running_job_count: status.running_job_count,
        pending_message_count: status.pending_record_count,
        scope_session_count: status.scope_thread_count,
        project_id: project_id.to_string(),
        contact_id: contact_id.map(ToOwned::to_owned),
        agent_id: agent_id.map(ToOwned::to_owned),
        job_type: status.job_type,
    })
}
