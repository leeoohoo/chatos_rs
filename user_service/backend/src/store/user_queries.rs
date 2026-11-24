// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{UserOptionPageResponse, UserOptionRecord};

use super::{db_error, AppStore};

impl AppStore {
    pub async fn resolve_user_options(
        &self,
        user_ids: &[String],
    ) -> Result<Vec<UserOptionRecord>, String> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }
        let users = sqlx::query_as::<_, (String, String, String)>(
            "SELECT id, username, display_name FROM users WHERE id = ANY($1) ORDER BY username, id",
        )
        .bind(user_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        Ok(users
            .into_iter()
            .map(|(id, username, display_name)| UserOptionRecord {
                id,
                username,
                display_name,
            })
            .collect())
    }

    pub async fn list_user_options_page(
        &self,
        search: Option<&str>,
        limit: i64,
        offset: u64,
    ) -> Result<UserOptionPageResponse, String> {
        let offset =
            i64::try_from(offset).map_err(|_| "user page offset is too large".to_string())?;
        let search_pattern = search.map(|value| format!("{}%", escape_like_pattern(value)));
        let (users, total) = match search_pattern.as_deref() {
            Some(pattern) => tokio::try_join!(
                async {
                    sqlx::query_as::<_, (String, String, String)>(
                        r#"SELECT id, username, display_name
                           FROM users
                           WHERE lower(username) LIKE lower($1) ESCAPE '\'
                              OR lower(display_name) LIKE lower($1) ESCAPE '\'
                           ORDER BY username, id
                           LIMIT $2 OFFSET $3"#,
                    )
                    .bind(pattern)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_error)
                },
                async {
                    sqlx::query_scalar::<_, i64>(
                        r#"SELECT count(*) FROM users
                           WHERE lower(username) LIKE lower($1) ESCAPE '\'
                              OR lower(display_name) LIKE lower($1) ESCAPE '\'"#,
                    )
                    .bind(pattern)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(db_error)
                },
            )?,
            None => tokio::try_join!(
                async {
                    sqlx::query_as::<_, (String, String, String)>(
                        "SELECT id, username, display_name FROM users ORDER BY username, id LIMIT $1 OFFSET $2",
                    )
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_error)
                },
                async {
                    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM users")
                        .fetch_one(&self.pool)
                        .await
                        .map_err(db_error)
                },
            )?,
        };
        Ok(UserOptionPageResponse {
            items: users
                .into_iter()
                .map(|(id, username, display_name)| UserOptionRecord {
                    id,
                    username,
                    display_name,
                })
                .collect(),
            total: u64::try_from(total).map_err(|error| error.to_string())?,
        })
    }
}

pub(super) fn escape_like_pattern(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
