// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_mcp::{
    TaskManagerOptions, TaskManagerService, TaskManagerStoreRef, REVIEW_TIMEOUT_MS_DEFAULT,
};
use chatos_mcp_runtime::{
    BuiltinToolProvider, ToolCallContext, ToolStreamChunkCallback, TASK_MANAGER_SERVER_NAME,
};
use serde_json::Value;

use crate::local_runtime::storage::LocalDatabase;
use crate::local_runtime::LocalAskUserPromptRegistry;

use super::store::LocalTaskManagerStore;

#[derive(Clone)]
pub(crate) struct LocalTaskManagerProvider {
    service: TaskManagerService,
}

impl LocalTaskManagerProvider {
    pub(crate) fn new(
        database: LocalDatabase,
        owner_user_id: impl Into<String>,
        auto_create_task: bool,
        ask_user_prompts: LocalAskUserPromptRegistry,
    ) -> Self {
        Self::build(
            database,
            owner_user_id,
            auto_create_task,
            ask_user_prompts,
            None,
        )
    }

    pub(crate) fn for_task_run(
        database: LocalDatabase,
        owner_user_id: impl Into<String>,
        auto_create_task: bool,
        ask_user_prompts: LocalAskUserPromptRegistry,
        task_session_id: impl Into<String>,
    ) -> Self {
        Self::build(
            database,
            owner_user_id,
            auto_create_task,
            ask_user_prompts,
            Some(task_session_id.into()),
        )
    }

    fn build(
        database: LocalDatabase,
        owner_user_id: impl Into<String>,
        auto_create_task: bool,
        ask_user_prompts: LocalAskUserPromptRegistry,
        task_session_id: Option<String>,
    ) -> Self {
        let lifecycle_tools_enabled = task_session_id.is_some();
        let store =
            LocalTaskManagerStore::new(database, owner_user_id, ask_user_prompts, task_session_id);
        let service = TaskManagerService::new(TaskManagerOptions {
            server_name: TASK_MANAGER_SERVER_NAME.to_string(),
            review_timeout_ms: REVIEW_TIMEOUT_MS_DEFAULT,
            auto_create_task,
            expose_context_ids: true,
            default_current_turn_only: lifecycle_tools_enabled,
            lifecycle_tools_enabled,
            store: TaskManagerStoreRef::new(Arc::new(store)),
        })
        .expect("local task manager service configuration is valid");
        Self { service }
    }
}

#[async_trait]
impl BuiltinToolProvider for LocalTaskManagerProvider {
    fn server_name(&self) -> &str {
        TASK_MANAGER_SERVER_NAME
    }

    fn list_tools(&self) -> Vec<Value> {
        self.service.list_tools()
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        context: ToolCallContext,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        self.service.call_tool(
            name,
            args,
            context.conversation_id.as_deref(),
            context.conversation_turn_id.as_deref(),
            on_stream_chunk,
        )
    }
}
