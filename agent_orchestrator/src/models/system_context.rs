use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemContext {
    pub id: String,
    pub name: String,
    pub content: Option<String>,
    pub user_id: String,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, FromRow)]
pub struct SystemContextRow {
    pub id: String,
    pub name: String,
    pub content: Option<String>,
    pub user_id: String,
    pub is_active: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl SystemContextRow {
    pub fn to_ctx(self) -> SystemContext {
        SystemContext {
            id: self.id,
            name: self.name,
            content: self.content,
            user_id: self.user_id,
            is_active: self.is_active == 1,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
