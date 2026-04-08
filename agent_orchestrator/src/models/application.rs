use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Application {
    pub id: String,
    pub name: String,
    pub url: String,
    pub description: Option<String>,
    pub user_id: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, FromRow)]
pub struct ApplicationRow {
    pub id: String,
    pub name: String,
    pub url: String,
    pub description: Option<String>,
    pub user_id: Option<String>,
    pub enabled: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl ApplicationRow {
    pub fn to_app(self) -> Application {
        Application {
            id: self.id,
            name: self.name,
            url: self.url,
            description: self.description,
            user_id: self.user_id,
            enabled: self.enabled == 1,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
