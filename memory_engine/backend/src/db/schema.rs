use mongodb::bson::doc;

use crate::db::Db;

use super::index_helpers::{ensure_index, ensure_unique_index};

pub async fn init_schema(db: &Db) -> Result<(), String> {
    ensure_source_indexes(db).await?;
    ensure_subject_indexes(db).await?;
    ensure_subject_memory_indexes(db).await?;
    ensure_thread_indexes(db).await?;
    ensure_record_indexes(db).await?;
    ensure_summary_indexes(db).await?;
    Ok(())
}

async fn ensure_source_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(db.collection("engine_sources"), doc! {"tenant_id": 1, "source_id": 1})
        .await?;
    ensure_index(
        db.collection("engine_sources"),
        doc! {"tenant_id": 1, "status": 1, "updated_at": -1},
    )
    .await
}

async fn ensure_subject_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(
        db.collection("engine_subjects"),
        doc! {"tenant_id": 1, "source_id": 1, "subject_id": 1},
    )
    .await?;
    ensure_index(
        db.collection("engine_subjects"),
        doc! {"tenant_id": 1, "source_id": 1, "status": 1, "updated_at": -1},
    )
    .await
}

async fn ensure_subject_memory_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(
        db.collection("engine_subject_memories"),
        doc! {"tenant_id": 1, "source_id": 1, "subject_id": 1, "memory_key": 1},
    )
    .await?;
    ensure_index(
        db.collection("engine_subject_memories"),
        doc! {"tenant_id": 1, "source_id": 1, "subject_id": 1, "updated_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("engine_subject_memories"),
        doc! {"subject_id": 1, "level": -1, "updated_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("engine_subject_memories"),
        doc! {"tenant_id": 1, "source_id": 1, "subject_id": 1, "memory_type": 1, "level": 1, "rollup_status": 1, "updated_at": 1},
    )
    .await?;
    ensure_index(
        db.collection("engine_subject_memories"),
        doc! {"tenant_id": 1, "source_id": 1, "subject_id": 1, "metadata.relation_subject_id": 1, "memory_type": 1, "updated_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("engine_subject_memories"),
        doc! {"tenant_id": 1, "source_id": 1, "subject_id": 1, "memory_type": 1, "level": 1, "source_digest": 1},
    )
    .await
}

async fn ensure_thread_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(
        db.collection("engine_threads"),
        doc! {"tenant_id": 1, "source_id": 1, "id": 1},
    )
    .await?;
    ensure_index(
        db.collection("engine_threads"),
        doc! {"tenant_id": 1, "source_id": 1, "subject_id": 1, "updated_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("engine_threads"),
        doc! {"tenant_id": 1, "source_id": 1, "external_thread_id": 1},
    )
    .await
}

async fn ensure_record_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(db.collection("engine_records"), doc! {"thread_id": 1, "id": 1}).await?;
    ensure_index(
        db.collection("engine_records"),
        doc! {"thread_id": 1, "created_at": 1},
    )
    .await?;
    ensure_index(
        db.collection("engine_records"),
        doc! {"thread_id": 1, "summary_status": 1, "created_at": 1},
    )
    .await?;
    ensure_index(
        db.collection("engine_records"),
        doc! {"tenant_id": 1, "source_id": 1, "external_record_id": 1},
    )
    .await?;
    ensure_index(
        db.collection("engine_records"),
        doc! {"source_id": 1, "id": 1},
    )
    .await
}

async fn ensure_summary_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(db.collection("engine_summaries"), doc! {"id": 1}).await?;
    ensure_index(
        db.collection("engine_summaries"),
        doc! {"thread_id": 1, "level": 1, "created_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("engine_summaries"),
        doc! {"subject_id": 1, "summary_type": 1, "created_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("engine_summaries"),
        doc! {"thread_id": 1, "status": 1, "rollup_status": 1, "level": 1, "created_at": 1},
    )
    .await?;
    ensure_index(
        db.collection("engine_summaries"),
        doc! {"thread_id": 1, "level": 1, "source_digest": 1},
    )
    .await?;
    ensure_index(
        db.collection("engine_summaries"),
        doc! {"tenant_id": 1, "source_id": 1, "summary_type": 1, "rollup_status": 1, "level": 1, "created_at": 1},
    )
    .await
}
