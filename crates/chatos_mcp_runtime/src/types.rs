// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpHttpServer {
    pub name: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

impl McpHttpServer {
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            headers: None,
            timeout_ms: None,
        }
    }

    pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.headers = Some(headers);
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout_ms = Some(timeout.as_millis().min(u128::from(u64::MAX)) as u64);
        self
    }

    pub fn timeout_duration(&self) -> Option<Duration> {
        self.timeout_ms.map(Duration::from_millis)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpStdioServer {
    pub name: String,
    pub command: String,
    pub args: Option<Vec<String>>,
    pub cwd: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

impl McpStdioServer {
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            args: None,
            cwd: None,
            env: None,
            user_id: None,
        }
    }

    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = Some(args.into_iter().map(Into::into).collect());
        self
    }

    pub fn with_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn with_env(mut self, env: std::collections::HashMap<String, String>) -> Self {
        self.env = Some(env);
        self
    }

    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpBuiltinServer {
    pub name: String,
    pub kind: String,
    pub workspace_dir: String,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub remote_connection_id: Option<String>,
    pub contact_agent_id: Option<String>,
    pub auto_create_task: bool,
    pub allow_writes: bool,
    pub max_file_bytes: i64,
    pub max_write_bytes: i64,
    pub search_limit: usize,
}

#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub original_name: String,
    pub server_name: String,
    pub server_type: String,
    pub server_url: Option<String>,
    pub server_headers: Option<HashMap<String, String>>,
    pub server_timeout: Option<Duration>,
    pub server_config: Option<McpStdioServer>,
    pub tool_info: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub name: String,
    pub success: bool,
    pub is_error: bool,
    #[serde(default)]
    pub is_stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_turn_id: Option<String>,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
}

#[derive(Clone, Default)]
pub struct ToolCallerModelRuntime {
    pub model: String,
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub supports_responses: bool,
    pub supports_images: Option<bool>,
    pub thinking_level: Option<String>,
    pub temperature: Option<f64>,
    pub instructions: Option<String>,
    pub max_output_tokens: Option<i64>,
    pub request_body_limit_bytes: Option<usize>,
}

impl ToolCallerModelRuntime {
    pub fn openai_compatible(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
        provider: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            provider: provider.into(),
            ..Self::default()
        }
    }

    pub fn with_responses_support(mut self, supports_responses: bool) -> Self {
        self.supports_responses = supports_responses;
        self
    }

    pub fn with_images_support(mut self, supports_images: Option<bool>) -> Self {
        self.supports_images = supports_images;
        self
    }

    pub fn with_thinking_level(mut self, thinking_level: Option<String>) -> Self {
        self.thinking_level = thinking_level;
        self
    }

    pub fn with_temperature(mut self, temperature: Option<f64>) -> Self {
        self.temperature = temperature;
        self
    }

    pub fn with_instructions(mut self, instructions: Option<String>) -> Self {
        self.instructions = instructions;
        self
    }

    pub fn with_max_output_tokens(mut self, max_output_tokens: Option<i64>) -> Self {
        self.max_output_tokens = max_output_tokens;
        self
    }

    pub fn with_request_body_limit_bytes(
        mut self,
        request_body_limit_bytes: Option<usize>,
    ) -> Self {
        self.request_body_limit_bytes = request_body_limit_bytes;
        self
    }

    pub fn is_configured(&self) -> bool {
        !self.model.trim().is_empty()
            && !self.base_url.trim().is_empty()
            && !self.api_key.trim().is_empty()
    }
}

impl std::fmt::Debug for ToolCallerModelRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolCallerModelRuntime")
            .field("model", &self.model)
            .field("provider", &self.provider)
            .field("base_url", &self.base_url)
            .field("has_api_key", &(!self.api_key.trim().is_empty()))
            .field("supports_responses", &self.supports_responses)
            .field("supports_images", &self.supports_images)
            .field("thinking_level", &self.thinking_level)
            .field("temperature", &self.temperature)
            .field("has_instructions", &self.instructions.is_some())
            .field("max_output_tokens", &self.max_output_tokens)
            .field("request_body_limit_bytes", &self.request_body_limit_bytes)
            .finish()
    }
}

