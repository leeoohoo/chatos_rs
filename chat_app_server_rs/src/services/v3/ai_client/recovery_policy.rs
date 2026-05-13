mod completion_error;
mod request_error;
mod support;

use serde_json::Value;
use tracing::warn;

use super::{build_current_input_items, AiClient};
use crate::services::chatos_memory_engine;
use crate::services::chatos_sessions;

impl AiClient {
    pub(in crate::services::v3::ai_client) async fn build_stateless_from_raw_input(
        &self,
        session_id: Option<&String>,
        raw_input: &Value,
        force_text_content: bool,
        history_limit: i64,
        stable_prefix_mode: bool,
        include_tool_items: bool,
        prefixed_input_items: &[Value],
    ) -> Vec<Value> {
        let current_items = build_current_input_items(raw_input, force_text_content);
        self.build_stateless_items(
            session_id.cloned(),
            history_limit,
            stable_prefix_mode,
            force_text_content,
            prefixed_input_items,
            &current_items,
            include_tool_items,
        )
        .await
    }

    pub(in crate::services::v3::ai_client) async fn try_remote_active_summary_recovery(
        &mut self,
        session_id: Option<&String>,
        raw_input: &Value,
        force_text_content: bool,
        adaptive_history_limit: i64,
        stable_prefix_mode: bool,
        include_tool_items: bool,
        prefixed_input_items: &[Value],
        remote_active_summary_attempted: &mut bool,
        stateless_context_items: &mut Option<Vec<Value>>,
        input: &mut Value,
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

        let Ok(Some(session)) = chatos_sessions::get_session_by_id(session_id.as_str()).await else {
            return false;
        };
        let Some(status) = chatos_memory_engine::try_wait_for_chatos_active_summary_completion(
            &session,
            "context_overflow",
        )
        .await else {
            return false;
        };
        if status.failed || (!status.generated && !status.compacted) {
            warn!(
                "[AI_V3] remote active summary did not compact context: session_id={}, failed={}, generated={}, compacted={}",
                session_id,
                status.failed,
                status.generated,
                status.compacted
            );
            return false;
        }

        let stateless = self
            .build_stateless_from_raw_input(
                Some(&session_id),
                raw_input,
                force_text_content,
                adaptive_history_limit,
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
