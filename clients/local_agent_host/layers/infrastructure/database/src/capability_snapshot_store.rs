// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    ClientStorageError, IdempotentCommand, LocalCapabilityPolicySnapshot,
    LocalCapabilitySnapshotStore, SqliteClientStorage, SqliteResultExt,
};
use async_trait::async_trait;
use sqlx::Row;

const SNAPSHOT_SELECT: &str =
    "SELECT owner_user_id, profile_key, capability_policy_revision, instructions, \
     prefixed_input_items_json, tools_json FROM local_capability_policy_snapshots \
     WHERE owner_user_id = ? AND profile_key = ? AND capability_policy_revision = ?";

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
        if let Some(replay) = Self::replay(&mut connection, command).await? {
            return Ok(replay);
        }
        if let Some(current) = fetch_snapshot(
            &mut connection,
            &snapshot.owner_user_id,
            &snapshot.profile_key,
            &snapshot.capability_policy_revision,
        )
        .await?
        {
            if current == *snapshot {
                return Ok(current);
            }
            return Err(ClientStorageError::Conflict(format!(
                "capability revision already contains different content: {}@{}",
                snapshot.profile_key, snapshot.capability_policy_revision
            )));
        }
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            if let Some(current) = fetch_snapshot(
                &mut connection,
                &snapshot.owner_user_id,
                &snapshot.profile_key,
                &snapshot.capability_policy_revision,
            )
            .await?
            {
                if current == *snapshot {
                    return Ok(current);
                }
                return Err(ClientStorageError::Conflict(format!(
                    "capability revision already contains different content: {}@{}",
                    snapshot.profile_key, snapshot.capability_policy_revision
                )));
            }
            sqlx::query(
                "INSERT INTO local_capability_policy_snapshots(\
                 owner_user_id, profile_key, capability_policy_revision, instructions, \
                 prefixed_input_items_json, tools_json, created_at_unix_ms) \
                 VALUES(?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&snapshot.owner_user_id)
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
        owner_user_id: &str,
        profile_key: &str,
        capability_policy_revision: &str,
    ) -> Result<Option<LocalCapabilityPolicySnapshot>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        fetch_snapshot(
            &mut connection,
            owner_user_id,
            profile_key,
            capability_policy_revision,
        )
        .await
    }
}

async fn fetch_snapshot(
    connection: &mut sqlx::SqliteConnection,
    owner_user_id: &str,
    profile_key: &str,
    capability_policy_revision: &str,
) -> Result<Option<LocalCapabilityPolicySnapshot>, ClientStorageError> {
    let row = sqlx::query(SNAPSHOT_SELECT)
        .bind(owner_user_id)
        .bind(profile_key)
        .bind(capability_policy_revision)
        .fetch_optional(&mut *connection)
        .await
        .db()?;
    row.map(|row| {
        let prefixed_input_items: String = row.try_get("prefixed_input_items_json").db()?;
        let tools: String = row.try_get("tools_json").db()?;
        let snapshot = LocalCapabilityPolicySnapshot {
            owner_user_id: row.try_get("owner_user_id").db()?,
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
            persist_receipt: true,
        }
    }

    fn snapshot(owner_user_id: &str, instructions: &str) -> LocalCapabilityPolicySnapshot {
        LocalCapabilityPolicySnapshot {
            owner_user_id: owner_user_id.to_string(),
            profile_key: "main_chat".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            instructions: Some(instructions.to_string()),
            prefixed_input_items: vec![serde_json::json!({"role": "developer"})],
            tools: vec![serde_json::json!({"name": "read_file"})],
        }
    }

    #[tokio::test]
    async fn identical_capability_revisions_are_isolated_by_owner() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        storage
            .put_capability_snapshot(
                &command("put-owner-1", "policy-a"),
                &snapshot("user-1", "policy a"),
                1_000,
            )
            .await
            .expect("put first owner");
        storage
            .put_capability_snapshot(
                &command("put-owner-2", "policy-b"),
                &snapshot("user-2", "policy b"),
                2_000,
            )
            .await
            .expect("put second owner");

        let first = storage
            .get_capability_snapshot("user-1", "main_chat", "policy-1")
            .await
            .expect("get first owner")
            .expect("first snapshot");
        let second = storage
            .get_capability_snapshot("user-2", "main_chat", "policy-1")
            .await
            .expect("get second owner")
            .expect("second snapshot");
        assert_eq!(first.instructions.as_deref(), Some("policy a"));
        assert_eq!(second.instructions.as_deref(), Some("policy b"));
        assert!(storage
            .get_capability_snapshot("user-3", "main_chat", "policy-1")
            .await
            .expect("cross-owner read")
            .is_none());
    }

    #[tokio::test]
    async fn immutable_snapshot_round_trips_and_rejects_revision_reuse() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        storage
            .put_capability_snapshot(
                &command("put-1", "local"),
                &snapshot("user-1", "local policy"),
                1_000,
            )
            .await
            .expect("put snapshot");
        storage
            .put_capability_snapshot(
                &command("put-2", "local"),
                &snapshot("user-1", "local policy"),
                2_000,
            )
            .await
            .expect("idempotent put");
        let receipt_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_command_receipts")
                .fetch_one(&storage.pool)
                .await
                .expect("receipt count");
        assert_eq!(receipt_count, 1);
        assert_eq!(
            storage
                .get_capability_snapshot("user-1", "main_chat", "policy-1")
                .await
                .expect("get snapshot"),
            Some(snapshot("user-1", "local policy"))
        );
        let error = storage
            .put_capability_snapshot(
                &command("put-3", "different"),
                &snapshot("user-1", "different policy"),
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
                &snapshot("user-1", "restart policy"),
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
                .get_capability_snapshot("user-1", "main_chat", "policy-1")
                .await
                .expect("get snapshot"),
            Some(snapshot("user-1", "restart policy"))
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
