use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::repositories::projects as repo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub description: Option<String>,
    pub user_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, FromRow)]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub description: Option<String>,
    pub user_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl ProjectRow {
    pub fn to_project(self) -> Project {
        Project {
            id: self.id,
            name: self.name,
            root_path: self.root_path,
            description: self.description,
            user_id: self.user_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl Project {
    pub fn new(name: String, root_path: String, description: Option<String>, user_id: Option<String>) -> Project {
        let now = chrono::Utc::now().to_rfc3339();
        Project {
            id: Uuid::new_v4().to_string(),
            name,
            root_path,
            description,
            user_id,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

pub struct ProjectService;

impl ProjectService {
    pub async fn create(data: Project) -> Result<String, String> {
        repo::create_project(&data).await
    }

    pub async fn get_by_id(id: &str) -> Result<Option<Project>, String> {
        repo::get_project_by_id(id).await
    }

    pub async fn list(user_id: Option<String>) -> Result<Vec<Project>, String> {
        repo::list_projects(user_id).await
    }

    pub async fn update(id: &str, name: Option<String>, root_path: Option<String>, description: Option<String>) -> Result<(), String> {
        repo::update_project(id, name, root_path, description).await
    }

    pub async fn delete(id: &str) -> Result<(), String> {
        repo::delete_project(id).await
    }
}
