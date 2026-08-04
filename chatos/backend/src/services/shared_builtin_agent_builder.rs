// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use serde_json::Value;

use chatos_mcp::AgentBuilderStore;

use crate::models::chatos_agent_types::{CreateChatosAgentRequest, UpdateChatosAgentRequest};
use crate::services::chatos_agents;

#[derive(Debug, Clone)]
pub struct ChatosAgentBuilderStore {
    owner_user_id: String,
}

impl ChatosAgentBuilderStore {
    pub fn new(owner_user_id: &str) -> Result<Self, String> {
        let owner_user_id = owner_user_id.trim();
        if owner_user_id.is_empty() {
            return Err("Agent Builder owner_user_id is required".to_string());
        }
        Ok(Self {
            owner_user_id: owner_user_id.to_string(),
        })
    }
}

#[async_trait]
impl AgentBuilderStore for ChatosAgentBuilderStore {
    async fn create_agent(&self, request: Value) -> Result<Value, String> {
        let mut payload: CreateChatosAgentRequest =
            serde_json::from_value(request).map_err(|err| err.to_string())?;
        payload.user_id = Some(self.owner_user_id.clone());
        let created = chatos_agents::create_agent(&payload).await?;
        serde_json::to_value(created).map_err(|err| err.to_string())
    }

    async fn update_agent(&self, agent_id: &str, request: Value) -> Result<Option<Value>, String> {
        let Some(existing) = chatos_agents::get_agent(agent_id).await? else {
            return Ok(None);
        };
        if existing.user_id.trim() != self.owner_user_id {
            return Err("Agent Builder cannot update an agent owned by another user".to_string());
        }
        let payload: UpdateChatosAgentRequest =
            serde_json::from_value(request).map_err(|err| err.to_string())?;
        let updated = chatos_agents::update_agent(agent_id, &payload).await?;
        updated
            .map(serde_json::to_value)
            .transpose()
            .map_err(|err| err.to_string())
    }
}
