use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

use crate::repositories::sub_agent_run_messages as repo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentRunMessage {
    pub id: String,
    pub run_id: String,
    pub role: String,
    pub content: String,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<Value>,
    pub summary_status: Option<String>,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct SubAgentRunMessageRow {
    pub id: String,
    pub run_id: String,
    pub role: String,
    pub content: String,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<String>,
    pub summary_status: Option<String>,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: String,
}

impl SubAgentRunMessageRow {
    pub fn to_message(self) -> SubAgentRunMessage {
        SubAgentRunMessage {
            id: self.id,
            run_id: self.run_id,
            role: self.role,
            content: self.content,
            tool_call_id: self.tool_call_id,
            reasoning: self.reasoning,
            metadata: self
                .metadata
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok()),
            summary_status: self.summary_status,
            summary_id: self.summary_id,
            summarized_at: self.summarized_at,
            created_at: self.created_at,
        }
    }
}

impl SubAgentRunMessage {
    pub fn new(run_id: String, role: String, content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            run_id,
            role,
            content,
            tool_call_id: None,
            reasoning: None,
            metadata: None,
            summary_status: Some("pending".to_string()),
            summary_id: None,
            summarized_at: None,
            created_at: crate::core::time::now_rfc3339(),
        }
    }
}

pub struct SubAgentRunMessageService;

impl SubAgentRunMessageService {
    pub async fn create(message: SubAgentRunMessage) -> Result<SubAgentRunMessage, String> {
        repo::create_message(&message).await
    }

    pub async fn list_by_run(
        run_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<SubAgentRunMessage>, String> {
        repo::list_messages_by_run(run_id, limit).await
    }

    pub async fn list_runs_with_pending_summary(limit: Option<i64>) -> Result<Vec<String>, String> {
        repo::list_runs_with_pending_summary(limit).await
    }

    pub async fn get_pending_for_summary(
        run_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<SubAgentRunMessage>, String> {
        repo::get_pending_messages_for_summary(run_id, limit).await
    }

    pub async fn mark_summarized(
        run_id: &str,
        message_ids: &[String],
        summary_id: &str,
        summarized_at: &str,
    ) -> Result<usize, String> {
        repo::mark_messages_summarized(run_id, message_ids, summary_id, summarized_at).await
    }
}
