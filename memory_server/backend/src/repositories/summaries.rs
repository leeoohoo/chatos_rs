use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::models::SessionSummary;

use super::now_rfc3339;
use super::summaries_support::{doc_i64, summary_agent_id_expr, summary_project_id_expr};

mod aggregate_ops;
mod read_ops;
mod write_ops;

pub use self::aggregate_ops::{
    list_agent_ids_with_pending_agent_memory_by_user, list_pending_agent_memory_summaries_by_agent,
    list_session_ids_with_pending_rollup_by_user, list_summary_level_stats,
};
pub use self::read_ops::{
    find_summary_by_source_digest, list_all_summaries_by_session,
    list_pending_summaries_by_level_no_limit, list_summaries,
};
pub use self::write_ops::{
    create_summary, delete_summary, mark_summaries_agent_memory_summarized,
    mark_summaries_rolled_up,
};

pub(super) fn collection(db: &Db) -> mongodb::Collection<SessionSummary> {
    db.collection::<SessionSummary>("session_summaries_v2")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemorySummarySource {
    pub id: String,
    pub source_kind: String,
    pub session_id: String,
    pub summary_text: String,
    pub summary_model: String,
    pub trigger_type: String,
    pub source_start_message_id: Option<String>,
    pub source_end_message_id: Option<String>,
    #[serde(default)]
    pub source_message_count: i64,
    #[serde(default)]
    pub source_estimated_tokens: i64,
    pub status: String,
    #[serde(default)]
    pub level: i64,
    pub project_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
