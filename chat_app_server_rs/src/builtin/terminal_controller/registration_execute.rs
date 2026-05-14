use serde_json::json;

use crate::core::tool_registry::async_text_tool_handler_with_optional_string;

use super::actions::actions_execute::execute_command_with_context;
use super::context::required_trimmed_string;
use super::{BoundContext, TerminalControllerService};

impl TerminalControllerService {
    pub(super) fn register_execute_command(&mut self, bound: BoundContext, root_for_desc: &str) {
        self.register_tool(
            "execute_command",
            &format!(
                "LOCAL ONLY: execute shell command in the local project terminal with path switching. Relative path is resolved from project root ({root_for_desc}). This tool does NOT execute on remote SSH hosts. For remote servers, use builtin_remote_connection_controller.run_command instead."
            ),
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Local directory path under project root."
                    },
                    "common": {
                        "type": "string",
                        "description": "Local shell command to run."
                    },
                    "command": {
                        "type": "string",
                        "description": "Alias of common. Local shell command to run."
                    },
                    "background": {
                        "type": "boolean",
                        "default": false,
                        "description": "When true, return immediately and use process_poll/process_wait to track progress."
                    }
                },
                "additionalProperties": false,
                "required": ["path"]
            }),
            async_text_tool_handler_with_optional_string(move |args, _conversation_id| {
                let path = required_trimmed_string(&args, "path")?;
                let command = args
                    .get("common")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_string())
                    .or_else(|| {
                        args.get("command")
                            .and_then(|value| value.as_str())
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(|value| value.to_string())
                    })
                    .ok_or_else(|| "common is required".to_string())?;
                let background = args
                    .get("background")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                let ctx = bound.clone();
                Ok(async move {
                    execute_command_with_context(ctx, path.as_str(), command.as_str(), background)
                        .await
                })
            }),
        );
    }
}
