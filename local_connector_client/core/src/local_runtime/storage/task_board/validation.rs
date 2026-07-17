// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use super::super::LocalDatabase;

pub(super) async fn require_local_task_scope(
    database: &LocalDatabase,
    owner_user_id: &str,
    session_id: &str,
    turn_id: Option<&str>,
) -> Result<()> {
    let exists = if let Some(turn_id) = turn_id {
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM sessions
            INNER JOIN turns ON turns.session_id = sessions.id
            WHERE sessions.id = ? AND sessions.owner_user_id = ? AND turns.id = ?
            "#,
        )
        .bind(session_id)
        .bind(owner_user_id)
        .bind(turn_id)
        .fetch_one(database.pool())
        .await
        .context("validate local task board turn")?
    } else {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sessions WHERE id = ? AND owner_user_id = ?",
        )
        .bind(session_id)
        .bind(owner_user_id)
        .fetch_one(database.pool())
        .await
        .context("validate local task board session")?
    };
    if exists == 0 {
        return Err(anyhow::anyhow!("local task board scope was not found"));
    }
    Ok(())
}

pub(super) async fn validate_prerequisites(
    database: &LocalDatabase,
    owner_user_id: &str,
    session_id: &str,
    task_id: &str,
    prerequisite_ids: &[String],
) -> Result<()> {
    if prerequisite_ids.iter().any(|value| value == task_id) {
        return Err(anyhow::anyhow!("task cannot depend on itself"));
    }
    for prerequisite_id in prerequisite_ids {
        let exists = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM task_board_tasks
            WHERE id = ? AND session_id = ? AND owner_user_id = ?
            "#,
        )
        .bind(prerequisite_id)
        .bind(session_id)
        .bind(owner_user_id)
        .fetch_one(database.pool())
        .await
        .context("validate local task prerequisite")?;
        if exists == 0 {
            return Err(anyhow::anyhow!(
                "local prerequisite task was not found: {prerequisite_id}"
            ));
        }
    }
    Ok(())
}
