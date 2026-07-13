// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    pub id: String,
    pub name: String,
    pub command: String,
    pub r#type: String,
    pub args: Option<Value>,
    pub env: Option<Value>,
    pub cwd: Option<String>,
    pub user_id: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}
