// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthoritativeConversationRecord {
    pub record_id: String,
    pub role: String,
    pub content: String,
    pub structured_payload: Option<Value>,
    pub metadata: Option<Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthoritativeConversationPage {
    pub records: Vec<AuthoritativeConversationRecord>,
    pub next_before_cursor: Option<String>,
}

#[async_trait]
pub trait LocalConversationTranscriptSource: Send + Sync {
    async fn list_conversation_records(
        &self,
        tenant_id: &str,
        thread_id: &str,
        conversation_id: &str,
        before_cursor: Option<&str>,
        limit: u32,
    ) -> Result<AuthoritativeConversationPage, String>;
}
