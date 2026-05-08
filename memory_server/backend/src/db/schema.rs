use mongodb::bson::doc;
use tracing::info;

use super::index_helpers::{ensure_index, ensure_unique_index, ensure_unique_partial_index};
use super::normalize::{normalize_agent_plugin_sources, normalize_running_job_runs};
use super::Db;

pub async fn init_schema(db: &Db) -> Result<(), String> {
    ensure_session_indexes(db).await?;
    ensure_config_indexes(db).await?;
    ensure_job_run_indexes(db).await?;
    ensure_job_lock_indexes(db).await?;
    ensure_agent_skill_indexes(db).await?;
    ensure_turn_runtime_snapshot_indexes(db).await?;

    normalize_agent_plugin_sources(db).await?;

    info!("[MEMORY-SERVER] mongodb indexes initialized");
    Ok(())
}

async fn ensure_session_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(db.collection("sessions"), doc! {"id": 1}).await?;
    ensure_index(
        db.collection("sessions"),
        doc! {"user_id": 1, "status": 1, "created_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("sessions"),
        doc! {"project_id": 1, "status": 1, "created_at": -1},
    )
    .await?;
    ensure_unique_partial_index(
        db.collection("sessions"),
        doc! {"user_id": 1, "project_id": 1, "metadata.contact.contact_id": 1, "status": 1},
        doc! {
            "status": "active",
            "metadata.contact.contact_id": {"$exists": true, "$type": "string"},
        },
    )
    .await?;
    ensure_unique_partial_index(
        db.collection("sessions"),
        doc! {"user_id": 1, "project_id": 1, "metadata.contact.agent_id": 1, "status": 1},
        doc! {
            "status": "active",
            "metadata.contact.agent_id": {"$exists": true, "$type": "string"},
        },
    )
    .await?;
    Ok(())
}

async fn ensure_config_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(db.collection("ai_model_configs"), doc! {"id": 1}).await?;
    ensure_index(
        db.collection("ai_model_configs"),
        doc! {"user_id": 1, "enabled": 1, "updated_at": -1},
    )
    .await?;

    ensure_unique_index(db.collection("auth_users"), doc! {"user_id": 1}).await?;
    ensure_index(db.collection("auth_users"), doc! {"role": 1}).await?;

    ensure_unique_index(db.collection("summary_job_configs"), doc! {"user_id": 1}).await?;
    ensure_unique_index(
        db.collection("summary_rollup_job_configs"),
        doc! {"user_id": 1},
    )
    .await?;
    ensure_unique_index(
        db.collection("agent_memory_job_configs"),
        doc! {"user_id": 1},
    )
    .await?;
    Ok(())
}

async fn ensure_job_run_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(db.collection("job_runs"), doc! {"id": 1}).await?;
    ensure_index(
        db.collection("job_runs"),
        doc! {"job_type": 1, "started_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("job_runs"),
        doc! {"session_id": 1, "started_at": -1},
    )
    .await?;
    normalize_running_job_runs(db).await?;
    ensure_unique_partial_index(
        db.collection("job_runs"),
        doc! {"job_type": 1, "session_id": 1},
        doc! {
            "status": "running",
            "session_id": {"$exists": true, "$type": "string"},
        },
    )
    .await?;
    Ok(())
}

async fn ensure_job_lock_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(db.collection("job_locks"), doc! {"lock_key": 1}).await?;
    ensure_index(db.collection("job_locks"), doc! {"expires_at_ts": 1}).await?;
    Ok(())
}

async fn ensure_agent_skill_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(db.collection("memory_agents"), doc! {"id": 1}).await?;
    ensure_index(
        db.collection("memory_agents"),
        doc! {"user_id": 1, "enabled": 1, "updated_at": -1},
    )
    .await?;

    ensure_unique_index(db.collection("memory_skill_plugins"), doc! {"id": 1}).await?;
    ensure_unique_index(
        db.collection("memory_skill_plugins"),
        doc! {"user_id": 1, "source": 1},
    )
    .await?;
    ensure_index(
        db.collection("memory_skill_plugins"),
        doc! {"user_id": 1, "updated_at": -1},
    )
    .await?;

    ensure_unique_index(db.collection("memory_skills"), doc! {"id": 1}).await?;
    ensure_unique_index(
        db.collection("memory_skills"),
        doc! {"user_id": 1, "plugin_source": 1, "source_path": 1},
    )
    .await?;
    ensure_index(
        db.collection("memory_skills"),
        doc! {"user_id": 1, "plugin_source": 1, "updated_at": -1},
    )
    .await?;

    ensure_unique_index(db.collection("contacts"), doc! {"id": 1}).await?;
    ensure_unique_index(
        db.collection("contacts"),
        doc! {"user_id": 1, "agent_id": 1},
    )
    .await?;
    ensure_index(
        db.collection("contacts"),
        doc! {"user_id": 1, "status": 1, "updated_at": -1},
    )
    .await?;
    Ok(())
}

async fn ensure_turn_runtime_snapshot_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(db.collection("turn_runtime_snapshots"), doc! {"id": 1}).await?;
    ensure_unique_index(
        db.collection("turn_runtime_snapshots"),
        doc! {"session_id": 1, "turn_id": 1},
    )
    .await?;
    ensure_index(
        db.collection("turn_runtime_snapshots"),
        doc! {"session_id": 1, "user_message_id": 1},
    )
    .await?;
    ensure_index(
        db.collection("turn_runtime_snapshots"),
        doc! {"session_id": 1, "captured_at": -1},
    )
    .await?;
    Ok(())
}
