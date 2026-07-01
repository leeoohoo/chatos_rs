// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::doc;

use crate::db::Db;

use super::index_helpers::{
    drop_index_if_exists, ensure_index, ensure_named_index, ensure_named_unique_index,
    ensure_unique_index,
};

pub async fn init_schema(db: &Db) -> Result<(), String> {
    ensure_control_plane_indexes(db).await?;
    ensure_source_indexes(db).await?;
    ensure_subject_indexes(db).await?;
    ensure_subject_memory_scope_indexes(db).await?;
    ensure_subject_memory_indexes(db).await?;
    ensure_thread_indexes(db).await?;
    ensure_record_indexes(db).await?;
    ensure_compact_turn_indexes(db).await?;
    ensure_summary_indexes(db).await?;
    ensure_thread_snapshot_indexes(db).await?;
    Ok(())
}

async fn ensure_control_plane_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(db.collection("engine_model_profiles"), doc! {"id": 1}).await?;
    ensure_index(
        db.collection("engine_model_profiles"),
        doc! {"enabled": 1, "updated_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("engine_model_profiles"),
        doc! {"owner_user_id": 1, "is_default": -1, "enabled": -1, "updated_at": -1},
    )
    .await?;

    ensure_unique_index(db.collection("engine_job_policies"), doc! {"job_type": 1}).await?;
    ensure_index(
        db.collection("engine_job_policies"),
        doc! {"enabled": 1, "updated_at": -1},
    )
    .await?;

    ensure_unique_index(db.collection("engine_job_runs"), doc! {"id": 1}).await?;
    ensure_index(
        db.collection("engine_job_runs"),
        doc! {"job_type": 1, "status": 1, "started_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("engine_job_runs"),
        doc! {"tenant_id": 1, "source_id": 1, "started_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("engine_job_runs"),
        doc! {"status": 1, "started_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("engine_job_runs"),
        doc! {"thread_id": 1, "status": 1, "started_at": -1},
    )
    .await
}

async fn ensure_source_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(
        db.collection("engine_sources"),
        doc! {"tenant_id": 1, "source_id": 1},
    )
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

async fn ensure_subject_memory_scope_indexes(db: &Db) -> Result<(), String> {
    ensure_unique_index(
        db.collection("engine_subject_memory_scopes"),
        doc! {"tenant_id": 1, "source_id": 1, "scope_key": 1},
    )
    .await?;
    ensure_index(
        db.collection("engine_subject_memory_scopes"),
        doc! {"status": 1, "source_id": 1, "updated_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("engine_subject_memory_scopes"),
        doc! {"tenant_id": 1, "source_id": 1, "subject_id": 1, "memory_type": 1, "updated_at": -1},
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
    .await?;
    ensure_index(
        db.collection("engine_threads"),
        doc! {"tenant_id": 1, "source_id": 1, "summary_status": 1, "summary_lock_expires_at": 1, "updated_at": -1},
    )
    .await?;
    ensure_index(
        db.collection("engine_threads"),
        doc! {"tenant_id": 1, "source_id": 1, "summary_status": 1, "updated_at": 1},
    )
    .await?;
    ensure_named_index(
        db.collection("engine_threads"),
        "idx_engine_threads_scope_summary_status_pending_tokens_updated_at",
        doc! {"tenant_id": 1, "source_id": 1, "summary_status": 1, "pending_summary_tokens": 1, "updated_at": 1},
    )
    .await
}

async fn ensure_record_indexes(db: &Db) -> Result<(), String> {
    let collection = db.collection("engine_records");
    ensure_named_unique_index(
        collection.clone(),
        "uq_engine_records_scope_thread_record",
        doc! {"tenant_id": 1, "source_id": 1, "thread_id": 1, "id": 1},
    )
    .await?;
    ensure_named_index(
        collection.clone(),
        "idx_engine_records_scope_thread_created_at",
        doc! {"tenant_id": 1, "source_id": 1, "thread_id": 1, "created_at": 1},
    )
    .await?;
    ensure_named_index(
        collection.clone(),
        "idx_engine_records_scope_thread_summary_status_created_at",
        doc! {"tenant_id": 1, "source_id": 1, "thread_id": 1, "summary_status": 1, "created_at": 1},
    )
    .await?;
    ensure_index(
        collection.clone(),
        doc! {"tenant_id": 1, "source_id": 1, "external_record_id": 1},
    )
    .await?;
    ensure_named_index(
        collection.clone(),
        "idx_engine_records_scope_record_id",
        doc! {"tenant_id": 1, "source_id": 1, "id": 1},
    )
    .await?;
    drop_index_if_exists(collection.clone(), "thread_id_1_id_1").await?;
    drop_index_if_exists(collection.clone(), "thread_id_1_created_at_1").await?;
    drop_index_if_exists(collection, "thread_id_1_summary_status_1_created_at_1").await
}

async fn ensure_compact_turn_indexes(db: &Db) -> Result<(), String> {
    let collection = db.collection("engine_compact_turns");
    ensure_named_unique_index(
        collection.clone(),
        "uq_engine_compact_turns_scope_thread_type_turn",
        doc! {"tenant_id": 1, "source_id": 1, "thread_id": 1, "record_type": 1, "turn_id": 1},
    )
    .await?;
    ensure_named_index(
        collection,
        "idx_engine_compact_turns_scope_thread_type_user_created_at",
        doc! {"tenant_id": 1, "source_id": 1, "thread_id": 1, "record_type": 1, "user_created_at": -1, "user_record_id": -1},
    )
    .await
}

async fn ensure_summary_indexes(db: &Db) -> Result<(), String> {
    let collection = db.collection("engine_summaries");
    ensure_unique_index(collection.clone(), doc! {"id": 1}).await?;
    ensure_named_index(
        collection.clone(),
        "idx_engine_summaries_scope_thread_level_created_at",
        doc! {"tenant_id": 1, "source_id": 1, "thread_id": 1, "level": 1, "created_at": -1},
    )
    .await?;
    ensure_index(
        collection.clone(),
        doc! {"subject_id": 1, "summary_type": 1, "created_at": -1},
    )
    .await?;
    ensure_named_index(
        collection.clone(),
        "idx_engine_summaries_scope_thread_status_rollup_status_level_created_at",
        doc! {"tenant_id": 1, "source_id": 1, "thread_id": 1, "status": 1, "rollup_status": 1, "level": 1, "created_at": 1},
    )
    .await?;
    ensure_named_index(
        collection.clone(),
        "idx_engine_summaries_scope_thread_level_source_digest",
        doc! {"tenant_id": 1, "source_id": 1, "thread_id": 1, "level": 1, "source_digest": 1},
    )
    .await?;
    ensure_index(
        collection.clone(),
        doc! {"tenant_id": 1, "source_id": 1, "summary_type": 1, "rollup_status": 1, "level": 1, "created_at": 1},
    )
    .await?;
    drop_index_if_exists(collection.clone(), "thread_id_1_level_1_created_at_-1").await?;
    drop_index_if_exists(
        collection.clone(),
        "thread_id_1_status_1_rollup_status_1_level_1_created_at_1",
    )
    .await?;
    drop_index_if_exists(collection, "thread_id_1_level_1_source_digest_1").await
}

async fn ensure_thread_snapshot_indexes(db: &Db) -> Result<(), String> {
    let collection = db.collection("engine_thread_snapshots");
    ensure_named_unique_index(
        collection.clone(),
        "uq_engine_thread_snapshots_scope_thread_snapshot_turn",
        doc! {"tenant_id": 1, "source_id": 1, "thread_id": 1, "snapshot_type": 1, "turn_id": 1},
    )
    .await?;
    ensure_index(
        collection.clone(),
        doc! {"tenant_id": 1, "source_id": 1, "thread_id": 1, "snapshot_type": 1, "captured_at": -1},
    )
    .await?;
    ensure_named_index(
        collection.clone(),
        "idx_engine_thread_snapshots_scope_thread_snapshot_updated_at",
        doc! {"tenant_id": 1, "source_id": 1, "thread_id": 1, "snapshot_type": 1, "updated_at": -1},
    )
    .await?;
    drop_index_if_exists(collection.clone(), "thread_id_1_snapshot_type_1_turn_id_1").await?;
    drop_index_if_exists(collection, "thread_id_1_snapshot_type_1_updated_at_-1").await
}
