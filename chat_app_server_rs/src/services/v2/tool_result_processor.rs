use serde_json::Value;

use crate::services::v2::message_manager::MessageManager;

pub struct ToolResultProcessor {
    message_manager: MessageManager,
}

impl ToolResultProcessor {
    pub fn new(message_manager: MessageManager) -> Self {
        Self { message_manager }
    }

    pub async fn process_tool_results(
        &self,
        results: &[Value],
        session_id: &str,
    ) -> Result<(), String> {
        for result in results {
            let tool_call_id = result
                .get("tool_call_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let content = result.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let metadata = result.get("metadata").cloned();
            if !tool_call_id.is_empty() {
                let _ = self
                    .message_manager
                    .save_tool_message(session_id, content, tool_call_id, metadata)
                    .await;
            }
        }
        Ok(())
    }
}
