// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use async_trait::async_trait;
use chatos_builtin_tools::{
    AskUserDecision, AskUserPromptPayload, AskUserResponseSubmission, AskUserStore,
    AskUserStreamChunkCallback,
};
use chrono::{Duration as ChronoDuration, Utc};
use serde_json::json;

use crate::local_now_rfc3339;
use crate::local_runtime::storage::{LocalAskUserPromptRecord, LocalDatabase};

use super::registry::LocalAskUserPromptRegistry;

#[derive(Clone)]
pub(in crate::local_runtime) struct LocalAskUserStore {
    database: LocalDatabase,
    owner_user_id: String,
    registry: LocalAskUserPromptRegistry,
}

impl LocalAskUserStore {
    pub(in crate::local_runtime) fn new(
        database: LocalDatabase,
        owner_user_id: impl Into<String>,
        registry: LocalAskUserPromptRegistry,
    ) -> Self {
        Self {
            database,
            owner_user_id: owner_user_id.into(),
            registry,
        }
    }

    async fn wait_for_decision(
        &self,
        payload: &AskUserPromptPayload,
        notify: std::sync::Arc<tokio::sync::Notify>,
    ) -> Result<AskUserDecision, String> {
        let wait = async {
            loop {
                let record = self
                    .database
                    .get_ask_user_prompt(self.owner_user_id.as_str(), payload.prompt_id.as_str())
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "local Ask User prompt disappeared".to_string())?;
                if record.status != "pending" {
                    return decision_from_record(&record.status, record.response_json.as_deref());
                }
                tokio::select! {
                    _ = notify.notified() => {}
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {}
                }
            }
        };
        match tokio::time::timeout(Duration::from_millis(payload.timeout_ms.max(1)), wait).await {
            Ok(result) => result,
            Err(_) => self.timeout_prompt(payload.prompt_id.as_str()).await,
        }
    }

    async fn timeout_prompt(&self, prompt_id: &str) -> Result<AskUserDecision, String> {
        let response = AskUserResponseSubmission {
            status: "timeout".to_string(),
            values: None,
            selection: None,
            reason: Some("timeout".to_string()),
        };
        let response_json = serde_json::to_string(&response).map_err(|error| error.to_string())?;
        let _ = self
            .database
            .resolve_ask_user_prompt(
                self.owner_user_id.as_str(),
                prompt_id,
                "timeout",
                response_json.as_str(),
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(AskUserDecision {
            status: "timeout".to_string(),
            response,
        })
    }
}

#[async_trait]
impl AskUserStore for LocalAskUserStore {
    async fn execute_prompt(
        &self,
        payload: AskUserPromptPayload,
        on_stream_chunk: Option<AskUserStreamChunkCallback>,
    ) -> Result<AskUserDecision, String> {
        validate_payload(&payload, self.owner_user_id.as_str())?;
        let now = local_now_rfc3339();
        let expires_at = Utc::now()
            .checked_add_signed(ChronoDuration::milliseconds(
                payload.timeout_ms.min(i64::MAX as u64) as i64,
            ))
            .map(|value| value.to_rfc3339());
        let record = LocalAskUserPromptRecord {
            id: payload.prompt_id.clone(),
            session_id: payload.conversation_id.clone(),
            turn_id: payload.conversation_turn_id.clone(),
            owner_user_id: self.owner_user_id.clone(),
            tool_call_id: payload.tool_call_id.clone(),
            kind: payload.kind.clone(),
            status: "pending".to_string(),
            prompt_json: serde_json::to_string(&payload).map_err(|error| error.to_string())?,
            response_json: None,
            expires_at,
            created_at: now.clone(),
            updated_at: now,
        };
        let notify = self.registry.register(payload.prompt_id.as_str()).await;
        if let Err(error) = self.database.create_ask_user_prompt(&record).await {
            self.registry.remove(payload.prompt_id.as_str()).await;
            return Err(error.to_string());
        }
        emit_required(on_stream_chunk.as_ref(), &payload);
        let decision = self.wait_for_decision(&payload, notify).await;
        self.registry.remove(payload.prompt_id.as_str()).await;
        if let Ok(ref resolved) = decision {
            emit_resolved(
                on_stream_chunk.as_ref(),
                payload.prompt_id.as_str(),
                resolved.status.as_str(),
            );
        }
        decision
    }
}

fn validate_payload(payload: &AskUserPromptPayload, owner_user_id: &str) -> Result<(), String> {
    if owner_user_id.trim().is_empty() {
        return Err("local Ask User owner is missing".to_string());
    }
    if !payload.conversation_id.starts_with("lc_session_") {
        return Err("local Ask User requires a local session".to_string());
    }
    if !payload.conversation_turn_id.starts_with("lc_turn_") {
        return Err("local Ask User requires a local turn".to_string());
    }
    Ok(())
}

fn decision_from_record(
    status: &str,
    response_json: Option<&str>,
) -> Result<AskUserDecision, String> {
    let response = response_json
        .map(serde_json::from_str::<AskUserResponseSubmission>)
        .transpose()
        .map_err(|error| format!("parse local Ask User response: {error}"))?
        .unwrap_or_else(|| AskUserResponseSubmission {
            status: status.to_string(),
            values: None,
            selection: None,
            reason: None,
        });
    Ok(AskUserDecision {
        status: status.to_string(),
        response,
    })
}

fn emit_required(callback: Option<&AskUserStreamChunkCallback>, payload: &AskUserPromptPayload) {
    emit(
        callback,
        json!({ "event": "ask_user_prompt_required", "data": payload }),
    );
}

fn emit_resolved(callback: Option<&AskUserStreamChunkCallback>, prompt_id: &str, status: &str) {
    emit(
        callback,
        json!({
            "event": "ask_user_prompt_resolved",
            "data": { "prompt_id": prompt_id, "status": status }
        }),
    );
}

fn emit(callback: Option<&AskUserStreamChunkCallback>, value: serde_json::Value) {
    if let (Some(callback), Ok(serialized)) = (callback, serde_json::to_string(&value)) {
        callback(serialized);
    }
}
