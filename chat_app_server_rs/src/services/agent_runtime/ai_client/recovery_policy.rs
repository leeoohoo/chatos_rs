// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod completion_error;
mod request_error;
mod support;

use serde_json::{json, Value};
use tracing::warn;

use super::{build_current_input_items, AiClient};
use crate::services::ai_client_common::AiClientCallbacks;
use crate::services::chatos_memory_engine;
use crate::services::chatos_sessions;

impl AiClient {
    pub(in crate::services::agent_runtime::ai_client) async fn build_stateless_from_raw_input(
        &self,
        session_id: Option<&String>,
        raw_input: &Value,
        force_text_content: bool,
        stable_prefix_mode: bool,
        include_tool_items: bool,
        prefixed_input_items: &[Value],
    ) -> Vec<Value> {
        let current_items = build_current_input_items(raw_input, force_text_content);
        self.build_stateless_items(
            session_id.cloned(),
            stable_prefix_mode,
            force_text_content,
            prefixed_input_items,
            &current_items,
            include_tool_items,
        )
        .await
    }

    pub(in crate::services::agent_runtime::ai_client) async fn try_remote_active_summary_recovery(
        &mut self,
        session_id: Option<&String>,
        raw_input: &Value,
        force_text_content: bool,
        stable_prefix_mode: bool,
        include_tool_items: bool,
        prefixed_input_items: &[Value],
        remote_active_summary_attempted: &mut bool,
        stateless_context_items: &mut Option<Vec<Value>>,
        input: &mut Value,
        callbacks: &AiClientCallbacks,
    ) -> bool {
        let Some(session_id) = session_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        else {
            return false;
        };
        if *remote_active_summary_attempted {
            return false;
        }
        *remote_active_summary_attempted = true;

        let Ok(Some(session)) = chatos_sessions::get_session_by_id(session_id.as_str()).await
        else {
            return false;
        };
        let Some(initial_status) =
            chatos_memory_engine::try_start_chatos_active_summary(&session, "context_overflow")
                .await
        else {
            return false;
        };
        notify_active_summary_progress(
            callbacks,
            "正在自动压缩上下文，压缩完成后将继续当前请求。",
            &session,
            &initial_status,
        );
        let status = match chatos_memory_engine::wait_for_existing_chatos_active_summary_completion(
            &session,
            initial_status,
        )
        .await
        {
            Ok(status) => status,
            Err(err) => {
                warn!(
                    "[Agent Runtime] remote active summary wait failed: session_id={}, error={}",
                    session_id, err
                );
                return false;
            }
        };
        if status.failed || (!status.generated && !status.compacted) {
            warn!(
                "[Agent Runtime] remote active summary did not compact context: session_id={}, failed={}, generated={}, compacted={}",
                session_id, status.failed, status.generated, status.compacted
            );
            return false;
        }
        notify_active_summary_progress(
            callbacks,
            "上下文压缩完成，正在继续当前请求。",
            &session,
            &status,
        );

        let stateless = self
            .build_stateless_from_raw_input(
                Some(&session_id),
                raw_input,
                force_text_content,
                stable_prefix_mode,
                include_tool_items,
                prefixed_input_items,
            )
            .await;
        if stateless.is_empty() {
            return false;
        }
        *stateless_context_items = Some(stateless.clone());
        *input = Value::Array(stateless);
        true
    }
}

fn notify_active_summary_progress(
    callbacks: &AiClientCallbacks,
    message: &str,
    session: &crate::models::session::Session,
    status: &memory_engine_sdk::RunThreadActiveSummaryResponse,
) {
    if let Some(cb) = &callbacks.on_thinking {
        cb(message.to_string());
    }
    if let Some(cb) = &callbacks.on_context_summarized_start {
        cb(json!({
            "kind": "active_summary_progress",
            "message": message,
            "session_id": session.id,
            "job_run_id": status.job_run_id,
            "pending_before_count": status.pending_before_count,
            "pending_after_count": status.pending_after_count,
            "running": status.running,
            "completed": status.completed,
            "failed": status.failed,
            "generated": status.generated,
            "compacted": status.compacted
        }));
    }
}
