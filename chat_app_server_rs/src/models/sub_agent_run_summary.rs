use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::repositories::sub_agent_run_summaries as repo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentRunSummary {
    pub id: String,
    pub run_id: String,
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

#[derive(Debug, Clone, FromRow)]
pub struct SubAgentRunSummaryRow {
    pub id: String,
    pub run_id: String,
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

impl SubAgentRunSummaryRow {
    pub fn to_summary(self) -> SubAgentRunSummary {
        SubAgentRunSummary {
            id: self.id,
            run_id: self.run_id,
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

impl SubAgentRunSummary {
    pub fn new(
        run_id: String,
        summary_text: String,
        summary_model: String,
        trigger_type: String,
        source_start_message_id: Option<String>,
        source_end_message_id: Option<String>,
        source_message_count: i64,
        source_estimated_tokens: i64,
        status: String,
        error_message: Option<String>,
    ) -> Self {
        let now = crate::core::time::now_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            run_id,
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

pub struct SubAgentRunSummaryService;

impl SubAgentRunSummaryService {
    pub async fn create(summary: SubAgentRunSummary) -> Result<SubAgentRunSummary, String> {
        repo::create_summary(&summary).await
    }

    pub async fn list_by_run(
        run_id: &str,
        limit: Option<i64>,
        offset: i64,
    ) -> Result<Vec<SubAgentRunSummary>, String> {
        repo::list_summaries_by_run(run_id, limit, offset).await
    }
}
