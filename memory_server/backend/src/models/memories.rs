use serde::{Deserialize, Serialize};

use super::{default_i64_0, default_i64_1};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMemory {
    pub id: String,
    pub user_id: String,
    pub contact_id: String,
    pub agent_id: String,
    pub project_id: String,
    pub memory_text: String,
    #[serde(default = "default_i64_1")]
    pub memory_version: i64,
    #[serde(default = "default_i64_0")]
    pub recall_summarized: i64,
    pub recall_summarized_at: Option<String>,
    pub last_source_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecall {
    pub id: String,
    pub user_id: String,
    pub agent_id: String,
    pub recall_key: String,
    pub source_digest: Option<String>,
    pub recall_text: String,
    #[serde(default = "default_i64_0")]
    pub level: i64,
    #[serde(default = "default_i64_0")]
    pub rolled_up: i64,
    pub rollup_recall_key: Option<String>,
    pub rolled_up_at: Option<String>,
    pub confidence: Option<f64>,
    pub last_seen_at: Option<String>,
    pub updated_at: String,
}
