use uuid::Uuid;

use crate::models::session_mcp_server::SessionMcpServer;
use crate::repositories::session_mcp_servers as session_mcp_repo;

#[derive(Debug, Clone)]
pub struct AddSessionMcpServerInput {
    pub session_id: String,
    pub mcp_server_name: Option<String>,
    pub mcp_config_id: Option<String>,
}

pub async fn list_session_mcp_servers(session_id: &str) -> Result<Vec<SessionMcpServer>, String> {
    session_mcp_repo::list_session_mcp_servers(session_id).await
}

pub async fn add_session_mcp_server(
    input: AddSessionMcpServerInput,
) -> Result<SessionMcpServer, String> {
    let item = SessionMcpServer {
        id: Uuid::new_v4().to_string(),
        session_id: input.session_id,
        mcp_server_name: input.mcp_server_name,
        mcp_config_id: input.mcp_config_id,
        created_at: crate::core::time::now_rfc3339(),
    };
    session_mcp_repo::add_session_mcp_server(&item).await?;
    Ok(item)
}

pub async fn delete_session_mcp_server(
    session_id: &str,
    mcp_config_id_or_id: &str,
) -> Result<(), String> {
    session_mcp_repo::delete_session_mcp_server(session_id, mcp_config_id_or_id).await
}
