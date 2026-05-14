use crate::models::memory_runtime_types::DeleteSummaryResultDto;
use crate::models::session_summary_v2::SessionSummaryV2;
use crate::services::chatos_sessions;

pub async fn list_summaries(
    session_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<SessionSummaryV2>, String> {
    chatos_sessions::list_summaries(session_id, limit, offset).await
}

pub async fn delete_summary(
    session_id: &str,
    summary_id: &str,
) -> Result<DeleteSummaryResultDto, String> {
    chatos_sessions::delete_summary(session_id, summary_id).await
}

pub async fn clear_summaries(session_id: &str) -> Result<i64, String> {
    chatos_sessions::clear_summaries(session_id).await
}
