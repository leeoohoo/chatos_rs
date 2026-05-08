use futures_util::TryStreamExt;
use mongodb::bson::doc;
use tracing::info;

use super::Db;

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
