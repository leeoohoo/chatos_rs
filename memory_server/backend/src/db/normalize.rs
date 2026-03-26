use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson};
use tracing::info;

use super::Db;

pub(super) async fn normalize_summary_status(db: &Db) -> Result<(), String> {
    let collection = db.collection::<mongodb::bson::Document>("session_summaries_v2");

    let to_summarized = collection
        .update_many(
            doc! {
                "rollup_status": "summarized",
                "status": {"$ne": "summarized"}
            },
            doc! { "$set": { "status": "summarized" } },
        )
        .await
        .map_err(|e| e.to_string())?;

    let to_pending = collection
        .update_many(
            doc! {
                "$and": [
                    {
                        "$or": [
                            {"status": "done"},
                            {"status": "pending"},
                            {"status": {"$exists": false}},
                            {"status": Bson::Null}
                        ]
                    },
                    {
                        "$or": [
                            {"rollup_status": {"$ne": "summarized"}},
                            {"rollup_status": {"$exists": false}},
                            {"rollup_status": Bson::Null}
                        ]
                    }
                ]
            },
            doc! { "$set": { "status": "pending" } },
        )
        .await
        .map_err(|e| e.to_string())?;

    if to_summarized.modified_count > 0 || to_pending.modified_count > 0 {
        info!(
            "[MEMORY-SERVER] summary status normalized: summarized={}, pending={}",
            to_summarized.modified_count, to_pending.modified_count
        );
    }

    let to_unsummarized = collection
        .update_many(
            doc! {
                "$or": [
                    {"agent_memory_summarized": {"$exists": false}},
                    {"agent_memory_summarized": Bson::Null}
                ]
            },
            doc! { "$set": { "agent_memory_summarized": 0, "agent_memory_summarized_at": Bson::Null } },
        )
        .await
        .map_err(|e| e.to_string())?;

    if to_unsummarized.modified_count > 0 {
        info!(
            "[MEMORY-SERVER] agent memory summary flags normalized: unsummarized={}",
            to_unsummarized.modified_count
        );
    }

    Ok(())
}

pub(super) async fn normalize_running_job_runs(db: &Db) -> Result<(), String> {
    let collection = db.collection::<mongodb::bson::Document>("job_runs");
    let now = chrono::Utc::now().to_rfc3339();
    let result = collection
        .update_many(
            doc! {"status": "running"},
            doc! {
                "$set": {
                    "status": "failed",
                    "error_message": "interrupted: memory_server restarted",
                    "finished_at": &now,
                }
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    if result.modified_count > 0 {
        info!(
            "[MEMORY-SERVER] recovered interrupted running job_runs: {}",
            result.modified_count
        );
    }

    Ok(())
}

pub(super) async fn normalize_agent_plugin_sources(db: &Db) -> Result<(), String> {
    let collection = db.collection::<crate::models::MemoryAgent>("memory_agents");
    let mut cursor = collection.find(doc! {}).await.map_err(|e| e.to_string())?;
    let mut updated = 0_u64;

    while let Some(agent) = cursor.try_next().await.map_err(|e| e.to_string())? {
        let derived =
            crate::repositories::agents::derive_plugin_sources_for_agent(db, &agent).await?;
        if derived == agent.plugin_sources {
            continue;
        }

        collection
            .update_one(
                doc! { "id": &agent.id },
                doc! { "$set": { "plugin_sources": derived } },
            )
            .await
            .map_err(|e| e.to_string())?;
        updated += 1;
    }

    if updated > 0 {
        info!(
            "[MEMORY-SERVER] normalized memory agent plugin_sources: {}",
            updated
        );
    }

    Ok(())
}
