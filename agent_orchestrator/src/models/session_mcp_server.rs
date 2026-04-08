use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMcpServer {
    pub id: String,
    pub session_id: String,
    pub mcp_server_name: Option<String>,
    pub mcp_config_id: Option<String>,
    pub created_at: String,
}
