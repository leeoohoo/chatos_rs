// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    ClientStorageError, IdempotentCommand, LocalPluginHookBundleStore, SqliteClientStorage,
    SqliteResultExt,
};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    LocalPluginHookBundleRecord, LocalPluginHookBundleSpec, LOCAL_PLUGIN_HOOK_MAX_BUNDLES,
};
use sqlx::Row;

pub(super) const SCHEMA_V38: &[&str] = &[
    "CREATE TABLE local_plugin_hook_bundles (\
       owner_user_id TEXT NOT NULL,\
       bundle_id TEXT NOT NULL,\
       spec_json TEXT NOT NULL,\
       created_at_unix_ms INTEGER NOT NULL,\
       PRIMARY KEY(owner_user_id, bundle_id)\
     )",
    "CREATE INDEX local_plugin_hook_bundles_owner ON local_plugin_hook_bundles(\
       owner_user_id, created_at_unix_ms, bundle_id\
     )",
    "CREATE TABLE local_plugin_hook_activations (\
       owner_user_id TEXT NOT NULL,\
       bundle_id TEXT NOT NULL,\
       activated_at_unix_ms INTEGER NOT NULL,\
       PRIMARY KEY(owner_user_id, bundle_id),\
       FOREIGN KEY(owner_user_id, bundle_id) REFERENCES local_plugin_hook_bundles(\
         owner_user_id, bundle_id\
       ) ON DELETE CASCADE\
     )",
];

