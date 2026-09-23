// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl PostgresStore {
    pub(in crate::store) async fn count_users(&self) -> Result<i64, String> {
        sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(&self.pool)
            .await
            .map_err(db_error)
    }

    pub(in crate::store) async fn list_users(&self) -> Result<Vec<UserRecord>, String> {
        let values = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM users ORDER BY updated_at DESC,username ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        values.into_iter().map(decode_json).collect()
    }

    pub(in crate::store) async fn get_user(&self, id: &str) -> Result<Option<UserRecord>, String> {
        sqlx::query_scalar::<_, Json<serde_json::Value>>("SELECT data FROM users WHERE id=$1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?
            .map(decode_json)
            .transpose()
    }

    pub(in crate::store) async fn get_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserRecord>, String> {
        sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM users WHERE username_normalized=$1",
        )
        .bind(username.trim().to_lowercase())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_error)?
        .map(decode_json)
        .transpose()
    }

    pub(in crate::store) async fn save_user(&self, user: UserRecord) -> Result<UserRecord, String> {
        let result = sqlx::query(
            "INSERT INTO users(id,username,username_normalized,enabled,created_at,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7) \
             ON CONFLICT(id) DO UPDATE SET username=EXCLUDED.username,username_normalized=EXCLUDED.username_normalized,enabled=EXCLUDED.enabled,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data",
        )
        .bind(&user.id)
        .bind(&user.username)
        .bind(user.username.trim().to_lowercase())
        .bind(user.enabled)
        .bind(timestamp(&user.created_at)?)
        .bind(timestamp(&user.updated_at)?)
        .bind(json(&user)?)
        .execute(&self.pool)
        .await;
        match result {
            Ok(_) => Ok(user),
            Err(error) if error.to_string().contains("users_username_normalized_key") => {
                Err(format!("用户名已存在: {}", user.username))
            }
            Err(error) => Err(db_error(error)),
        }
    }

    pub(in crate::store) async fn delete_user(&self, id: &str) -> Result<bool, String> {
        sqlx::query("DELETE FROM users WHERE id=$1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map(|result| result.rows_affected() == 1)
            .map_err(db_error)
    }
}
