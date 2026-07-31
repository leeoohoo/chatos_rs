// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use serde_json::Value;

use chatos_mcp::AgentBuilderStore;

use crate::models::chatos_agent_types::{CreateChatosAgentRequest, UpdateChatosAgentRequest};
use crate::services::chatos_agents;

#[derive(Debug, Clone, Default)]
pub struct ChatosAgentBuilderStore;

#[async_trait]
impl AgentBuilderStore for ChatosAgentBuilderStore {
    async fn create_agent(&self, request: Value) -> Result<Value, String> {
        let payload: CreateChatosAgentRequest =
            serde_json::from_value(request).map_err(|err| err.to_string())?;
        let created = chatos_agents::create_agent(&payload).await?;
        serde_json::to_value(created).map_err(|err| err.to_string())
    }

    async fn update_agent(&self, agent_id: &str, request: Value) -> Result<Option<Value>, String> {
        let payload: UpdateChatosAgentRequest =
            serde_json::from_value(request).map_err(|err| err.to_string())?;
        let updated = chatos_agents::update_agent(agent_id, &payload).await?;
        updated
            .map(serde_json::to_value)
            .transpose()
            .map_err(|err| err.to_string())
    }
}
