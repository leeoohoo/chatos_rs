use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::repositories::sub_agent_runs as repo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentRun {
    pub id: String,
    pub status: String,
    pub task: String,
    pub agent_id: Option<String>,
    pub command_id: Option<String>,
    pub payload_json: Option<String>,
    pub result_json: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub session_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct SubAgentRunRow {
    pub id: String,
    pub status: String,
    pub task: String,
    pub agent_id: Option<String>,
    pub command_id: Option<String>,
    pub payload_json: Option<String>,
    pub result_json: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub session_id: String,
    pub run_id: String,
}

impl SubAgentRunRow {
    pub fn to_run(self) -> SubAgentRun {
        SubAgentRun {
            id: self.id,
            status: self.status,
            task: self.task,
            agent_id: self.agent_id,
            command_id: self.command_id,
            payload_json: self.payload_json,
            result_json: self.result_json,
            error: self.error,
            created_at: self.created_at,
            updated_at: self.updated_at,
            session_id: self.session_id,
            run_id: self.run_id,
        }
    }
}

pub struct SubAgentRunService;

impl SubAgentRunService {
    pub async fn create(run: SubAgentRun) -> Result<SubAgentRun, String> {
        repo::create_run(&run).await
    }

    pub async fn update_status(
        id: &str,
        status: &str,
        result_json: Option<String>,
        error: Option<String>,
    ) -> Result<Option<SubAgentRun>, String> {
        repo::update_run_status(id, status, result_json, error).await
    }

    pub async fn get_by_id(id: &str) -> Result<Option<SubAgentRun>, String> {
        repo::get_run_by_id(id).await
    }
}
