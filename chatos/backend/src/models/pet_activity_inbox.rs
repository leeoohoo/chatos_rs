// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PetActivityInboxStatus {
    Unread,
    Displayed,
    Acknowledged,
    Ignored,
    Handled,
    Resolved,
    Expired,
}

impl PetActivityInboxStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unread => "unread",
            Self::Displayed => "displayed",
            Self::Acknowledged => "acknowledged",
            Self::Ignored => "ignored",
            Self::Handled => "handled",
            Self::Resolved => "resolved",
            Self::Expired => "expired",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PetActivityRoute {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PetActivityInboxRecord {
    pub id: String,
    pub user_id: String,
    pub activity_key: String,
    pub activity_version: String,
    pub source: String,
    pub kind: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default)]
    pub route: PetActivityRoute,
    pub business_status: String,
    pub inbox_status: PetActivityInboxStatus,
    #[serde(default)]
    pub requires_action: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_sequence: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    pub occurred_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub displayed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acknowledged_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignored_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handled_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct PetActivityInboxUpsert {
    pub user_id: String,
    pub activity_key: String,
    pub activity_version: String,
    pub source: String,
    pub kind: String,
    pub title: String,
    pub detail: Option<String>,
    pub route: PetActivityRoute,
    pub business_status: String,
    pub requires_action: bool,
    pub event_id: Option<String>,
    pub event_sequence: Option<i64>,
    pub metadata: Option<Value>,
    pub occurred_at: String,
    pub expires_at: Option<String>,
    pub resolved: bool,
}
