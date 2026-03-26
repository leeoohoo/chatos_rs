use crate::db::Db;
use crate::models::{AgentRecall, ProjectMemory};

use super::now_rfc3339;

mod read_ops;
mod write_ops;

pub use self::read_ops::{
    find_agent_recall_by_source_digest, list_agent_ids_with_pending_recall_rollup_by_user,
    list_agent_recalls, list_pending_agent_recalls_by_level, list_project_memories,
    list_project_memories_by_contact,
};
pub use self::write_ops::{
    mark_agent_recalls_rolled_up, upsert_agent_recall, upsert_project_memory,
    UpsertAgentRecallInput, UpsertProjectMemoryInput,
};

pub(super) fn project_memories_collection(db: &Db) -> mongodb::Collection<ProjectMemory> {
    db.collection::<ProjectMemory>("project_memories")
}

pub(super) fn agent_recalls_collection(db: &Db) -> mongodb::Collection<AgentRecall> {
    db.collection::<AgentRecall>("agent_recalls")
}
