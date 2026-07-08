// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use crate::models::memory_runtime_types::{
    SyncTurnRuntimeSnapshotRequestDto, TurnRuntimeSnapshotDto, TurnRuntimeSnapshotLookupResponseDto,
};
use crate::models::session::Session;

use super::client::build_client;
use super::mappers::{
    build_chatos_turn_snapshot_payload_value, engine_lookup_to_turn_snapshot_lookup,
    engine_snapshot_to_turn_snapshot,
};
use super::mapping::build_thread_mapping;
use super::CHATOS_TURN_RUNTIME_SNAPSHOT_TYPE;
use memory_engine_sdk::SdkUpsertThreadSnapshotRequest;

pub async fn sync_chatos_turn_runtime_snapshot(
    session: &Session,
    turn_id: &str,
    payload: &SyncTurnRuntimeSnapshotRequestDto,
) -> Result<TurnRuntimeSnapshotDto, String> {
    let mapping = build_thread_mapping(session)?;
    let client = build_client()?;
    let payload_value = build_chatos_turn_snapshot_payload_value(payload)?;
    let metadata = Some(json!({
        "subsystem": "chatos",
        "resource_type": "turn_runtime_snapshot",
        "schema_version": "chatos.turn_runtime_snapshot.v1",
    }));

    let resp = client
        .upsert_thread_snapshot(
            mapping.thread_id.as_str(),
            CHATOS_TURN_RUNTIME_SNAPSHOT_TYPE,
            turn_id,
            &SdkUpsertThreadSnapshotRequest {
                tenant_id: mapping.tenant_id,
                user_message_id: payload.user_message_id.clone(),
                status: payload.status.clone(),
                snapshot_source: payload.snapshot_source.clone(),
                snapshot_version: payload.snapshot_version,
                payload: payload_value,
                metadata,
                captured_at: payload.captured_at.clone(),
            },
        )
        .await?;

    engine_snapshot_to_turn_snapshot(resp)
}

pub async fn get_latest_chatos_turn_runtime_snapshot(
    session: &Session,
) -> Result<TurnRuntimeSnapshotLookupResponseDto, String> {
    let mapping = build_thread_mapping(session)?;
    let client = build_client()?;
    let resp = client
        .get_latest_thread_snapshot(
            mapping.thread_id.as_str(),
            CHATOS_TURN_RUNTIME_SNAPSHOT_TYPE,
            mapping.tenant_id.as_str(),
        )
        .await?;
    engine_lookup_to_turn_snapshot_lookup(resp)
}

pub async fn get_chatos_turn_runtime_snapshot_by_turn(
    session: &Session,
    turn_id: &str,
) -> Result<TurnRuntimeSnapshotLookupResponseDto, String> {
    let mapping = build_thread_mapping(session)?;
    let client = build_client()?;
    let resp = client
        .get_thread_snapshot_by_turn(
            mapping.thread_id.as_str(),
            CHATOS_TURN_RUNTIME_SNAPSHOT_TYPE,
            turn_id,
            mapping.tenant_id.as_str(),
        )
        .await?;
    engine_lookup_to_turn_snapshot_lookup(resp)
}
