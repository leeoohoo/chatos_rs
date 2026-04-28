use serde_json::json;

use crate::core::tool_registry::async_text_tool_handler_with_optional_string;

use super::actions::actions_query::get_recent_logs_with_context;
use super::{
    BoundContext, RECENT_LOGS_MAX_PER_TERMINAL_LIMIT, RECENT_LOGS_MAX_TERMINAL_LIMIT,
    TerminalControllerService,
};

impl TerminalControllerService {
    pub(super) fn register_get_recent_logs(&mut self, bound: BoundContext) {
        self.register_tool(
            "get_recent_logs",
            "Get recent logs grouped by terminal for current agent project.",
            json!({
                "type": "object",
                "properties": {
                    "per_terminal_limit": { "type": "integer", "minimum": 1, "maximum": 50 },
                    "terminal_limit": { "type": "integer", "minimum": 1, "maximum": 20 }
                },
                "additionalProperties": false
            }),
            async_text_tool_handler_with_optional_string(move |args, _conversation_id| {
                let per_terminal_limit = args
                    .get("per_terminal_limit")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(10)
                    .clamp(1, RECENT_LOGS_MAX_PER_TERMINAL_LIMIT);
                let terminal_limit = args
                    .get("terminal_limit")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(20)
                    .clamp(1, RECENT_LOGS_MAX_TERMINAL_LIMIT) as usize;
                let ctx = bound.clone();
                Ok(async move {
                    get_recent_logs_with_context(ctx, per_terminal_limit, terminal_limit).await
                })
            }),
        );
    }
}