#[derive(Clone, Default)]
pub struct ToolCallContext {
    pub conversation_id: Option<String>,
    pub conversation_turn_id: Option<String>,
    pub caller_model: Option<String>,
    pub caller_model_runtime: Option<ToolCallerModelRuntime>,
    pub is_aborted: Option<ToolAbortCheckCallback>,
}

impl ToolCallContext {
    pub fn new(
        conversation_id: Option<String>,
        conversation_turn_id: Option<String>,
        caller_model: Option<String>,
    ) -> Self {
        Self {
            conversation_id,
            conversation_turn_id,
            caller_model,
            caller_model_runtime: None,
            is_aborted: None,
        }
    }

    pub fn with_caller_model_runtime(
        mut self,
        caller_model_runtime: Option<ToolCallerModelRuntime>,
    ) -> Self {
        if self.caller_model.is_none() {
            self.caller_model = caller_model_runtime
                .as_ref()
                .map(|runtime| runtime.model.clone())
                .filter(|model| !model.trim().is_empty());
        }
        self.caller_model_runtime = caller_model_runtime;
        self
    }

    pub fn with_abort_checker(mut self, is_aborted: ToolAbortCheckCallback) -> Self {
        self.is_aborted = Some(is_aborted);
        self
    }

    pub fn is_aborted(&self) -> bool {
        let Some(conversation_id) = self.conversation_id.as_deref() else {
            return false;
        };
        self.is_aborted
            .as_ref()
            .is_some_and(|callback| callback(conversation_id))
    }

    pub fn is_active(&self) -> bool {
        !self.is_aborted()
    }
}

impl std::fmt::Debug for ToolCallContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolCallContext")
            .field("conversation_id", &self.conversation_id)
            .field("conversation_turn_id", &self.conversation_turn_id)
            .field("caller_model", &self.caller_model)
            .field("caller_model_runtime", &self.caller_model_runtime)
            .field("has_abort_checker", &self.is_aborted.is_some())
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct ParsedToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

pub type ToolResultCallback = Arc<dyn Fn(&ToolResult) + Send + Sync>;
pub type ToolStreamChunkCallback = Arc<dyn Fn(String) + Send + Sync>;
pub type ToolAbortCheckCallback = Arc<dyn Fn(&str) -> bool + Send + Sync>;

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{McpHttpServer, McpStdioServer, ToolCallContext, ToolCallerModelRuntime};

    #[test]
    fn server_config_builders_fill_common_fields() {
        let http = McpHttpServer::new("remote", "http://127.0.0.1:9000/mcp");
        assert_eq!(http.name, "remote");
        assert_eq!(http.url, "http://127.0.0.1:9000/mcp");

        let stdio = McpStdioServer::new("local", "node")
            .with_args(["server.js", "--stdio"])
            .with_cwd("/tmp/work")
            .with_env(HashMap::from([("TOKEN".to_string(), "secret".to_string())]));
        assert_eq!(stdio.name, "local");
        assert_eq!(stdio.command, "node");
        assert_eq!(
            stdio.args.as_ref(),
            Some(&vec!["server.js".to_string(), "--stdio".to_string()])
        );
        assert_eq!(stdio.cwd.as_deref(), Some("/tmp/work"));
        assert_eq!(
            stdio.env.as_ref().and_then(|env| env.get("TOKEN")),
            Some(&"secret".to_string())
        );
    }

    #[test]
    fn tool_context_debug_redacts_caller_runtime_api_key() {
        let context = ToolCallContext::new(None, None, None).with_caller_model_runtime(Some(
            ToolCallerModelRuntime::openai_compatible(
                "https://example.com/v1",
                "secret-key",
                "gpt-vision",
                "gpt",
            ),
        ));

        let rendered = format!("{context:?}");
        assert!(rendered.contains("has_api_key"));
        assert!(rendered.contains("gpt-vision"));
        assert!(!rendered.contains("secret-key"));
    }
}
