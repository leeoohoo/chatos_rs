use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::repositories::session_summaries_v2 as repo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummaryV2 {
    pub id: String,
    pub session_id: String,
    pub summary_text: String,
    pub summary_model: String,
    pub trigger_type: String,
    pub source_start_message_id: Option<String>,
    pub source_end_message_id: Option<String>,
    pub source_message_count: i64,
    pub source_estimated_tokens: i64,
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, FromRow)]
pub struct SessionSummaryV2Row {
    pub id: String,
    pub session_id: String,
    pub summary_text: String,
    pub summary_model: String,
    pub trigger_type: String,
    pub source_start_message_id: Option<String>,
    pub source_end_message_id: Option<String>,
    pub source_message_count: i64,
    pub source_estimated_tokens: i64,
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl SessionSummaryV2Row {
    pub fn to_summary(self) -> SessionSummaryV2 {
        SessionSummaryV2 {
            id: self.id,
            session_id: self.session_id,
            summary_text: self.summary_text,
            summary_model: self.summary_model,
            trigger_type: self.trigger_type,
            source_start_message_id: self.source_start_message_id,
            source_end_message_id: self.source_end_message_id,
            source_message_count: self.source_message_count,
            source_estimated_tokens: self.source_estimated_tokens,
            status: self.status,
            error_message: self.error_message,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl SessionSummaryV2 {
    pub fn new(
        session_id: String,
        summary_text: String,
        summary_model: String,
        trigger_type: String,
        source_start_message_id: Option<String>,
        source_end_message_id: Option<String>,
        source_message_count: i64,
        source_estimated_tokens: i64,
        status: String,
        error_message: Option<String>,
    ) -> SessionSummaryV2 {
        let now = crate::core::time::now_rfc3339();
        SessionSummaryV2 {
            id: Uuid::new_v4().to_string(),
            session_id,
            summary_text,
            summary_model,
            trigger_type,
            source_start_message_id,
            source_end_message_id,
            source_message_count,
            source_estimated_tokens,
            status,
            error_message,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

pub struct SessionSummaryV2Service;

impl SessionSummaryV2Service {
    pub async fn create(summary: SessionSummaryV2) -> Result<SessionSummaryV2, String> {
        repo::create_summary(&summary).await
    }

    pub async fn list_by_session(
        session_id: &str,
        limit: Option<i64>,
        offset: i64,
    ) -> Result<Vec<SessionSummaryV2>, String> {
        repo::list_summaries_by_session(session_id, limit, offset).await
    }

    pub async fn count_by_session(session_id: &str) -> Result<i64, String> {
        repo::count_summaries_by_session(session_id).await
    }

    pub async fn delete_by_id(session_id: &str, summary_id: &str) -> Result<bool, String> {
        repo::delete_summary_by_id(session_id, summary_id).await
    }

    pub async fn delete_by_session(session_id: &str) -> Result<i64, String> {
        repo::delete_summaries_by_session(session_id).await
    }
}
