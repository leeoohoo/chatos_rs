// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use chatos_plugin_management_sdk::{required_agent_prompt_vendor, SystemAgentKey};
use serde_json::json;
use uuid::Uuid;

use crate::local_runtime::capabilities::merge_system_prompts;
use crate::local_runtime::load_installed_agent_prompt;
use crate::local_runtime::memory::maybe_spawn_local_memory_review;
use crate::local_runtime::model::build_local_model_config;
use crate::local_runtime::storage::{
    AppendLocalRuntimeEventInput, BeginLocalTurnInput, BeginLocalTurnResult,
    CompleteLocalTurnInput, LocalDatabase, LocalMessageRecord, LocalTurnSnapshot,
};
use crate::model_configs::resolve_local_model_runtime;
use crate::LocalRuntime;

use super::control::LocalGuidanceLifecycleHook;
use super::events::LocalChatEventStream;
use super::model::run_text_turn;
use super::request::{normalize_optional, LocalChatSendRequest};
use super::tools::{prepare_local_chat_tools, LocalChatRecordWriter};

#[derive(Debug, Clone, Copy)]
pub(crate) enum LocalChatExecutionErrorKind {
    BadRequest,
    NotFound,
    Conflict,
    Cancelled,
    Model,
    Internal,
}

#[derive(Debug)]
pub(crate) struct LocalChatExecutionError {
    pub(crate) kind: LocalChatExecutionErrorKind,
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

impl LocalChatExecutionError {
    fn new(
        kind: LocalChatExecutionErrorKind,
        code: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            code,
            message: message.into(),
        }
    }

    fn internal(error: anyhow::Error) -> Self {
        Self::new(
            LocalChatExecutionErrorKind::Internal,
            "local_runtime_internal_error",
            error.to_string(),
        )
    }
}

#[derive(Debug)]
pub(crate) struct LocalChatResult {
    pub(crate) snapshot: LocalTurnSnapshot,
    pub(crate) process_messages: Vec<LocalMessageRecord>,
    pub(crate) reused: bool,
}

