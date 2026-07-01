// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub(super) struct CompatSessionQuery {
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CompatListMessagesQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub order: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CompatListSummariesQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CompatComposeContextRequest {
    pub session_id: String,
    pub mode: Option<String>,
    pub include_raw_messages: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CompatCreateSessionRequest {
    pub user_id: String,
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CompatPatchSessionRequest {
    pub title: Option<String>,
    pub status: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CompatCreateMessageRequest {
    pub role: String,
    pub content: String,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CompatBatchCreateMessagesRequest {
    pub messages: Vec<CompatCreateMessageRequest>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CompatSyncMessageRequest {
    pub role: String,
    pub content: String,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<Value>,
    pub created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CompatSyncSessionRequest {
    pub user_id: String,
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub metadata: Option<Value>,
    pub status: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}
