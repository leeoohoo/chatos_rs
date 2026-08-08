// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl RuntimeInvocationStore {
    pub async fn mark_running(&self, invocation_id: &str) -> Result<bool, String> {
        self.transition_status(
            invocation_id,
            &[RuntimeInvocationStatus::Queued],
            RuntimeInvocationStatus::Running,
        )
        .await
    }

    pub async fn mark_waiting_for_user(&self, invocation_id: &str) -> Result<bool, String> {
        self.transition_status(
            invocation_id,
            &[RuntimeInvocationStatus::Running],
            RuntimeInvocationStatus::WaitingForUser,
        )
        .await
    }

    pub async fn complete(&self, invocation_id: &str, result: Value) -> Result<bool, String> {
        self.transition_terminal(
            invocation_id,
            &[
                RuntimeInvocationStatus::Running,
                RuntimeInvocationStatus::WaitingForUser,
            ],
            RuntimeInvocationStatus::Completed,
            Some(result),
            None,
            None,
        )
        .await
    }

    pub async fn fail(
        &self,
        invocation_id: &str,
        error_code: i32,
        error_message: impl Into<String>,
    ) -> Result<bool, String> {
        self.transition_terminal(
            invocation_id,
            &[
                RuntimeInvocationStatus::Queued,
                RuntimeInvocationStatus::Running,
                RuntimeInvocationStatus::WaitingForUser,
            ],
            RuntimeInvocationStatus::Failed,
            None,
            Some(error_code),
            Some(error_message.into()),
        )
        .await
    }

    pub async fn finish_cancellation(
        &self,
        invocation_id: &str,
        status: RuntimeInvocationStatus,
    ) -> Result<bool, String> {
        if !matches!(
            status,
            RuntimeInvocationStatus::Cancelled | RuntimeInvocationStatus::UnknownExecutionState
        ) {
            return Err("invalid terminal Runtime Invocation cancellation state".to_string());
        }
        self.transition_terminal(
            invocation_id,
            &[RuntimeInvocationStatus::CancelRequested],
            status,
            None,
            None,
            None,
        )
        .await
    }

    pub async fn cancel_without_start(&self, invocation_id: &str) -> Result<bool, String> {
        self.transition_terminal(
            invocation_id,
            &[
                RuntimeInvocationStatus::Queued,
                RuntimeInvocationStatus::CancelRequested,
            ],
            RuntimeInvocationStatus::Cancelled,
            None,
            None,
            None,
        )
        .await
    }

    pub async fn pending_result_events(
        &self,
        limit: i64,
    ) -> Result<Vec<PendingRuntimeInvocationResultEvent>, String> {
        let now = chrono::Utc::now().timestamp();
        let records = match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                invocations.retain(|_, record| record.expires_at_unix > now);
                invocations
                    .values()
                    .filter(|record| record.result_event_pending)
                    .take(limit.max(1) as usize)
                    .cloned()
                    .collect::<Vec<_>>()
            }
            RuntimeInvocationStoreBackend::Mongo(collection) => collection
                .find(
                    doc! {
                        "result_event_pending": true,
                        "expires_at": { "$gt": DateTime::now() },
                    },
                    FindOptions::builder()
                        .sort(doc! { "completed_at_unix_ms": 1 })
                        .limit(limit.max(1))
                        .build(),
                )
                .await
                .map_err(|error| {
                    format!("load pending Runtime Invocation result events failed: {error}")
                })?
                .try_collect::<Vec<_>>()
                .await
                .map_err(|error| {
                    format!("read pending Runtime Invocation result events failed: {error}")
                })?,
        };
        records
            .into_iter()
            .map(pending_result_event_from_record)
            .collect()
    }

    pub async fn acknowledge_result_event(
        &self,
        invocation_id: &str,
        event_id: &str,
    ) -> Result<bool, String> {
        match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                let Some(record) = invocations.get_mut(invocation_id) else {
                    return Ok(false);
                };
                if !record.result_event_pending
                    || record.result_event_id.as_deref() != Some(event_id)
                {
                    return Ok(false);
                }
                record.result_event_pending = false;
                Ok(true)
            }
            RuntimeInvocationStoreBackend::Mongo(collection) => collection
                .update_one(
                    doc! {
                        "_id": invocation_id,
                        "result_event_id": event_id,
                        "result_event_pending": true,
                    },
                    doc! { "$set": { "result_event_pending": false } },
                    None,
                )
                .await
                .map(|result| result.modified_count == 1)
                .map_err(|error| {
                    format!("acknowledge Runtime Invocation result event failed: {error}")
                }),
        }
    }

    pub async fn wait_for_result_event_signal(&self) {
        self.result_event_notify.notified().await;
    }

    async fn transition_status(
        &self,
        invocation_id: &str,
        from: &[RuntimeInvocationStatus],
        to: RuntimeInvocationStatus,
    ) -> Result<bool, String> {
        match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                let Some(record) = invocations.get_mut(invocation_id) else {
                    return Ok(false);
                };
                if !from.contains(&record.status) {
                    return Ok(false);
                }
                record.status = to;
                if to == RuntimeInvocationStatus::Running && record.started_at_unix_ms.is_none() {
                    record.started_at_unix_ms = Some(chrono::Utc::now().timestamp_millis());
                }
                Ok(true)
            }
            RuntimeInvocationStoreBackend::Mongo(collection) => {
                let mut set_doc = doc! { "status": to.as_str() };
                if to == RuntimeInvocationStatus::Running {
                    set_doc.insert(
                        "started_at_unix_ms",
                        bson::to_bson(&chrono::Utc::now().timestamp_millis())
                            .map_err(|error| error.to_string())?,
                    );
                }
                collection
                    .update_one(
                        doc! {
                            "_id": invocation_id,
                            "status": { "$in": from.iter().map(|status| status.as_str()).collect::<Vec<_>>() }
                        },
                        doc! { "$set": set_doc },
                        None,
                    )
                    .await
                    .map(|result| result.modified_count == 1)
                    .map_err(|error| format!("finish Runtime Invocation failed: {error}"))
            }
        }
    }

    pub(super) async fn transition_terminal(
        &self,
        invocation_id: &str,
        from: &[RuntimeInvocationStatus],
        to: RuntimeInvocationStatus,
        result: Option<Value>,
        error_code: Option<i32>,
        error_message: Option<String>,
    ) -> Result<bool, String> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let result_event_id = format!("mcp_result_{}", Uuid::new_v4().simple());
        let file_modification_outcome =
            terminal_file_modification_outcome(to, result.as_ref(), error_message.as_deref());
        let result = sanitize_terminal_result(result)?;
        let transitioned_record = match self.backend.as_ref() {
            RuntimeInvocationStoreBackend::Memory(invocations) => {
                let mut invocations = invocations.write().await;
                let Some(record) = invocations.get_mut(invocation_id) else {
                    return Ok(false);
                };
                if !from.contains(&record.status) {
                    return Ok(false);
                }
                record.status = to;
                record.completed_at_unix_ms = Some(now_ms);
                record.terminal_result = result;
                record.terminal_error_code = error_code;
                record.terminal_error_message = error_message;
                record.file_modification_outcome =
                    if is_file_modification_tool(record.original_tool_name.as_str()) {
                        file_modification_outcome
                    } else {
                        None
                    };
                if record.async_execution {
                    record.result_event_id = Some(result_event_id);
                    record.result_event_pending = true;
                }
                Ok(Some(record.clone()))
            }
            RuntimeInvocationStoreBackend::Mongo(collection) => {
                let mut set_doc = doc! {
                    "status": to.as_str(),
                    "completed_at_unix_ms": now_ms,
                };
                match result {
                    Some(value) => {
                        set_doc.insert(
                            "terminal_result",
                            bson::to_bson(&value).map_err(|error| error.to_string())?,
                        );
                    }
                    None => {
                        set_doc.insert("terminal_result", bson::Bson::Null);
                    }
                }
                match error_code {
                    Some(value) => {
                        set_doc.insert("terminal_error_code", value);
                    }
                    None => {
                        set_doc.insert("terminal_error_code", bson::Bson::Null);
                    }
                }
                match error_message {
                    Some(value) => {
                        set_doc.insert("terminal_error_message", value);
                    }
                    None => {
                        set_doc.insert("terminal_error_message", bson::Bson::Null);
                    }
                }
                let outcome_value = file_modification_outcome
                    .map(|outcome| bson::Bson::String(outcome.as_str().to_string()))
                    .unwrap_or(bson::Bson::Null);
                set_doc.insert(
                    "file_modification_outcome",
                    doc! {
                        "$cond": [
                            { "$in": ["$original_tool_name", ["edit_file", "apply_patch", "patch"]] },
                            outcome_value,
                            bson::Bson::Null,
                        ]
                    },
                );
                set_doc.insert(
                    "result_event_id",
                    doc! {
                        "$cond": [
                            "$async_execution",
                            result_event_id,
                            bson::Bson::Null,
                        ]
                    },
                );
                set_doc.insert("result_event_pending", "$async_execution");
                collection
                    .find_one_and_update(
                        doc! {
                            "_id": invocation_id,
                            "status": { "$in": from.iter().map(|status| status.as_str()).collect::<Vec<_>>() }
                        },
                        vec![doc! { "$set": set_doc }],
                        FindOneAndUpdateOptions::builder()
                            .return_document(ReturnDocument::After)
                            .build(),
                    )
                    .await
                    .map_err(|error| format!("finish Runtime Invocation failed: {error}"))
            }
        }?;
        if let Some(record) = transitioned_record.as_ref() {
            if let Err(error) = self.quota.release(record).await {
                self.diagnostics
                    .quota_release_failures
                    .fetch_add(1, Ordering::Relaxed);
                tracing::error!(
                    invocation_id = record.invocation_id.as_str(),
                    error = error.as_str(),
                    "release terminal Runtime Invocation quota reservation failed"
                );
            }
            self.result_event_notify.notify_one();
        }
        Ok(transitioned_record.is_some())
    }
}

