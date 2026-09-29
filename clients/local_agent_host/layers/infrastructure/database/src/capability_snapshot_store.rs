// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    ClientStorageError, IdempotentCommand, LocalCapabilityPolicySnapshot,
    LocalCapabilitySnapshotStore, SqliteClientStorage, SqliteResultExt,
};
use async_trait::async_trait;
use sqlx::Row;

const SNAPSHOT_SELECT: &str = "SELECT profile_key, capability_policy_revision, instructions, \
     prefixed_input_items_json, tools_json FROM local_capability_policy_snapshots \
     WHERE profile_key = ? AND capability_policy_revision = ?";

#[async_trait]
impl LocalCapabilitySnapshotStore for SqliteClientStorage {
    async fn put_capability_snapshot(
        &self,
        command: &IdempotentCommand,
        snapshot: &LocalCapabilityPolicySnapshot,
        now_unix_ms: i64,
    ) -> Result<LocalCapabilityPolicySnapshot, ClientStorageError> {
        snapshot
            .validate()
            .map_err(ClientStorageError::InvalidState)?;
        if now_unix_ms < 0 {
            return Err(ClientStorageError::InvalidState(
                "capability snapshot timestamp must not be negative".to_string(),
            ));
        }
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            if let Some(current) = fetch_snapshot(
                &mut connection,
                &snapshot.profile_key,
                &snapshot.capability_policy_revision,
            )
            .await?
            {
                if current == *snapshot {
                    Self::record_receipt(&mut connection, command, &current, now_unix_ms).await?;
                    return Ok(current);
                }
                return Err(ClientStorageError::Conflict(format!(
                    "capability revision already contains different content: {}@{}",
                    snapshot.profile_key, snapshot.capability_policy_revision
                )));
            }
            sqlx::query(
                "INSERT INTO local_capability_policy_snapshots(\
                 profile_key, capability_policy_revision, instructions, \
                 prefixed_input_items_json, tools_json, created_at_unix_ms) \
                 VALUES(?, ?, ?, ?, ?, ?)",
            )
            .bind(&snapshot.profile_key)
            .bind(&snapshot.capability_policy_revision)
            .bind(&snapshot.instructions)
            .bind(serde_json::to_string(&snapshot.prefixed_input_items)?)
            .bind(serde_json::to_string(&snapshot.tools)?)
            .bind(now_unix_ms)
            .execute(&mut *connection)
            .await
            .db()?;
            Self::record_receipt(&mut connection, command, snapshot, now_unix_ms).await?;
            Ok(snapshot.clone())
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn get_capability_snapshot(
        &self,
        profile_key: &str,
        capability_policy_revision: &str,
    ) -> Result<Option<LocalCapabilityPolicySnapshot>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        fetch_snapshot(&mut connection, profile_key, capability_policy_revision).await
    }
}

async fn fetch_snapshot(
    connection: &mut sqlx::SqliteConnection,
    profile_key: &str,
    capability_policy_revision: &str,
) -> Result<Option<LocalCapabilityPolicySnapshot>, ClientStorageError> {
    let row = sqlx::query(SNAPSHOT_SELECT)
        .bind(profile_key)
        .bind(capability_policy_revision)
        .fetch_optional(&mut *connection)
        .await
        .db()?;
    row.map(|row| {
        let prefixed_input_items: String = row.try_get("prefixed_input_items_json").db()?;
        let tools: String = row.try_get("tools_json").db()?;
        let snapshot = LocalCapabilityPolicySnapshot {
            profile_key: row.try_get("profile_key").db()?,
            capability_policy_revision: row.try_get("capability_policy_revision").db()?,
            instructions: row.try_get("instructions").db()?,
            prefixed_input_items: serde_json::from_str(&prefixed_input_items)?,
            tools: serde_json::from_str(&tools)?,
        };
        snapshot
            .validate()
            .map_err(ClientStorageError::InvalidState)?;
        Ok(snapshot)
    })
    .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn command(id: &str, fingerprint: &str) -> IdempotentCommand {
        IdempotentCommand {
            command_id: id.to_string(),
            request_fingerprint: fingerprint.to_string(),
        }
    }

    fn snapshot(instructions: &str) -> LocalCapabilityPolicySnapshot {
        LocalCapabilityPolicySnapshot {
            profile_key: "main_chat".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            instructions: Some(instructions.to_string()),
            prefixed_input_items: vec![serde_json::json!({"role": "developer"})],
            tools: vec![serde_json::json!({"name": "read_file"})],
        }
    }

    #[tokio::test]
    async fn immutable_snapshot_round_trips_and_rejects_revision_reuse() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        storage
            .put_capability_snapshot(&command("put-1", "local"), &snapshot("local policy"), 1_000)
            .await
            .expect("put snapshot");
        storage
            .put_capability_snapshot(&command("put-2", "local"), &snapshot("local policy"), 2_000)
            .await
            .expect("idempotent put");
        assert_eq!(
            storage
                .get_capability_snapshot("main_chat", "policy-1")
                .await
                .expect("get snapshot"),
            Some(snapshot("local policy"))
        );
        let error = storage
            .put_capability_snapshot(
                &command("put-3", "different"),
                &snapshot("different policy"),
                3_000,
            )
            .await
            .expect_err("revision reuse must fail");
        assert!(matches!(error, ClientStorageError::Conflict(_)));
    }

    #[tokio::test]
    async fn snapshot_survives_database_reopen() {
        let database_path = std::env::temp_dir().join(format!(
            "chatos-local-capabilities-{}.sqlite",
            Uuid::new_v4()
        ));
        let storage = SqliteClientStorage::connect_file(&database_path)
            .await
            .expect("storage");
        storage
            .put_capability_snapshot(
                &command("put-restart", "restart"),
                &snapshot("restart policy"),
                1_000,
            )
            .await
            .expect("put snapshot");
        storage.pool.close().await;
        drop(storage);

        let reopened = SqliteClientStorage::connect_file(&database_path)
            .await
            .expect("reopen storage");
        assert_eq!(
            reopened
                .get_capability_snapshot("main_chat", "policy-1")
                .await
                .expect("get snapshot"),
            Some(snapshot("restart policy"))
        );
        reopened.pool.close().await;
        drop(reopened);
        for path in [
            database_path.clone(),
            database_path.with_extension("sqlite-wal"),
            database_path.with_extension("sqlite-shm"),
        ] {
            if let Err(error) = std::fs::remove_file(path) {
                assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
            }
        }
    }
}
