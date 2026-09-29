// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteClientStorage, SqliteResultExt};
use chatos_local_agent_protocol::{LocalPluginInstallationPage, LocalPluginInstallationSummary};
use sqlx::Row;

pub(super) async fn list_installations(
    storage: &SqliteClientStorage,
    owner_user_id: &str,
    before_updated_at_unix_ms: Option<i64>,
    before_installation_id: Option<&str>,
    limit: u32,
) -> Result<LocalPluginInstallationPage, ClientStorageError> {
    validate_page(
        owner_user_id,
        before_updated_at_unix_ms,
        before_installation_id,
        limit,
    )?;
    let cursor_filter = if before_updated_at_unix_ms.is_some() {
        " AND (updated_at_unix_ms < ? OR (updated_at_unix_ms = ? AND installation_id < ?))"
    } else {
        ""
    };
    let sql = format!(
        "SELECT installation_id, owner_user_id, plugin_id, release_id, component_id, \
         component_revision, server_id, enabled, version, created_at_unix_ms, \
         updated_at_unix_ms FROM local_plugin_installations \
         WHERE owner_user_id = ?{cursor_filter} \
         ORDER BY updated_at_unix_ms DESC, installation_id DESC LIMIT ?"
    );
    let mut query = sqlx::query(&sql).bind(owner_user_id);
    if let (Some(timestamp), Some(installation_id)) =
        (before_updated_at_unix_ms, before_installation_id)
    {
        query = query.bind(timestamp).bind(timestamp).bind(installation_id);
    }
    let mut connection = storage.pool.acquire().await.db()?;
    let rows = query
        .bind(i64::from(limit) + 1)
        .fetch_all(&mut *connection)
        .await
        .db()?;
    let mut installations = rows
        .into_iter()
        .map(|row| {
            Ok(LocalPluginInstallationSummary {
                installation_id: row.try_get("installation_id").db()?,
                owner_user_id: row.try_get("owner_user_id").db()?,
                plugin_id: row.try_get("plugin_id").db()?,
                release_id: row.try_get("release_id").db()?,
                component_id: row.try_get("component_id").db()?,
                component_revision: row.try_get("component_revision").db()?,
                server_id: row.try_get("server_id").db()?,
                enabled: row.try_get("enabled").db()?,
                version: u64::try_from(row.try_get::<i64, _>("version").db()?).map_err(|_| {
                    ClientStorageError::InvalidState(
                        "invalid plugin installation version".to_string(),
                    )
                })?,
                created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
                updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
            })
        })
        .collect::<Result<Vec<_>, ClientStorageError>>()?;
    let has_more = installations.len() > limit as usize;
    installations.truncate(limit as usize);
    let (next_before_updated_at_unix_ms, next_before_installation_id) = if has_more {
        let last = installations.last().expect("positive validated page limit");
        (
            Some(last.updated_at_unix_ms),
            Some(last.installation_id.clone()),
        )
    } else {
        (None, None)
    };
    Ok(LocalPluginInstallationPage {
        installations,
        next_before_updated_at_unix_ms,
        next_before_installation_id,
    })
}

fn validate_page(
    owner_user_id: &str,
    before_updated_at_unix_ms: Option<i64>,
    before_installation_id: Option<&str>,
    limit: u32,
) -> Result<(), ClientStorageError> {
    let owner_valid = !owner_user_id.trim().is_empty()
        && owner_user_id.len() <= 256
        && !owner_user_id.chars().any(char::is_control);
    let cursor_valid = match (before_updated_at_unix_ms, before_installation_id) {
        (None, None) => true,
        (Some(timestamp), Some(installation_id)) => {
            timestamp >= 0
                && !installation_id.trim().is_empty()
                && installation_id.len() <= 256
                && !installation_id.chars().any(char::is_control)
        }
        _ => false,
    };
    if !owner_valid || !cursor_valid || !(1..=200).contains(&limit) {
        return Err(ClientStorageError::InvalidState(
            "invalid Plugin installation list page request".to_string(),
        ));
    }
    Ok(())
}
