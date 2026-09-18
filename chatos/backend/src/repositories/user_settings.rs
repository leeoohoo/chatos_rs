// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;
use sqlx::types::Json;

use crate::models::user_settings::UserSettings;
use crate::repositories::db::{db_error, with_db};

pub async fn get_user_settings(user_id: &str) -> Result<Option<UserSettings>, String> {
    with_db(|pool| {
        Box::pin(async move {
            let value = sqlx::query_scalar::<_, Json<Value>>(
                "SELECT settings FROM user_settings WHERE user_id=$1",
            )
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(db_error)?;
            Ok(value.map(|Json(settings)| UserSettings {
                user_id: user_id.to_string(),
                settings,
            }))
        })
    })
    .await
}
pub async fn set_user_settings(user_id: &str, settings: &Value) -> Result<(), String> {
    with_db(|pool|Box::pin(async move{sqlx::query("INSERT INTO user_settings(user_id,updated_at,settings) VALUES($1,now(),$2) ON CONFLICT(user_id) DO UPDATE SET updated_at=now(),settings=EXCLUDED.settings").bind(user_id).bind(Json(settings.clone())).execute(pool).await.map(|_|()).map_err(db_error)})).await
}
pub async fn purge_managed_runtime_settings() -> Result<u64, String> {
    const KEYS: &[&str] = &[
        "MAX_ITERATIONS",
        "TASK_FOLLOW_UP_MAX_ROUNDS",
        "LOG_LEVEL",
        "HISTORY_LIMIT",
        "CHAT_MAX_TOKENS",
        "ATTACHMENT_TOTAL_MAX_BYTES",
        "TERMINAL_UI_ENABLED",
    ];
    with_db(|pool|Box::pin(async move{sqlx::query("UPDATE user_settings SET settings=settings-$1::text[],updated_at=now() WHERE settings ?| $1::text[]").bind(KEYS).execute(pool).await.map(|result|result.rows_affected()).map_err(db_error)})).await
}