fn pending_result_event_from_record(
    record: RuntimeInvocationRecord,
) -> Result<PendingRuntimeInvocationResultEvent, String> {
    let reply_to = record
        .result_reply_to
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "pending Runtime Invocation {} is missing result_reply_to",
                record.invocation_id
            )
        })?;
    let event_id = record.result_event_id.clone().ok_or_else(|| {
        format!(
            "pending Runtime Invocation {} is missing result_event_id",
            record.invocation_id
        )
    })?;
    let status = match record.status {
        RuntimeInvocationStatus::Completed => {
            chatos_mcp_management_sdk::RuntimeInvocationStatus::Completed
        }
        RuntimeInvocationStatus::Failed => {
            chatos_mcp_management_sdk::RuntimeInvocationStatus::Failed
        }
        RuntimeInvocationStatus::Cancelled => {
            chatos_mcp_management_sdk::RuntimeInvocationStatus::Cancelled
        }
        RuntimeInvocationStatus::UnknownExecutionState => {
            chatos_mcp_management_sdk::RuntimeInvocationStatus::UnknownExecutionState
        }
        other => {
            return Err(format!(
                "pending Runtime Invocation {} has non-terminal status {}",
                record.invocation_id,
                other.as_str()
            ))
        }
    };
    Ok(PendingRuntimeInvocationResultEvent {
        reply_to,
        event: chatos_mcp_management_sdk::RuntimeInvocationResultEvent {
            event_id,
            correlation_id: record.request_id_key,
            invocation_id: record.invocation_id,
            session_id: record.session_id,
            caller_service: record.caller_service,
            resource_id: record.resource_id,
            exposed_tool_name: record.exposed_tool_name,
            status,
            occurred_at_unix_ms: record
                .completed_at_unix_ms
                .unwrap_or(record.created_at_unix_ms),
            terminal_result: record.terminal_result,
            terminal_error_code: record.terminal_error_code,
            terminal_error_message: record.terminal_error_message,
        },
    })
}

