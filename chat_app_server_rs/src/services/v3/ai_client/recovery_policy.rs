mod completion_error;
mod request_error;
mod support;

use serde_json::Value;

use super::{build_current_input_items, AiClient};

impl AiClient {
    async fn build_stateless_from_raw_input(
        &self,
        session_id: Option<&String>,
        raw_input: &Value,
        force_text_content: bool,
        history_limit: i64,
        stable_prefix_mode: bool,
        include_tool_items: bool,
    ) -> Vec<Value> {
        let current_items = build_current_input_items(raw_input, force_text_content);
        self.build_stateless_items(
            session_id.cloned(),
            history_limit,
            stable_prefix_mode,
            force_text_content,
            &current_items,
            include_tool_items,
        )
        .await
    }
}
