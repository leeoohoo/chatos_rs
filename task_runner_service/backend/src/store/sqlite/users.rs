// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl SqliteStore {
    pub(in crate::store) async fn count_users(&self) -> Result<i64, String> {
        let row = sqlx::query("SELECT COUNT(1) AS total FROM users")
            .fetch_one(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(row.get::<i64, _>("total"))
    }

    pub(in crate::store) async fn list_users(&self) -> Result<Vec<UserRecord>, String> {
        let rows =
            sqlx::query("SELECT * FROM users ORDER BY datetime(updated_at) DESC, username ASC")
                .fetch_all(&self.pool)
                .await
                .map_err(|err| err.to_string())?;
        rows.iter().map(user_from_row).collect()
    }

    pub(in crate::store) async fn get_user(&self, id: &str) -> Result<Option<UserRecord>, String> {
        let row = sqlx::query("SELECT * FROM users WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        row.as_ref().map(user_from_row).transpose()
    }

    pub(in crate::store) async fn get_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserRecord>, String> {
        let row = sqlx::query("SELECT * FROM users WHERE username = ?")
            .bind(username)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        row.as_ref().map(user_from_row).transpose()
    }

    pub(in crate::store) async fn save_user(&self, user: UserRecord) -> Result<UserRecord, String> {
        sqlx::query(
            "INSERT INTO users (
                id, username, display_name, password_hash, role, enabled, created_at, updated_at,
                last_login_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                username = excluded.username,
                display_name = excluded.display_name,
                password_hash = excluded.password_hash,
                role = excluded.role,
                enabled = excluded.enabled,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                last_login_at = excluded.last_login_at",
        )
        .bind(&user.id)
        .bind(&user.username)
        .bind(&user.display_name)
        .bind(&user.password_hash)
        .bind(user_role_to_str(user.role))
        .bind(bool_to_int(user.enabled))
        .bind(&user.created_at)
        .bind(&user.updated_at)
        .bind(user.last_login_at.clone())
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(user)
    }

    pub(in crate::store) async fn delete_user(&self, id: &str) -> Result<bool, String> {
        let result = sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.rows_affected() > 0)
    }
}
