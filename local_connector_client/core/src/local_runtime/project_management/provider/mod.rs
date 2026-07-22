// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod documents;
mod project;
mod requirement_support;
mod requirements;
mod task_support;
mod tasks;

#[cfg(test)]
mod tests;

use async_trait::async_trait;
use chatos_mcp::project_management_contract::tools;
use chatos_mcp_runtime::{
    BuiltinToolProvider, ToolCallContext, ToolStreamChunkCallback, PROJECT_MANAGEMENT_SERVER_NAME,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::local_runtime::storage::LocalDatabase;

#[derive(Clone)]
pub(crate) struct LocalProjectManagementProvider {
    database: LocalDatabase,
    owner_user_id: String,
    project_id: String,
}

impl LocalProjectManagementProvider {
    pub(crate) fn new(
        database: LocalDatabase,
        owner_user_id: impl Into<String>,
        project_id: impl Into<String>,
    ) -> Self {
        Self {
            database,
            owner_user_id: owner_user_id.into(),
            project_id: project_id.into(),
        }
    }
}

#[async_trait]
impl BuiltinToolProvider for LocalProjectManagementProvider {
    fn server_name(&self) -> &str {
        PROJECT_MANAGEMENT_SERVER_NAME
    }

    fn list_tools(&self) -> Vec<Value> {
        chatos_mcp::system_mcp_static_tools(
            chatos_plugin_management_sdk::SystemMcpKey::ProjectManagement,
        )
        .expect("Project Management must have a static system MCP catalog")
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        _context: ToolCallContext,
        _on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        let payload = match name {
            tools::GET_PROJECT_OVERVIEW => project::get_overview(self).await,
            tools::INITIALIZE_PROJECT => project::initialize(self, args).await,
            tools::GET_PROJECT_DEPENDENCY_GRAPH => project::get_dependency_graph(self).await,
            tools::LIST_REQUIREMENTS => requirements::list(self, args).await,
            tools::CREATE_REQUIREMENT => requirements::create(self, args).await,
            tools::UPDATE_REQUIREMENT => requirements::update(self, args).await,
            tools::DELETE_REQUIREMENT => requirements::archive(self, args).await,
            tools::SET_REQUIREMENT_DEPENDENCIES => requirements::set_dependencies(self, args).await,
            tools::LIST_REQUIREMENT_TECHNICAL_DOCUMENTS => documents::list(self, args).await,
            tools::GET_REQUIREMENT_TECHNICAL_DOCUMENT => documents::get(self, args).await,
            tools::UPSERT_REQUIREMENT_TECHNICAL_DOCUMENT => documents::upsert(self, args).await,
            tools::LIST_PROJECT_TASKS => tasks::list(self, args).await,
            tools::CREATE_PROJECT_TASK => tasks::create(self, args).await,
            tools::UPDATE_PROJECT_TASK => tasks::update(self, args).await,
            tools::DELETE_PROJECT_TASK => tasks::archive(self, args).await,
            tools::SET_PROJECT_TASK_DEPENDENCIES => tasks::set_dependencies(self, args).await,
            _ => Err(format!("unknown local project management tool: {name}")),
        }?;
        Ok(tool_result(payload))
    }
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| error.to_string())
}

fn tool_result(payload: Value) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
        }],
        "isError": false
    })
}

fn page<T>(mut records: Vec<T>, limit: Option<usize>, offset: Option<usize>) -> (Vec<T>, bool) {
    let offset = offset.unwrap_or_default().min(records.len());
    records.drain(0..offset);
    let limit = limit.unwrap_or(100).clamp(1, 500);
    let has_more = records.len() > limit;
    records.truncate(limit);
    (records, has_more)
}

fn normalized(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
