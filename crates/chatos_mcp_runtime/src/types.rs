use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpHttpServer {
    pub name: String,
    pub url: String,
}

impl McpHttpServer {
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpStdioServer {
    pub name: String,
    pub command: String,
    pub args: Option<Vec<String>>,
    pub cwd: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
}

impl McpStdioServer {
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            args: None,
            cwd: None,
            env: None,
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
pub struct ToolCallContext {
    pub conversation_id: Option<String>,
    pub conversation_turn_id: Option<String>,
    pub caller_model: Option<String>,
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
            is_aborted: None,
        }
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

    use super::{McpHttpServer, McpStdioServer};

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
}
