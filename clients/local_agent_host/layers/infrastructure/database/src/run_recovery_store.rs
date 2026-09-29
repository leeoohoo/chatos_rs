// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteClientStorage, SqliteResultExt};
use sqlx::{Row, SqliteConnection};

pub(super) async fn recover_expired_claims(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    now_unix_ms: i64,
) -> Result<u64, ClientStorageError> {
    let rows = sqlx::query(
        "SELECT run_id, version FROM local_agent_runs \
         WHERE owner_user_id = ? AND status = 'model_running' \
         AND claim_until_unix_ms IS NOT NULL \
         AND claim_until_unix_ms <= ? ORDER BY run_id",
    )
    .bind(owner_user_id)
    .bind(now_unix_ms)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    for row in &rows {
        let run_id: String = row.try_get("run_id").db()?;
        let version: i64 = row.try_get("version").db()?;
        sqlx::query(
            "UPDATE local_agent_runs SET status = 'needs_review', version = version + 1, \
             claim_token = NULL, claim_until_unix_ms = NULL, updated_at_unix_ms = ? \
             WHERE run_id = ? AND owner_user_id = ? AND version = ? \
             AND status = 'model_running'",
        )
        .bind(now_unix_ms)
        .bind(&run_id)
        .bind(owner_user_id)
        .bind(version)
        .execute(&mut *connection)
        .await
        .db()?;
        let event_id = format!("recovery:{run_id}:{}", version + 1);
        SqliteClientStorage::insert_event(
            connection,
            &event_id,
            &run_id,
            "claim_expired_needs_review",
            &serde_json::json!({
                "reason": "the host stopped while a step result was unknown"
            }),
            now_unix_ms,
        )
        .await
        .db()?;
    }
    Ok(rows.len() as u64)
}
