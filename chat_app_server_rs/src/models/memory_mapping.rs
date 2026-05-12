use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatosContact {
    pub id: String,
    pub user_id: String,
    pub agent_id: String,
    pub agent_name_snapshot: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl ChatosContact {
    pub fn new(
        user_id: String,
        agent_id: String,
        agent_name_snapshot: Option<String>,
        status: String,
    ) -> Self {
        let now = crate::core::time::now_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            user_id,
            agent_id,
            agent_name_snapshot,
            status,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(Debug, FromRow)]
pub struct ChatosContactRow {
    pub id: String,
    pub user_id: String,
    pub agent_id: String,
    pub agent_name_snapshot: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl ChatosContactRow {
    pub fn to_contact(self) -> ChatosContact {
        ChatosContact {
            id: self.id,
            user_id: self.user_id,
            agent_id: self.agent_id,
            agent_name_snapshot: self.agent_name_snapshot,
            status: self.status,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatosMemoryProject {
    pub id: String,
    pub user_id: String,
    pub project_id: String,
    pub name: String,
    pub root_path: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub is_virtual: i64,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

impl ChatosMemoryProject {
    pub fn new(
        user_id: String,
        project_id: String,
        name: String,
        root_path: Option<String>,
        description: Option<String>,
        status: String,
        is_virtual: i64,
    ) -> Self {
        let now = crate::core::time::now_rfc3339();
        let archived_at = if status == "archived" || status == "deleted" {
            Some(now.clone())
        } else {
            None
        };
        Self {
            id: Uuid::new_v4().to_string(),
            user_id,
            project_id,
            name,
            root_path,
            description,
            status,
            is_virtual: is_virtual.max(0),
            created_at: now.clone(),
            updated_at: now,
            archived_at,
        }
    }
}

#[derive(Debug, FromRow)]
pub struct ChatosMemoryProjectRow {
    pub id: String,
    pub user_id: String,
    pub project_id: String,
    pub name: String,
    pub root_path: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub is_virtual: i64,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

impl ChatosMemoryProjectRow {
    pub fn to_project(self) -> ChatosMemoryProject {
        ChatosMemoryProject {
            id: self.id,
            user_id: self.user_id,
            project_id: self.project_id,
            name: self.name,
            root_path: self.root_path,
            description: self.description,
            status: self.status,
            is_virtual: self.is_virtual,
            created_at: self.created_at,
            updated_at: self.updated_at,
            archived_at: self.archived_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatosProjectAgentLink {
    pub id: String,
    pub user_id: String,
    pub project_id: String,
    pub agent_id: String,
    pub contact_id: Option<String>,
    pub latest_session_id: Option<String>,
    pub first_bound_at: String,
    pub last_bound_at: String,
    pub last_message_at: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl ChatosProjectAgentLink {
    pub fn new(
        user_id: String,
        project_id: String,
        agent_id: String,
        contact_id: Option<String>,
        latest_session_id: Option<String>,
        last_message_at: Option<String>,
        status: String,
    ) -> Self {
        let now = crate::core::time::now_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            user_id,
            project_id,
            agent_id,
            contact_id,
            latest_session_id,
            first_bound_at: now.clone(),
            last_bound_at: now.clone(),
            last_message_at,
            status,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(Debug, FromRow)]
pub struct ChatosProjectAgentLinkRow {
    pub id: String,
    pub user_id: String,
    pub project_id: String,
    pub agent_id: String,
    pub contact_id: Option<String>,
    pub latest_session_id: Option<String>,
    pub first_bound_at: String,
    pub last_bound_at: String,
    pub last_message_at: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl ChatosProjectAgentLinkRow {
    pub fn to_link(self) -> ChatosProjectAgentLink {
        ChatosProjectAgentLink {
            id: self.id,
            user_id: self.user_id,
            project_id: self.project_id,
            agent_id: self.agent_id,
            contact_id: self.contact_id,
            latest_session_id: self.latest_session_id,
            first_bound_at: self.first_bound_at,
            last_bound_at: self.last_bound_at,
            last_message_at: self.last_message_at,
            status: self.status,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
