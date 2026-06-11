use memory_engine_sdk::{ListJobRunsRequest, SdkCountThreadRecordsRequest};

use super::CHATOS_COMPAT_SOURCE_ID;
use super::client::build_client;
use super::mapping::{build_review_repair_scope, build_thread_mapping};
use super::types::{
    ChatosReviewRepairRequest, ReviewRepairJobRunResult, ReviewRepairStatusResult,
    ReviewRepairSummaryRunResult,
};

pub async fn run_chatos_review_repair(
    req: &ChatosReviewRepairRequest,
) -> Result<ReviewRepairSummaryRunResult, String> {
    let scope = build_review_repair_scope(&req.session)?;
    let mapping = build_thread_mapping(&req.session)?;
    let client = build_client()?;
    let pending_message_count = client
        .count_thread_records(
            mapping.thread_id.as_str(),
            &SdkCountThreadRecordsRequest {
                tenant_id: mapping.tenant_id.clone(),
                role: None,
                record_type: Some("message".to_string()),
                summary_status: Some("pending".to_string()),
            },
        )
        .await?;
    if pending_message_count <= 0 {
        return Ok(ReviewRepairSummaryRunResult {
            accepted: false,
            running: false,
            job_run_id: None,
            processed_sessions: 0,
            summarized_sessions: 0,
            generated_summaries: 0,
            marked_messages: 0,
            failed_sessions: 0,
            pending_message_count: 0,
            source_record_count: 0,
            project_id: scope.project_id,
            contact_id: scope.contact_id,
            agent_id: scope.agent_id,
            mode: "review_repair".to_string(),
        });
    }

    let resp = client
        .run_thread_repair_summary(mapping.thread_id.as_str(), mapping.tenant_id.as_str())
        .await?;

    Ok(ReviewRepairSummaryRunResult {
        accepted: resp.accepted,
        running: resp.running,
        job_run_id: resp.job_run_id,
        processed_sessions: usize::from(
            resp.accepted || resp.running || resp.generated || resp.source_record_count > 0,
        ),
        summarized_sessions: usize::from(resp.generated),
        generated_summaries: usize::from(resp.generated),
        marked_messages: if resp.generated {
            resp.source_record_count
        } else {
            0
        },
        failed_sessions: 0,
        pending_message_count,
        source_record_count: resp.source_record_count,
        project_id: scope.project_id,
        contact_id: scope.contact_id,
        agent_id: scope.agent_id,
        mode: "review_repair".to_string(),
    })
}

pub async fn get_chatos_review_repair_job_run(
    req: &ChatosReviewRepairRequest,
    job_run_id: &str,
) -> Result<Option<ReviewRepairJobRunResult>, String> {
    let normalized_job_run_id = job_run_id.trim();
    if normalized_job_run_id.is_empty() {
        return Ok(None);
    }

    let mapping = build_thread_mapping(&req.session)?;
    let client = build_client()?;
    let items = client
        .list_job_runs(&ListJobRunsRequest {
            job_type: Some("thread_repair".to_string()),
            thread_id: Some(mapping.thread_id),
            status: None,
            tenant_id: Some(mapping.tenant_id),
            source_id: Some(CHATOS_COMPAT_SOURCE_ID.to_string()),
            trigger_type: None,
            limit: Some(100),
        })
        .await?;

    Ok(items
        .into_iter()
        .find(|item| item.id == normalized_job_run_id)
        .map(|item| ReviewRepairJobRunResult {
            id: item.id,
            status: item.status,
            output_count: item.output_count,
            processed_count: item.processed_count,
            success_count: item.success_count,
            error_count: item.error_count,
            error_message: item.error_message,
        }))
}

pub async fn get_chatos_review_repair_status(
    req: &ChatosReviewRepairRequest,
) -> Result<ReviewRepairStatusResult, String> {
    let scope = build_review_repair_scope(&req.session)?;
    let mapping = build_thread_mapping(&req.session)?;
    let client = build_client()?;
    let pending_message_count = client
        .count_thread_records(
            mapping.thread_id.as_str(),
            &SdkCountThreadRecordsRequest {
                tenant_id: mapping.tenant_id.clone(),
                role: None,
                record_type: Some("message".to_string()),
                summary_status: Some("pending".to_string()),
            },
        )
        .await?;

    let running_job_count = client
        .list_job_runs(&ListJobRunsRequest {
            job_type: Some("thread_repair".to_string()),
            thread_id: Some(mapping.thread_id.clone()),
            status: Some("running".to_string()),
            tenant_id: Some(mapping.tenant_id.clone()),
            source_id: Some(CHATOS_COMPAT_SOURCE_ID.to_string()),
            trigger_type: None,
            limit: Some(10),
        })
        .await
        .map(|items| items.len() as i64)
        .unwrap_or(0);

    Ok(ReviewRepairStatusResult {
        running: running_job_count > 0,
        running_job_count,
        pending_message_count,
        scope_session_count: usize::from(pending_message_count > 0),
        project_id: scope.project_id,
        contact_id: scope.contact_id,
        agent_id: scope.agent_id,
        job_type: "memory_engine_thread_repair".to_string(),
    })
}
