use serde::{Deserialize, Serialize};

use super::default_active;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImContact {
    pub id: String,
    pub owner_user_id: String,
    pub agent_id: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    #[serde(default = "default_active")]
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateImContactRequest {
    pub owner_user_id: String,
    pub agent_id: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
}