pub(crate) async fn execute_chat_turn(
    runtime: &LocalRuntime,
    owner_user_id: &str,
    request: LocalChatSendRequest,
) -> Result<LocalChatResult, LocalChatExecutionError> {
    let session_id = required(request.conversation_id, "conversation_id")?;
    if !session_id.starts_with("lc_session_") {
        return Err(LocalChatExecutionError::new(
            LocalChatExecutionErrorKind::BadRequest,
            "local_runtime_invalid_session",
            "Local chat requires a local runtime session",
        ));
    }
    let content = required(request.content, "content")?;
    if !request.attachments.is_empty() {
        return Err(LocalChatExecutionError::new(
            LocalChatExecutionErrorKind::BadRequest,
            "local_runtime_attachments_not_supported",
            "Local runtime attachments are not available yet",
        ));
    }

    let database = runtime
        .local_database()
        .map_err(LocalChatExecutionError::internal)?;
    let session = database
        .get_session(session_id.as_str(), owner_user_id)
        .await
        .map_err(LocalChatExecutionError::internal)?
        .ok_or_else(|| {
            LocalChatExecutionError::new(
                LocalChatExecutionErrorKind::NotFound,
                "local_runtime_session_not_found",
                "Local runtime session was not found",
            )
        })?;
    let settings = database
        .get_runtime_settings(owner_user_id, session_id.as_str())
        .await
        .map_err(LocalChatExecutionError::internal)?
        .ok_or_else(|| {
            LocalChatExecutionError::new(
                LocalChatExecutionErrorKind::NotFound,
                "local_runtime_settings_not_found",
                "Local runtime settings were not found",
            )
        })?;
    let project = database
        .get_project(session.project_id.as_str(), owner_user_id)
        .await
        .map_err(LocalChatExecutionError::internal)?
        .ok_or_else(|| {
            LocalChatExecutionError::new(
                LocalChatExecutionErrorKind::NotFound,
                "local_runtime_project_not_found",
                "Local runtime project was not found",
            )
        })?;
    let model_config_id = normalize_optional(request.model_config_id)
        .or(settings.selected_model_id.clone())
        .or(session.selected_model_id.clone())
        .ok_or_else(|| {
            LocalChatExecutionError::new(
                LocalChatExecutionErrorKind::Conflict,
                "local_runtime_model_required",
                "Select a local model configuration before sending a message",
            )
        })?;
    let resolved_model = {
        let state = runtime.state.read().await;
        resolve_local_model_runtime(&state, owner_user_id, model_config_id.as_str()).map_err(
            |error| {
                LocalChatExecutionError::new(
                    LocalChatExecutionErrorKind::Conflict,
                    "local_runtime_model_unavailable",
                    error.to_string(),
                )
            },
        )?
    };
    let effective_model_name = resolved_model.model.clone();
    let agent_key = if settings.plan_mode_enabled {
        SystemAgentKey::ChatosPlanningAgent
    } else {
        SystemAgentKey::ChatosConversationAgent
    };
    let prompt_vendor = required_agent_prompt_vendor(
        resolved_model.prompt_vendor.as_deref(),
        resolved_model.provider.as_str(),
    )
    .map_err(|error| {
        LocalChatExecutionError::new(
            LocalChatExecutionErrorKind::Conflict,
            "local_runtime_prompt_vendor_unsupported",
            error.to_string(),
        )
    })?;
    let installed_prompt = load_installed_agent_prompt(runtime, agent_key, prompt_vendor)
        .await
        .map_err(|error| {
            let message = error.to_string();
            let code = if message.contains("checksum") {
                "local_runtime_agent_prompt_invalid"
            } else {
                "local_runtime_agent_prompt_not_initialized"
            };
            LocalChatExecutionError::new(LocalChatExecutionErrorKind::Conflict, code, message)
        })?;
    let turn_id = normalize_optional(request.turn_id)
        .unwrap_or_else(|| format!("lc_turn_{}", Uuid::new_v4()));
    let idempotency_key =
        normalize_optional(request.idempotency_key).unwrap_or_else(|| turn_id.clone());
    let user_metadata = json!({
        "conversation_turn_id": turn_id,
        "model_config_id": model_config_id,
        "model": effective_model_name,
        "runtime_origin": "local_device",
        "agent_prompt_bundle_version": installed_prompt.bundle_version,
        "agent_prompt_revision": installed_prompt.revision,
        "agent_prompt_checksum": installed_prompt.checksum,
    });
    let begin = database
        .begin_turn(BeginLocalTurnInput {
            session_id: session_id.clone(),
            owner_user_id: owner_user_id.to_string(),
            turn_id: turn_id.clone(),
            idempotency_key,
            content,
            metadata_json: Some(user_metadata.to_string()),
        })
        .await
        .map_err(LocalChatExecutionError::internal)?;
    let started_snapshot = match begin {
        BeginLocalTurnResult::Started(snapshot) => snapshot,
        BeginLocalTurnResult::Existing(snapshot) if snapshot.turn.status == "completed" => {
            if snapshot.assistant_message.is_none() {
                return Err(LocalChatExecutionError::new(
                    LocalChatExecutionErrorKind::Internal,
                    "local_runtime_turn_incomplete",
                    "Completed local turn has no assistant message",
                ));
            }
            return Ok(LocalChatResult {
                process_messages: load_process_messages(database, owner_user_id, &snapshot)
                    .await
                    .map_err(LocalChatExecutionError::internal)?,
                snapshot,
                reused: true,
            });
        }
        BeginLocalTurnResult::Existing(snapshot) if snapshot.turn.status == "running" => {
            return Err(LocalChatExecutionError::new(
                LocalChatExecutionErrorKind::Conflict,
                "local_runtime_turn_in_progress",
                "This local chat turn is already running",
            ));
        }
        BeginLocalTurnResult::Existing(snapshot) => {
            return Err(LocalChatExecutionError::new(
                LocalChatExecutionErrorKind::Conflict,
                "local_runtime_turn_failed",
                snapshot
                    .turn
                    .error_message
                    .unwrap_or_else(|| "This local chat turn previously failed".to_string()),
            ));
        }
    };

    let active_turn = match runtime
        .turn_control
        .register(session_id.as_str(), turn_id.as_str())
    {
        Ok(active_turn) => active_turn,
        Err(error) => {
            let _ = database
                .fail_turn(
                    owner_user_id,
                    started_snapshot.turn.id.as_str(),
                    "local_runtime_turn_in_progress",
                    error.as_str(),
                )
                .await;
            return Err(LocalChatExecutionError::new(
                LocalChatExecutionErrorKind::Conflict,
                "local_runtime_turn_in_progress",
                error,
            ));
        }
    };
    let prepared_tools = match prepare_local_chat_tools(
        runtime,
        owner_user_id,
        turn_id.as_str(),
        &project,
        &settings,
    )
    .await
    {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = database
                .fail_turn(
                    owner_user_id,
                    started_snapshot.turn.id.as_str(),
                    "local_runtime_tools_unavailable",
                    error.as_str(),
                )
                .await;
            return Err(LocalChatExecutionError::new(
                LocalChatExecutionErrorKind::Conflict,
                "local_runtime_tools_unavailable",
                error,
            ));
        }
    };
    let requested_thinking_level = normalize_optional(request.ai_model_config.thinking_level)
        .or(settings.selected_thinking_level.clone());
    let model_config = build_local_model_config(
        resolved_model,
        merge_system_prompts(
            merge_system_prompts(
                Some(installed_prompt.content),
                normalize_optional(request.system_prompt),
            ),
            prepared_tools.capability_prompt,
        ),
        requested_thinking_level,
        request.ai_model_config.temperature,
        request
            .reasoning_enabled
            .unwrap_or(settings.reasoning_enabled),
        Some(prepared_tools.project_root.display().to_string()),
    );

    let memory_context = database
        .load_memory_context(
            owner_user_id,
            session_id.as_str(),
            settings.memory_recall_limit,
        )
        .await
        .map_err(LocalChatExecutionError::internal)?;
    let task_board = database
        .local_task_board_prompt(owner_user_id, session_id.as_str())
        .await
        .map_err(LocalChatExecutionError::internal)?;
    let record_writer = Arc::new(LocalChatRecordWriter::new(
        database.clone(),
        owner_user_id,
        session_id.as_str(),
        turn_id.as_str(),
    ));
    let guidance_hook = Arc::new(LocalGuidanceLifecycleHook::new(
        runtime.turn_control.clone(),
        database.clone(),
        owner_user_id,
        session_id.as_str(),
        turn_id.as_str(),
    ));
    let event_stream = LocalChatEventStream::start(
        database.clone(),
        owner_user_id,
        session_id.as_str(),
        turn_id.as_str(),
    );
    event_stream.publish("chat.phase", Some("status"), json!({ "phase": "running" }));
    let result = match run_text_turn(
        model_config,
        session_id.as_str(),
        turn_id.as_str(),
        memory_context.summary,
        memory_context.recalls,
        memory_context.messages,
        task_board,
        prepared_tools.executor,
        record_writer,
        active_turn.token(),
        guidance_hook,
        event_stream.callbacks(),
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            if active_turn.token().is_cancelled() || error == "aborted" {
                event_stream.publish(
                    "chat.cancelled",
                    Some("status"),
                    json!({ "reason": "cancel_requested" }),
                );
                let _ = event_stream.finish().await;
                return Err(cancelled_turn_error(database, owner_user_id, &started_snapshot).await);
            }
            event_stream.publish(
                "chat.failed",
                Some("status"),
                json!({ "code": "model_request_failed", "message": error.as_str() }),
            );
            let _ = event_stream.finish().await;
            let _ = database
                .fail_turn(
                    owner_user_id,
                    started_snapshot.turn.id.as_str(),
                    "model_request_failed",
                    error.as_str(),
                )
                .await;
            return Err(LocalChatExecutionError::new(
                LocalChatExecutionErrorKind::Model,
                "local_runtime_model_request_failed",
                error,
            ));
        }
    };
    if active_turn.token().is_cancelled() {
        event_stream.publish(
            "chat.cancelled",
            Some("status"),
            json!({ "reason": "cancel_requested" }),
        );
        let _ = event_stream.finish().await;
        return Err(cancelled_turn_error(database, owner_user_id, &started_snapshot).await);
    }
    let _ = event_stream.finish().await;
    let assistant_metadata = json!({
        "conversation_turn_id": turn_id,
        "model_config_id": model_config_id,
        "model": effective_model_name,
        "runtime_origin": "local_device",
        "response_status": "completed",
        "finish_reason": result.finish_reason,
        "usage": result.usage,
        "response_id": result.response_id,
    });
    let completed = match database
        .complete_turn(CompleteLocalTurnInput {
            turn_id,
            owner_user_id: owner_user_id.to_string(),
            content: result.content,
            reasoning: result.reasoning,
            tool_calls_json: result.tool_calls.map(|value| value.to_string()),
            metadata_json: Some(assistant_metadata.to_string()),
        })
        .await
    {
        Ok(completed) => completed,
        Err(error) => {
            let error_message = error.to_string();
            let _ = database
                .fail_turn(
                    owner_user_id,
                    started_snapshot.turn.id.as_str(),
                    "local_runtime_internal_error",
                    error_message.as_str(),
                )
                .await;
            append_terminal_event(
                database,
                owner_user_id,
                session_id.as_str(),
                started_snapshot.turn.id.as_str(),
                "chat.failed",
                Some("status"),
                json!({ "code": "local_runtime_internal_error", "message": error_message }),
            )
            .await;
            return Err(LocalChatExecutionError::internal(error));
        }
    };
    append_terminal_event(
        database,
        owner_user_id,
        session_id.as_str(),
        completed.turn.id.as_str(),
        "chat.completed",
        Some("status"),
        json!({ "assistant_message_id": completed.assistant_message.as_ref().map(|message| &message.id) }),
    )
    .await;
    let process_messages = load_process_messages(database, owner_user_id, &completed)
        .await
        .map_err(LocalChatExecutionError::internal)?;
    drop(active_turn);
    let _ = maybe_spawn_local_memory_review(runtime, owner_user_id, session_id.as_str()).await;
    Ok(LocalChatResult {
        snapshot: completed,
        process_messages,
        reused: false,
    })
}

