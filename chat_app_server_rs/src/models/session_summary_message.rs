use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::repositories::session_summary_messages as repo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummaryMessage {
    pub id: String,
    pub summary_id: String,
    pub session_id: String,
    pub message_id: String,
    pub created_at: String,
}

#[derive(Debug, FromRow)]
pub struct SessionSummaryMessageRow {
    pub id: String,
    pub summary_id: String,
    pub session_id: String,
    pub message_id: String,
    pub created_at: String,
}

impl SessionSummaryMessageRow {
    pub fn to_summary_message(self) -> SessionSummaryMessage {
        SessionSummaryMessage {
            id: self.id,
            summary_id: self.summary_id,
            session_id: self.session_id,
            message_id: self.message_id,
            created_at: self.created_at,
        }
    }
}

impl SessionSummaryMessage {
    pub fn new(
        summary_id: String,
        session_id: String,
        message_id: String,
    ) -> SessionSummaryMessage {
        SessionSummaryMessage {
            id: Uuid::new_v4().to_string(),
            summary_id,
            session_id,
            message_id,
            created_at: crate::core::time::now_rfc3339(),
        }
    }
}

pub struct SessionSummaryMessageService;

impl SessionSummaryMessageService {
    pub async fn create_links(
        summary_id: &str,
        session_id: &str,
        message_ids: &[String],
    ) -> Result<usize, String> {
        repo::create_summary_message_links(summary_id, session_id, message_ids).await
    }
}
