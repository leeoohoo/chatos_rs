use mongodb::bson::doc;

use crate::db::Db;
use crate::models::{AgentMemoryJobConfig, SummaryJobConfig, SummaryRollupJobConfig};

pub(super) fn summary_job_collection(db: &Db) -> mongodb::Collection<SummaryJobConfig> {
    db.collection::<SummaryJobConfig>("summary_job_configs")
}

pub(super) fn summary_rollup_collection(db: &Db) -> mongodb::Collection<SummaryRollupJobConfig> {
    db.collection::<SummaryRollupJobConfig>("summary_rollup_job_configs")
}

pub(super) fn agent_memory_job_collection(db: &Db) -> mongodb::Collection<AgentMemoryJobConfig> {
    db.collection::<AgentMemoryJobConfig>("agent_memory_job_configs")
}

pub async fn delete_user_job_configs(db: &Db, user_id: &str) -> Result<(), String> {
    summary_job_collection(db)
        .delete_one(doc! {"user_id": user_id})
        .await
        .map_err(|e| e.to_string())?;

    summary_rollup_collection(db)
        .delete_one(doc! {"user_id": user_id})
        .await
        .map_err(|e| e.to_string())?;

    agent_memory_job_collection(db)
        .delete_one(doc! {"user_id": user_id})
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}
