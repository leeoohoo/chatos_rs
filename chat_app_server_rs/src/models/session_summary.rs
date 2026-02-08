use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

use crate::repositories::session_summaries as repo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub session_id: String,
    pub summary_text: String,
    pub summary_prompt: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f64>,
    pub target_summary_tokens: Option<i64>,
    pub keep_last_n: Option<i64>,
    pub message_count: Option<i64>,
    pub approx_tokens: Option<i64>,
    pub first_message_id: Option<String>,
    pub last_message_id: Option<String>,
    pub first_message_created_at: Option<String>,
    pub last_message_created_at: Option<String>,
    pub metadata: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, FromRow)]
pub struct SessionSummaryRow {
    pub id: String,
    pub session_id: String,
    pub summary_text: String,
    pub summary_prompt: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f64>,
    pub target_summary_tokens: Option<i64>,
    pub keep_last_n: Option<i64>,
    pub message_count: Option<i64>,
    pub approx_tokens: Option<i64>,
    pub first_message_id: Option<String>,
    pub last_message_id: Option<String>,
    pub first_message_created_at: Option<String>,
    pub last_message_created_at: Option<String>,
    pub metadata: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl SessionSummaryRow {
    pub fn to_summary(self) -> SessionSummary {
        SessionSummary {
            id: self.id,
            session_id: self.session_id,
            summary_text: self.summary_text,
            summary_prompt: self.summary_prompt,
            model: self.model,
            temperature: self.temperature,
            target_summary_tokens: self.target_summary_tokens,
            keep_last_n: self.keep_last_n,
            message_count: self.message_count,
            approx_tokens: self.approx_tokens,
            first_message_id: self.first_message_id,
            last_message_id: self.last_message_id,
            first_message_created_at: self.first_message_created_at,
            last_message_created_at: self.last_message_created_at,
            metadata: self.metadata.and_then(|v| serde_json::from_str::<Value>(&v).ok()),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl SessionSummary {
    pub fn new(session_id: String, summary_text: String) -> SessionSummary {
        let now = chrono::Utc::now().to_rfc3339();
        SessionSummary {
            id: Uuid::new_v4().to_string(),
            session_id,
            summary_text,
            summary_prompt: None,
            model: None,
            temperature: None,
            target_summary_tokens: None,
            keep_last_n: None,
            message_count: None,
            approx_tokens: None,
            first_message_id: None,
            last_message_id: None,
            first_message_created_at: None,
            last_message_created_at: None,
            metadata: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

pub struct SessionSummaryService;

impl SessionSummaryService {
    pub async fn create(summary: SessionSummary) -> Result<SessionSummary, String> {
        repo::create_summary(&summary).await
    }

    pub async fn list_by_session(session_id: &str, limit: Option<i64>) -> Result<Vec<SessionSummary>, String> {
        repo::list_summaries_by_session(session_id, limit).await
    }

    pub async fn get_last_by_session(session_id: &str) -> Result<Option<SessionSummary>, String> {
        repo::get_last_summary_by_session(session_id).await
    }
}
