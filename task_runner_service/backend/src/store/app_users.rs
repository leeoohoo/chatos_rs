// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl AppStore {
    pub async fn count_users(&self) -> Result<i64, String> {
        match self {
            Self::InMemory(store) => Ok(store.count_users()),
            Self::Sqlite(store) => store.count_users().await,
            Self::Mongo(store) => store.count_users().await,
        }
    }

    pub async fn list_users(&self) -> Result<Vec<UserRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_users()),
            Self::Sqlite(store) => store.list_users().await,
            Self::Mongo(store) => store.list_users().await,
        }
    }

    pub async fn get_user(&self, id: &str) -> Result<Option<UserRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_user(id)),
            Self::Sqlite(store) => store.get_user(id).await,
            Self::Mongo(store) => store.get_user(id).await,
        }
    }

    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<UserRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_user_by_username(username)),
            Self::Sqlite(store) => store.get_user_by_username(username).await,
            Self::Mongo(store) => store.get_user_by_username(username).await,
        }
    }

    pub async fn save_user(&self, user: UserRecord) -> Result<UserRecord, String> {
        match self {
            Self::InMemory(store) => store.save_user(user),
            Self::Sqlite(store) => store.save_user(user).await,
            Self::Mongo(store) => store.save_user(user).await,
        }
    }

    pub async fn delete_user(&self, id: &str) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.delete_user(id)),
            Self::Sqlite(store) => store.delete_user(id).await,
            Self::Mongo(store) => store.delete_user(id).await,
        }
    }
}
