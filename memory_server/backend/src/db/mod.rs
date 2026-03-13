use mongodb::bson::{doc, Bson};
use mongodb::options::{ClientOptions, IndexOptions};
use mongodb::{Client, Collection, Database, IndexModel};
use tracing::info;

use crate::config::AppConfig;

pub type Db = Database;

pub async fn init_pool(config: &AppConfig) -> Result<Db, String> {
    let mut options = ClientOptions::parse(config.mongodb_uri.as_str())
        .await
        .map_err(|e| format!("invalid mongodb uri: {e}"))?;
    options.app_name = Some("memory_server".to_string());

    let client = Client::with_options(options).map_err(|e| e.to_string())?;
    let db = client.database(config.mongodb_database.as_str());

    db.run_command(doc! { "ping": 1 })
        .await
        .map_err(|e| format!("mongodb ping failed: {e}"))?;

    Ok(db)
}

pub async fn init_schema(db: &Db) -> Result<(), String> {
    ensure_unique_index(
        db.collection::<mongodb::bson::Document>("sessions"),
        doc! {"id": 1},
    )
    .await?;
    ensure_index(
        db.collection::<mongodb::bson::Document>("sessions"),
        doc! {"user_id": 1, "status": 1, "created_at": -1},
    )
    .await?;
    ensure_index(
        db.collection::<mongodb::bson::Document>("sessions"),
        doc! {"project_id": 1, "status": 1, "created_at": -1},
    )
    .await?;

    ensure_unique_index(
        db.collection::<mongodb::bson::Document>("messages"),
        doc! {"id": 1},
    )
    .await?;
    ensure_index(
        db.collection::<mongodb::bson::Document>("messages"),
        doc! {"session_id": 1, "created_at": 1},
    )
    .await?;
    ensure_index(
        db.collection::<mongodb::bson::Document>("messages"),
        doc! {"session_id": 1, "summary_status": 1, "created_at": 1},
    )
    .await?;
    ensure_index(
        db.collection::<mongodb::bson::Document>("messages"),
        doc! {"summary_id": 1},
    )
    .await?;

    ensure_unique_index(
        db.collection::<mongodb::bson::Document>("session_summaries_v2"),
        doc! {"id": 1},
    )
    .await?;
    ensure_index(
        db.collection::<mongodb::bson::Document>("session_summaries_v2"),
        doc! {"session_id": 1, "created_at": 1},
    )
    .await?;
    ensure_index(
        db.collection::<mongodb::bson::Document>("session_summaries_v2"),
        doc! {"session_id": 1, "status": 1, "created_at": 1},
    )
    .await?;
    ensure_index(
        db.collection::<mongodb::bson::Document>("session_summaries_v2"),
        doc! {"session_id": 1, "level": 1, "status": 1, "created_at": 1},
    )
    .await?;
    ensure_index(
        db.collection::<mongodb::bson::Document>("session_summaries_v2"),
        doc! {"rollup_summary_id": 1},
    )
    .await?;

    ensure_unique_index(
        db.collection::<mongodb::bson::Document>("ai_model_configs"),
        doc! {"id": 1},
    )
    .await?;
    ensure_index(
        db.collection::<mongodb::bson::Document>("ai_model_configs"),
        doc! {"user_id": 1, "enabled": 1, "updated_at": -1},
    )
    .await?;

    ensure_unique_index(
        db.collection::<mongodb::bson::Document>("auth_users"),
        doc! {"user_id": 1},
    )
    .await?;
    ensure_index(
        db.collection::<mongodb::bson::Document>("auth_users"),
        doc! {"role": 1},
    )
    .await?;

    ensure_unique_index(
        db.collection::<mongodb::bson::Document>("summary_job_configs"),
        doc! {"user_id": 1},
    )
    .await?;
    ensure_unique_index(
        db.collection::<mongodb::bson::Document>("summary_rollup_job_configs"),
        doc! {"user_id": 1},
    )
    .await?;

    ensure_unique_index(
        db.collection::<mongodb::bson::Document>("job_runs"),
        doc! {"id": 1},
    )
    .await?;
    ensure_index(
        db.collection::<mongodb::bson::Document>("job_runs"),
        doc! {"job_type": 1, "started_at": -1},
    )
    .await?;
    ensure_index(
        db.collection::<mongodb::bson::Document>("job_runs"),
        doc! {"session_id": 1, "started_at": -1},
    )
    .await?;

    normalize_summary_status(db).await?;

    info!("[MEMORY-SERVER] mongodb indexes initialized");
    Ok(())
}

async fn normalize_summary_status(db: &Db) -> Result<(), String> {
    let collection = db.collection::<mongodb::bson::Document>("session_summaries_v2");

    // Legacy data compatibility:
    // 1) rollup_status=summarized -> status=summarized
    // 2) status=done/missing + not summarized -> status=pending
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

    Ok(())
}

async fn ensure_unique_index(
    collection: Collection<mongodb::bson::Document>,
    keys: mongodb::bson::Document,
) -> Result<(), String> {
    let options = IndexOptions::builder().unique(Some(true)).build();
    let model = IndexModel::builder().keys(keys).options(options).build();
    collection
        .create_index(model)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn ensure_index(
    collection: Collection<mongodb::bson::Document>,
    keys: mongodb::bson::Document,
) -> Result<(), String> {
    let model = IndexModel::builder().keys(keys).build();
    collection
        .create_index(model)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
