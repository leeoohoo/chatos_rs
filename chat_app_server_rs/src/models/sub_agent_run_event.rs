use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::repositories::sub_agent_run_events as repo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentRunEvent {
    pub id: String,
    pub job_id: String,
    pub event_type: String,
    pub payload_json: Option<String>,
    pub created_at: String,
    pub session_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct SubAgentRunEventRow {
    pub id: String,
    pub job_id: String,
    pub event_type: String,
    pub payload_json: Option<String>,
    pub created_at: String,
    pub session_id: String,
    pub run_id: String,
}

impl SubAgentRunEventRow {
    pub fn to_event(self) -> SubAgentRunEvent {
        SubAgentRunEvent {
            id: self.id,
            job_id: self.job_id,
            event_type: self.event_type,
            payload_json: self.payload_json,
            created_at: self.created_at,
            session_id: self.session_id,
            run_id: self.run_id,
        }
    }
}

pub struct SubAgentRunEventService;

impl SubAgentRunEventService {
    pub async fn create(event: SubAgentRunEvent) -> Result<SubAgentRunEvent, String> {
        repo::create_event(&event).await
    }

    pub async fn list_by_job_id(job_id: &str) -> Result<Vec<SubAgentRunEvent>, String> {
        repo::list_events_by_job_id(job_id).await
    }
}
