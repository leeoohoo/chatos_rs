// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_runtime::ToolCallerModelRuntime;
use serde_json::Value;

use crate::core::mcp_tools::{ToolInfo, ToolResult, ToolResultCallback};
use crate::services::mcp_execution_core::McpExecutorCore;
use crate::services::mcp_loader::{McpBuiltinServer, McpHttpServer, McpStdioServer};
use crate::services::shared_mcp_runtime::{build_shared_mcp_executor, chatos_tool_info};

#[derive(Clone)]
pub(crate) struct SharedMcpToolExecute {
    core: McpExecutorCore,
    shared_core: Option<chatos_mcp_runtime::McpExecutor>,
    shared_tool_metadata: Option<HashMap<String, ToolInfo>>,
    mcp_servers: Vec<McpHttpServer>,
    stdio_mcp_servers: Vec<McpStdioServer>,
    builtin_mcp_servers: Vec<McpBuiltinServer>,
}

impl SharedMcpToolExecute {
    pub(crate) fn new(
        mcp_servers: Vec<McpHttpServer>,
        stdio_mcp_servers: Vec<McpStdioServer>,
        builtin_mcp_servers: Vec<McpBuiltinServer>,
    ) -> Self {
        Self {
            core: McpExecutorCore::new(
                mcp_servers.clone(),
                stdio_mcp_servers.clone(),
                builtin_mcp_servers.clone(),
            ),
            shared_core: None,
            shared_tool_metadata: None,
            mcp_servers,
            stdio_mcp_servers,
            builtin_mcp_servers,
        }
    }

    pub(crate) async fn init(&mut self) -> Result<(), String> {
        self.build_tools().await
    }

    pub(crate) async fn build_tools(&mut self) -> Result<(), String> {
        let mut shared = build_shared_mcp_executor(
            self.mcp_servers.clone(),
            self.stdio_mcp_servers.clone(),
            self.builtin_mcp_servers.clone(),
        );
        shared.init().await?;
        self.shared_tool_metadata = Some(chatos_tool_metadata(&shared));
        self.shared_core = Some(shared);
        Ok(())
    }

    pub(crate) fn build_builtin_only(&mut self) -> Result<(), String> {
        self.core.build_builtin_only()?;
        let mut shared = build_shared_mcp_executor(
            self.mcp_servers.clone(),
            self.stdio_mcp_servers.clone(),
            self.builtin_mcp_servers.clone(),
        );
        shared.init_builtin_only()?;
        self.shared_tool_metadata = Some(chatos_tool_metadata(&shared));
        self.shared_core = Some(shared);
        Ok(())
    }

    pub(crate) fn available_tools(&self) -> Vec<Value> {
        if let Some(shared) = &self.shared_core {
            return shared.available_tools();
        }
        self.core.available_tools()
    }

    pub(crate) fn unavailable_tools(&self) -> Vec<Value> {
        if let Some(shared) = &self.shared_core {
            return shared.unavailable_tools();
        }
        self.core.unavailable_tools()
    }

    pub(crate) fn tool_metadata(&self) -> &HashMap<String, ToolInfo> {
        if let Some(metadata) = &self.shared_tool_metadata {
            return metadata;
        }
        self.core.tool_metadata()
    }

    #[cfg(test)]
    pub(crate) fn tool_metadata_mut(&mut self) -> &mut HashMap<String, ToolInfo> {
        self.shared_core = None;
        self.shared_tool_metadata = None;
        self.core.tool_metadata_mut()
    }

    pub(crate) fn codex_gateway_request_tools(&self) -> Vec<Value> {
        if let Some(shared) = &self.shared_core {
            return shared.codex_gateway_request_tools();
        }
        self.core.codex_gateway_request_tools()
    }

    pub(crate) async fn execute_tools_stream(
        &self,
        tool_calls: &[Value],
        session_id: Option<&str>,
        conversation_turn_id: Option<&str>,
        caller_model: Option<&str>,
        caller_model_runtime: Option<&ToolCallerModelRuntime>,
        on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult> {
        if let Some(shared) = &self.shared_core {
            return shared
                .execute_tools_stream(
                    tool_calls,
                    chatos_mcp_runtime::ToolCallContext::new(
                        session_id.map(ToOwned::to_owned),
                        conversation_turn_id.map(ToOwned::to_owned),
                        caller_model.map(ToOwned::to_owned),
                    )
                    .with_caller_model_runtime(caller_model_runtime.cloned())
                    .with_abort_checker(std::sync::Arc::new(|session_id| {
                        crate::utils::abort_registry::is_aborted(session_id)
                    })),
                    on_tool_result,
                )
                .await;
        }

        self.core
            .execute_tools_stream(
                tool_calls,
                session_id,
                conversation_turn_id,
                caller_model,
                caller_model_runtime,
                on_tool_result,
            )
            .await
    }

    #[cfg(test)]
    pub(crate) fn should_parallelize_tool_batch(&self, tool_calls: &[Value]) -> bool {
        if let Some(shared) = &self.shared_core {
            return shared.should_parallelize_tool_batch(tool_calls);
        }
        self.core.should_parallelize_tool_batch(tool_calls)
    }
}

fn chatos_tool_metadata(shared: &chatos_mcp_runtime::McpExecutor) -> HashMap<String, ToolInfo> {
    shared
        .tool_metadata()
        .iter()
        .map(|(name, info)| (name.clone(), chatos_tool_info(info)))
        .collect()
}

#[cfg(test)]
pub(crate) mod test_support {
    use serde_json::json;

