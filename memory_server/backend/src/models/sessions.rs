use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{default_active, default_i64_0, deserialize_string_active, deserialize_vec_or_default};

fn default_contact_authorized_builtin_mcp_ids() -> Vec<String> {
    Vec::new()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub metadata: Option<Value>,
    #[serde(default = "default_active", deserialize_with = "deserialize_string_active")]
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub user_id: String,
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSessionRequest {
    pub title: Option<String>,
    pub status: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub id: String,
    pub user_id: String,
    pub agent_id: String,
    pub agent_name_snapshot: Option<String>,
    #[serde(
        default = "default_contact_authorized_builtin_mcp_ids",
        deserialize_with = "deserialize_vec_or_default"
    )]
    pub authorized_builtin_mcp_ids: Vec<String>,
    #[serde(default = "default_active", deserialize_with = "deserialize_string_active")]
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryProject {
    pub id: String,
    pub user_id: String,
    pub project_id: String,
    pub name: String,
    pub root_path: Option<String>,
    pub description: Option<String>,
    #[serde(default = "default_active", deserialize_with = "deserialize_string_active")]
    pub status: String,
    #[serde(default = "default_i64_0")]
    pub is_virtual: i64,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryProjectAgentLink {
    pub id: String,
    pub user_id: String,
    pub project_id: String,
    pub agent_id: String,
    pub contact_id: Option<String>,
    pub latest_session_id: Option<String>,
    pub first_bound_at: String,
    pub last_bound_at: String,
    pub last_message_at: Option<String>,
    #[serde(default = "default_active", deserialize_with = "deserialize_string_active")]
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateContactRequest {
    pub user_id: String,
    pub agent_id: String,
    pub agent_name_snapshot: Option<String>,
    #[serde(default = "default_contact_authorized_builtin_mcp_ids")]
    pub authorized_builtin_mcp_ids: Vec<String>,
}