#[async_trait]
impl LocalPluginHookBundleStore for SqliteClientStorage {
    async fn put_plugin_hook_bundle(
        &self,
        command: &IdempotentCommand,
        bundle: &LocalPluginHookBundleSpec,
        now_unix_ms: i64,
    ) -> Result<LocalPluginHookBundleRecord, ClientStorageError> {
        bundle
            .validate()
            .map_err(ClientStorageError::InvalidState)?;
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            let existing = sqlx::query(
                "SELECT spec_json, created_at_unix_ms FROM local_plugin_hook_bundles \
                 WHERE owner_user_id = ? AND bundle_id = ?",
            )
            .bind(&bundle.owner_user_id)
            .bind(&bundle.bundle_id)
            .fetch_optional(&mut *connection)
            .await
            .db()?;
            let record = if let Some(row) = existing {
                let stored: LocalPluginHookBundleSpec =
                    serde_json::from_str(&row.try_get::<String, _>("spec_json").db()?)?;
                if stored != *bundle {
                    return Err(ClientStorageError::Conflict(format!(
                        "plugin hook bundle identity already contains different material: {}",
                        bundle.bundle_id
                    )));
                }
                LocalPluginHookBundleRecord {
                    spec: stored,
                    created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
                }
            } else {
                let count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM local_plugin_hook_bundles WHERE owner_user_id = ?",
                )
                .bind(&bundle.owner_user_id)
                .fetch_one(&mut *connection)
                .await
                .db()?;
                if usize::try_from(count).unwrap_or(usize::MAX) >= LOCAL_PLUGIN_HOOK_MAX_BUNDLES {
                    return Err(ClientStorageError::InvalidState(format!(
                        "plugin hook bundle limit reached: {LOCAL_PLUGIN_HOOK_MAX_BUNDLES}"
                    )));
                }
                sqlx::query(
                    "INSERT INTO local_plugin_hook_bundles(\
                     owner_user_id, bundle_id, spec_json, created_at_unix_ms) VALUES(?, ?, ?, ?)",
                )
                .bind(&bundle.owner_user_id)
                .bind(&bundle.bundle_id)
                .bind(serde_json::to_string(bundle)?)
                .bind(now_unix_ms)
                .execute(&mut *connection)
                .await
                .db()?;
                LocalPluginHookBundleRecord {
                    spec: bundle.clone(),
                    created_at_unix_ms: now_unix_ms,
                }
            };
            Self::record_receipt(&mut connection, command, &record, now_unix_ms).await?;
            Ok(record)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn list_plugin_hook_bundles(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<LocalPluginHookBundleRecord>, ClientStorageError> {
        let rows = sqlx::query(
            "SELECT bundles.spec_json, bundles.created_at_unix_ms \
             FROM local_plugin_hook_bundles bundles \
             INNER JOIN local_plugin_hook_activations active \
             ON active.owner_user_id = bundles.owner_user_id \
             AND active.bundle_id = bundles.bundle_id \
             WHERE bundles.owner_user_id = ? \
             ORDER BY bundles.created_at_unix_ms, bundles.bundle_id",
        )
        .bind(owner_user_id)
        .fetch_all(&self.pool)
        .await
        .db()?;
        rows.into_iter()
            .map(|row| {
                Ok(LocalPluginHookBundleRecord {
                    spec: serde_json::from_str(&row.try_get::<String, _>("spec_json").db()?)?,
                    created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
                })
            })
            .collect()
    }

    async fn set_active_plugin_hook_bundles(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        bundle_ids: &[String],
        now_unix_ms: i64,
    ) -> Result<Vec<String>, ClientStorageError> {
        let mut active = bundle_ids.to_vec();
        active.sort();
        active.dedup();
        if active.len() != bundle_ids.len() || active.len() > LOCAL_PLUGIN_HOOK_MAX_BUNDLES {
            return Err(ClientStorageError::InvalidState(
                "active plugin hook bundle set is invalid".to_string(),
            ));
        }
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            for bundle_id in &active {
                let exists: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM local_plugin_hook_bundles \
                     WHERE owner_user_id = ? AND bundle_id = ?",
                )
                .bind(owner_user_id)
                .bind(bundle_id)
                .fetch_one(&mut *connection)
                .await
                .db()?;
                if exists != 1 {
                    return Err(ClientStorageError::NotFound(bundle_id.clone()));
                }
            }
            sqlx::query("DELETE FROM local_plugin_hook_activations WHERE owner_user_id = ?")
                .bind(owner_user_id)
                .execute(&mut *connection)
                .await
                .db()?;
            for bundle_id in &active {
                sqlx::query(
                    "INSERT INTO local_plugin_hook_activations(\
                     owner_user_id, bundle_id, activated_at_unix_ms) VALUES(?, ?, ?)",
                )
                .bind(owner_user_id)
                .bind(bundle_id)
                .bind(now_unix_ms)
                .execute(&mut *connection)
                .await
                .db()?;
            }
            Self::record_receipt(&mut connection, command, &active, now_unix_ms).await?;
            Ok(active)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_local_agent_protocol::LocalPluginHookDefinition;

    fn command(id: &str) -> IdempotentCommand {
        IdempotentCommand {
            command_id: id.to_string(),
            request_fingerprint: id.to_string(),
            persist_receipt: true,
        }
    }

    fn bundle() -> LocalPluginHookBundleSpec {
        LocalPluginHookBundleSpec {
            bundle_id: "bundle-1".to_string(),
            owner_user_id: "user-1".to_string(),
            plugin_id: "plugin-1".to_string(),
            release_id: "release-1".to_string(),
            component_key: "hooks-1".to_string(),
            artifact_sha256: "a".repeat(64),
            content_sha256: "b".repeat(64),
            normalized_hook_sha256: "c".repeat(64),
            command_sha256: "d".repeat(64),
            permission_snapshot_sha256: "e".repeat(64),
            runner_policy_version: 1,
            installation_root: "/plugins/plugin-1".to_string(),
            permissions: vec!["process.spawn".to_string()],
            hooks: vec![LocalPluginHookDefinition {
                hook_id: "hook-1".to_string(),
                event: "SessionStart".to_string(),
                matcher: None,
                command: "node hook.js".to_string(),
                timeout_ms: 5_000,
                additional_context_limit: 65_536,
            }],
            enabled: true,
            trusted_at_unix_ms: 100,
        }
    }

    #[tokio::test]
    async fn immutable_bundle_is_idempotent_and_owner_scoped() {
        let store = SqliteClientStorage::connect_memory().await.expect("store");
        let original = bundle();
        store
            .put_plugin_hook_bundle(&command("put-1"), &original, 200)
            .await
            .expect("put");
        store
            .put_plugin_hook_bundle(&command("put-2"), &original, 300)
            .await
            .expect("idempotent put");
        assert!(store
            .list_plugin_hook_bundles("user-1")
            .await
            .expect("inactive list")
            .is_empty());
        store
            .set_active_plugin_hook_bundles(
                &command("activate-1"),
                "user-1",
                std::slice::from_ref(&original.bundle_id),
                350,
            )
            .await
            .expect("activate");
        assert_eq!(
            store
                .list_plugin_hook_bundles("user-1")
                .await
                .expect("list")
                .len(),
            1
        );
        assert!(store
            .list_plugin_hook_bundles("user-2")
            .await
            .expect("other owner")
            .is_empty());
        let mut changed = original;
        changed.hooks[0].command = "node changed.js".to_string();
        let error = store
            .put_plugin_hook_bundle(&command("put-3"), &changed, 400)
            .await
            .expect_err("immutable conflict");
        assert!(matches!(error, ClientStorageError::Conflict(_)));
        store
            .set_active_plugin_hook_bundles(&command("activate-none"), "user-1", &[], 500)
            .await
            .expect("deactivate");
        assert!(store
            .list_plugin_hook_bundles("user-1")
            .await
            .expect("inactive")
            .is_empty());
    }
}
