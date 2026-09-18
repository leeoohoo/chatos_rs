// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::repositories::db::{
    db_error, decode_optional, json, optional_timestamp, timestamp, with_db,
};
use crate::services::ask_user_prompt_manager::normalizer::{
    redact_prompt_payload, trimmed_non_empty,
};
use crate::services::ask_user_prompt_manager::types::{
    AskUserPromptPayload, AskUserPromptRecord, AskUserPromptStatus, ASK_USER_PROMPT_NOT_FOUND_ERR,
};
use crate::services::realtime::{
    publish_ask_user_prompt_updated, resolve_conversation_scope, AskUserPromptRealtimePayload,
};
use chrono::{Duration, Utc};

async fn save(record: &AskUserPromptRecord) -> Result<(), String> {
    with_db(|pool|Box::pin(async move{sqlx::query("INSERT INTO ask_user_prompt_requests(id,conversation_id,conversation_turn_id,status,source,external_prompt_id,created_at,updated_at,expires_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(id) DO UPDATE SET conversation_id=EXCLUDED.conversation_id,conversation_turn_id=EXCLUDED.conversation_turn_id,status=EXCLUDED.status,source=EXCLUDED.source,external_prompt_id=EXCLUDED.external_prompt_id,updated_at=EXCLUDED.updated_at,expires_at=EXCLUDED.expires_at,data=EXCLUDED.data").bind(&record.id).bind(&record.conversation_id).bind(&record.conversation_turn_id).bind(record.status.as_str()).bind(&record.source).bind(&record.external_prompt_id).bind(timestamp(&record.created_at)?).bind(timestamp(&record.updated_at)?).bind(optional_timestamp(record.expires_at.as_deref())?).bind(json(record)?).execute(pool).await.map(|_|()).map_err(db_error)})).await
}

pub async fn create_ask_user_prompt_record(
    payload: &AskUserPromptPayload,
) -> Result<AskUserPromptRecord, String> {
    let id = trimmed_non_empty(&payload.prompt_id)
        .ok_or_else(|| "prompt_id is required".to_string())?
        .to_string();
    let conversation_id = trimmed_non_empty(&payload.conversation_id)
        .ok_or_else(|| "conversation_id is required".to_string())?
        .to_string();
    let conversation_turn_id = trimmed_non_empty(&payload.conversation_turn_id)
        .ok_or_else(|| "conversation_turn_id is required".to_string())?
        .to_string();
    let kind = trimmed_non_empty(&payload.kind)
        .ok_or_else(|| "kind is required".to_string())?
        .to_string();
    if let Some(existing) = super::read_ops::get_ask_user_prompt_record(&id).await? {
        return Ok(existing);
    }
    let now = crate::core::time::now_rfc3339();
    let record = AskUserPromptRecord {
        id,
        conversation_id,
        conversation_turn_id,
        tool_call_id: payload
            .tool_call_id
            .as_deref()
            .and_then(trimmed_non_empty)
            .map(str::to_string),
        kind,
        status: AskUserPromptStatus::Pending,
        prompt: redact_prompt_payload(payload),
        response: None,
        expires_at: Some(
            (Utc::now()
                + Duration::milliseconds(payload.timeout_ms.clamp(1_000, i32::MAX as u64) as i64))
            .to_rfc3339(),
        ),
        source: "chatos".to_string(),
        external_prompt_id: None,
        external_task_id: None,
        external_run_id: None,
        external_project_id: None,
        created_at: now.clone(),
        updated_at: now,
    };
    save(&record).await?;
    publish_ask_user_prompt_created(&record).await;
    Ok(record)
}

pub async fn upsert_external_ask_user_prompt_record(
    mut record: AskUserPromptRecord,
) -> Result<AskUserPromptRecord, String> {
    if let Some(existing) = super::read_ops::get_ask_user_prompt_record(&record.id).await? {
        if should_preserve_external_prompt_status(existing.status, record.status) {
            return Ok(existing);
        }
        record.created_at = existing.created_at;
        if record.expires_at.is_none() {
            record.expires_at = existing.expires_at;
        }
    }
    save(&record).await?;
    if record.status == AskUserPromptStatus::Pending {
        publish_ask_user_prompt_created(&record).await
    } else {
        publish_ask_user_prompt_resolved(&record).await
    }
    Ok(record)
}
fn should_preserve_external_prompt_status(
    existing: AskUserPromptStatus,
    incoming: AskUserPromptStatus,
) -> bool {
    existing != AskUserPromptStatus::Pending && existing != incoming
}

