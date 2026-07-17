// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_builtin_tools::{
    AskUserOptions, AskUserService, AskUserStoreRef, ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
};
use chatos_mcp_runtime::{
    BuiltinToolProvider, ToolCallContext, ToolStreamChunkCallback, ASK_USER_SERVER_NAME,
};
use serde_json::Value;

use crate::local_runtime::storage::LocalDatabase;

use super::registry::LocalAskUserPromptRegistry;
use super::store::LocalAskUserStore;

#[derive(Clone)]
pub(crate) struct LocalAskUserProvider {
    service: AskUserService,
}

impl LocalAskUserProvider {
    pub(crate) fn new(
        database: LocalDatabase,
        owner_user_id: impl Into<String>,
        registry: LocalAskUserPromptRegistry,
    ) -> Self {
        let store = LocalAskUserStore::new(database, owner_user_id, registry);
        let service = AskUserService::new(AskUserOptions {
            server_name: ASK_USER_SERVER_NAME.to_string(),
            prompt_timeout_ms: ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
            store: AskUserStoreRef::new(Arc::new(store)),
        })
        .expect("local Ask User service configuration is valid");
        Self { service }
    }
}

#[async_trait]
impl BuiltinToolProvider for LocalAskUserProvider {
    fn server_name(&self) -> &str {
        ASK_USER_SERVER_NAME
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