    use super::SharedMcpToolExecute;
    use crate::services::builtin_mcp::BuiltinMcpKind;
    use crate::services::mcp_loader::{McpBuiltinServer, McpHttpServer, McpStdioServer};

    pub(crate) async fn build_skill_reader_executor(builtin_only: bool) -> SharedMcpToolExecute {
        let mut exec = SharedMcpToolExecute::new(
            Vec::<McpHttpServer>::new(),
            Vec::<McpStdioServer>::new(),
            vec![McpBuiltinServer {
                name: "memory_skill_reader".to_string(),
                kind: BuiltinMcpKind::MemorySkillReader,
                workspace_dir: String::new(),
                user_id: Some("user_1".to_string()),
                project_id: Some("project_1".to_string()),
                remote_connection_id: None,
                contact_agent_id: Some("agent_1".to_string()),
                auto_create_task: false,
                allow_writes: false,
                max_file_bytes: 0,
                max_write_bytes: 0,
                search_limit: 0,
            }],
        );
        if builtin_only {
            exec.build_builtin_only().expect("init builtin tools");
        } else {
            exec.init().await.expect("init builtin tools");
        }
        exec
    }

    pub(crate) fn prefixed_tool_name(exec: &SharedMcpToolExecute) -> String {
        exec.tool_metadata()
            .keys()
            .find(|name| name.starts_with("memory_skill_reader_"))
            .expect("prefixed tool name")
            .to_string()
    }

    pub(crate) fn assert_parallel_policy_allows_read_only_safe_batch(exec: &SharedMcpToolExecute) {
        let prefixed_tool_name = prefixed_tool_name(exec);
        let tool_calls = vec![
            json!({
                "id": "call_1",
                "function": {
                    "name": prefixed_tool_name,
                    "arguments": "{\"skill_ref\":\"SK1\"}"
                }
            }),
            json!({
                "id": "call_2",
                "function": {
                    "name": prefixed_tool_name,
                    "arguments": "{\"skill_ref\":\"SK2\"}"
                }
            }),
        ];
        assert!(exec.should_parallelize_tool_batch(tool_calls.as_slice()));
    }

    pub(crate) fn assert_parallel_policy_allows_web_research_batch(
        exec: &mut SharedMcpToolExecute,
    ) {
        let prefixed_tool_name = prefixed_tool_name(exec);
        exec.tool_metadata_mut()
            .get_mut(prefixed_tool_name.as_str())
            .expect("tool metadata")
            .original_name = "web_research".to_string();
        let tool_calls = vec![
            json!({
                "id": "call_1",
                "function": {
                    "name": prefixed_tool_name.clone(),
                    "arguments": "{\"query\":\"hermes agent browser automation\"}"
                }
            }),
            json!({
                "id": "call_2",
                "function": {
                    "name": prefixed_tool_name,
                    "arguments": "{\"query\":\"chatos web research mcp\"}"
                }
            }),
        ];
        assert!(exec.should_parallelize_tool_batch(tool_calls.as_slice()));
    }

    pub(crate) fn assert_parallel_policy_rejects_invalid_argument_json(
        exec: &SharedMcpToolExecute,
    ) {
        let prefixed_tool_name = prefixed_tool_name(exec);
        let tool_calls = vec![
            json!({
                "id": "call_1",
                "function": {
                    "name": prefixed_tool_name,
                    "arguments": "{\"skill_ref\":\"SK1\"}"
                }
            }),
            json!({
                "id": "call_2",
                "function": {
                    "name": prefixed_tool_name,
                    "arguments": "{\"skill_ref\":"
                }
            }),
        ];
        assert!(!exec.should_parallelize_tool_batch(tool_calls.as_slice()));
    }

    pub(crate) fn assert_parallel_policy_rejects_missing_required_path_scope(
        exec: &mut SharedMcpToolExecute,
    ) {
        let prefixed_tool_name = prefixed_tool_name(exec);
        exec.tool_metadata_mut()
            .get_mut(prefixed_tool_name.as_str())
            .expect("tool metadata")
            .original_name = "read_file_raw".to_string();
        let tool_calls = vec![
            json!({
                "id": "call_1",
                "function": {
                    "name": prefixed_tool_name.clone(),
                    "arguments": "{\"path\":\"src/lib.rs\"}"
                }
            }),
            json!({
                "id": "call_2",
                "function": {
                    "name": prefixed_tool_name,
                    "arguments": "{}"
                }
            }),
        ];
        assert!(!exec.should_parallelize_tool_batch(tool_calls.as_slice()));
    }
}
