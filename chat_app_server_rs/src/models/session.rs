use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

use crate::repositories::sessions as repo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub metadata: Option<Value>,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub status: String,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, FromRow)]
pub struct SessionRow {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub metadata: Option<String>,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub status: String,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl SessionRow {
    pub fn to_session(self) -> Session {
        Session {
            id: self.id,
            title: self.title,
            description: self.description,
            metadata: self
                .metadata
                .and_then(|m| serde_json::from_str::<Value>(&m).ok()),
            user_id: self.user_id,
            project_id: self.project_id,
            status: if self.status.trim().is_empty() {
                "active".to_string()
            } else {
                self.status
            },
            archived_at: self.archived_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl Session {
    pub fn new(
        title: String,
        description: Option<String>,
        metadata: Option<Value>,
        user_id: Option<String>,
        project_id: Option<String>,
    ) -> Session {
        let now = crate::core::time::now_rfc3339();
        Session {
            id: Uuid::new_v4().to_string(),
            title,
            description,
            metadata,
            user_id,
            project_id,
            status: "active".to_string(),
            archived_at: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

pub struct SessionService;

impl SessionService {
    pub async fn create(data: Session) -> Result<String, String> {
        repo::create_session(&data).await
    }

    pub async fn get_by_id(session_id: &str) -> Result<Option<Session>, String> {
        repo::get_session_by_id(session_id).await
    }

    pub async fn get_all(limit: Option<i64>, offset: i64) -> Result<Vec<Session>, String> {
        repo::get_all_sessions(limit, offset).await
    }

    pub async fn get_by_user_project(
        user_id: Option<String>,
        project_id: Option<String>,
        limit: Option<i64>,
        offset: i64,
    ) -> Result<Vec<Session>, String> {
        repo::get_sessions_by_user_project(user_id, project_id, limit, offset).await
    }

    pub async fn delete(session_id: &str) -> Result<(), String> {
        repo::delete_session(session_id).await
    }

    pub async fn list_archiving(limit: Option<i64>) -> Result<Vec<String>, String> {
        repo::list_archiving_session_ids(limit).await
    }

    pub async fn process_archive(session_id: &str) -> Result<(), String> {
        repo::process_session_archive(session_id).await
    }

    pub async fn update(
        session_id: &str,
        title: Option<String>,
        description: Option<String>,
        metadata: Option<Value>,
    ) -> Result<(), String> {
        repo::update_session(session_id, title, description, metadata).await
    }
}
