// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMcpServer {
    pub id: String,
    pub session_id: String,
    pub mcp_server_name: Option<String>,
    pub mcp_config_id: Option<String>,
    pub created_at: String,
}