pub async fn update_ask_user_prompt_response(
    prompt_id: &str,
    status: AskUserPromptStatus,
    response: Option<serde_json::Value>,
) -> Result<AskUserPromptRecord, String> {
    let prompt_id =
        trimmed_non_empty(prompt_id).ok_or_else(|| "prompt_id is required".to_string())?;
    let updated = with_db(|pool| {
        Box::pin(async move {
            let mut tx = pool.begin().await.map_err(db_error)?;
            let Some(mut record): Option<AskUserPromptRecord> = decode_optional(
                sqlx::query_scalar(
                    "SELECT data FROM ask_user_prompt_requests WHERE id=$1 FOR UPDATE",
                )
                .bind(prompt_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db_error)?,
            )?
            else {
                return Err(ASK_USER_PROMPT_NOT_FOUND_ERR.to_string());
            };
            record.status = status;
            record.response = response;
            record.updated_at = crate::core::time::now_rfc3339();
            sqlx::query(
                "UPDATE ask_user_prompt_requests SET status=$1,updated_at=$2,data=$3 WHERE id=$4",
            )
            .bind(status.as_str())
            .bind(timestamp(&record.updated_at)?)
            .bind(json(&record)?)
            .bind(prompt_id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
            tx.commit().await.map_err(db_error)?;
            Ok(record)
        })
    })
    .await?;
    publish_ask_user_prompt_resolved(&updated).await;
    if let Ok(config) =
        chatos_mcp_management_sdk::McpManagementClientConfig::from_env("chatos").await
    {
        if let Ok(client) = chatos_mcp_management_sdk::McpManagementClient::new(config) {
            if let Err(error) = client.notify_waiting_user_resolved(&updated.id).await {
                tracing::warn!(prompt_id=updated.id,error=%error,"notify MCP Management Ask User resolution failed");
            }
        }
    }
    Ok(updated)
}

async fn publish_ask_user_prompt_created(record: &AskUserPromptRecord) {
    let Ok(scope) = resolve_conversation_scope(&record.conversation_id).await else {
        return;
    };
    let Some(user_id) = scope.user_id.as_deref() else {
        return;
    };
    let project_id = scope
        .project_id
        .as_deref()
        .and_then(trimmed_non_empty)
        .or_else(|| {
            record
                .external_project_id
                .as_deref()
                .and_then(trimmed_non_empty)
        });
    publish_ask_user_prompt_updated(
        user_id,
        AskUserPromptRealtimePayload {
            conversation_id: record.conversation_id.clone(),
            conversation_turn_id: Some(record.conversation_turn_id.clone()),
            project_id: project_id.map(str::to_string),
            prompt_id: record.id.clone(),
            action: "prompt_required".to_string(),
            status: Some(record.status.as_str().to_string()),
            tool_call_id: record.tool_call_id.clone(),
            prompt_kind: Some(record.kind.clone()),
            title: record
                .prompt
                .get("title")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            message: record
                .prompt
                .get("message")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            allow_cancel: record
                .prompt
                .get("allow_cancel")
                .and_then(serde_json::Value::as_bool),
            timeout_ms: record
                .prompt
                .get("timeout_ms")
                .and_then(serde_json::Value::as_u64),
            payload: record.prompt.get("payload").cloned(),
        },
    );
}
async fn publish_ask_user_prompt_resolved(record: &AskUserPromptRecord) {
    let Ok(scope) = resolve_conversation_scope(&record.conversation_id).await else {
        return;
    };
    let Some(user_id) = scope.user_id.as_deref() else {
        return;
    };
    let project_id = scope
        .project_id
        .as_deref()
        .and_then(trimmed_non_empty)
        .or_else(|| {
            record
                .external_project_id
                .as_deref()
                .and_then(trimmed_non_empty)
        });
    publish_ask_user_prompt_updated(
        user_id,
        AskUserPromptRealtimePayload {
            conversation_id: record.conversation_id.clone(),
            conversation_turn_id: Some(record.conversation_turn_id.clone()),
            project_id: project_id.map(str::to_string),
            prompt_id: record.id.clone(),
            action: "prompt_resolved".to_string(),
            status: Some(record.status.as_str().to_string()),
            tool_call_id: record.tool_call_id.clone(),
            prompt_kind: Some(record.kind.clone()),
            title: None,
            message: None,
            allow_cancel: None,
            timeout_ms: None,
            payload: None,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_prompt_upsert_does_not_regress_resolved_status_to_pending() {
        assert!(should_preserve_external_prompt_status(
            AskUserPromptStatus::Canceled,
            AskUserPromptStatus::Pending
        ));
        assert!(should_preserve_external_prompt_status(
            AskUserPromptStatus::Ok,
            AskUserPromptStatus::Pending
        ));
    }
    #[test]
    fn external_prompt_upsert_allows_pending_to_resolve() {
        assert!(!should_preserve_external_prompt_status(
            AskUserPromptStatus::Pending,
            AskUserPromptStatus::Canceled
        ));
    }
}