fn terminal_file_modification_outcome(
    status: RuntimeInvocationStatus,
    result: Option<&Value>,
    error_message: Option<&str>,
) -> Option<FileModificationOutcome> {
    match status {
        RuntimeInvocationStatus::Completed => result
            .and_then(file_modification_outcome_from_result)
            .or(Some(FileModificationOutcome::Changed)),
        RuntimeInvocationStatus::Failed => error_message.map(classify_file_modification_error),
        RuntimeInvocationStatus::Queued
        | RuntimeInvocationStatus::Running
        | RuntimeInvocationStatus::WaitingForUser
        | RuntimeInvocationStatus::CancelRequested
        | RuntimeInvocationStatus::Cancelled
        | RuntimeInvocationStatus::UnknownExecutionState => None,
    }
}

fn file_modification_outcome_from_result(result: &Value) -> Option<FileModificationOutcome> {
    let payload = result.get("_structured_result").unwrap_or(result);
    if let Some(outcome) = payload.get("outcome").and_then(Value::as_str) {
        return match outcome {
            "changed" => Some(FileModificationOutcome::Changed),
            "already_applied" => Some(FileModificationOutcome::AlreadyApplied),
            "stale" | "stale_context" => Some(FileModificationOutcome::StaleContext),
            "expected_match" => Some(FileModificationOutcome::ExpectedMatch),
            "validation" => Some(FileModificationOutcome::Validation),
            "infrastructure" => Some(FileModificationOutcome::Infrastructure),
            _ => None,
        };
    }
    let result_payload = payload.get("result").unwrap_or(payload);
    if result_payload
        .get("already_applied")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Some(FileModificationOutcome::AlreadyApplied);
    }
    result_payload
        .get("changed")
        .and_then(Value::as_bool)
        .map(FileModificationOutcome::from_changed)
}

fn is_file_modification_tool(original_tool_name: &str) -> bool {
    matches!(original_tool_name, "edit_file" | "apply_patch" | "patch")
}

fn sanitize_terminal_result(result: Option<Value>) -> Result<Option<Value>, String> {
    const MAX_INLINE_RESULT_BYTES: usize = 256 * 1024;

    let Some(result) = result else {
        return Ok(None);
    };
    let encoded = serde_json::to_vec(&result).map_err(|error| error.to_string())?;
    if encoded.len() <= MAX_INLINE_RESULT_BYTES {
        return Ok(Some(result));
    }
    Ok(Some(serde_json::json!({
        "status": "result_truncated",
        "result_bytes": encoded.len(),
    })))
}