async fn append_terminal_event(
    database: &LocalDatabase,
    owner_user_id: &str,
    session_id: &str,
    turn_id: &str,
    event_name: &'static str,
    stream_type: Option<&'static str>,
    payload: serde_json::Value,
) {
    let _ = database
        .append_runtime_event(AppendLocalRuntimeEventInput {
            owner_user_id: owner_user_id.to_string(),
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            event_name: event_name.to_string(),
            stream_type: stream_type.map(ToOwned::to_owned),
            payload,
        })
        .await;
}

async fn cancelled_turn_error(
    database: &LocalDatabase,
    owner_user_id: &str,
    snapshot: &LocalTurnSnapshot,
) -> LocalChatExecutionError {
    let _ = database
        .cancel_turn(
            owner_user_id,
            snapshot.turn.id.as_str(),
            "Local chat turn was cancelled",
        )
        .await;
    LocalChatExecutionError::new(
        LocalChatExecutionErrorKind::Cancelled,
        "local_runtime_turn_cancelled",
        "Local chat turn was cancelled",
    )
}

async fn load_process_messages(
    database: &LocalDatabase,
    owner_user_id: &str,
    snapshot: &LocalTurnSnapshot,
) -> anyhow::Result<Vec<LocalMessageRecord>> {
    let final_assistant_id = snapshot
        .assistant_message
        .as_ref()
        .map(|message| message.id.as_str());
    Ok(database
        .list_turn_messages(owner_user_id, snapshot.turn.id.as_str())
        .await?
        .into_iter()
        .filter(|message| {
            message.id != snapshot.user_message.id
                && final_assistant_id != Some(message.id.as_str())
        })
        .collect())
}

fn required(value: String, field: &'static str) -> Result<String, LocalChatExecutionError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(LocalChatExecutionError::new(
            LocalChatExecutionErrorKind::BadRequest,
            "local_runtime_invalid_request",
            format!("{field} is required"),
        ));
    }
    Ok(value)
}
