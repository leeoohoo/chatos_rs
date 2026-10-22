// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Unix unit-test subprocess fault injection. This entire module and all calls
//! to its checkpoints are absent from normal library/Host/release builds.

use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Connection;
use std::{
    fs,
    io::Write,
    os::unix::{fs::OpenOptionsExt, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{process::Command, time::timeout};

const POINT_ENV: &str = "CHATOS_STORAGE_UNIT_TEST_CRASH_POINT";
const ROOT_ENV: &str = "CHATOS_STORAGE_UNIT_TEST_CRASH_ROOT";
const DEADLINE: Duration = Duration::from_secs(20);

pub(super) async fn checkpoint(point: &str) {
    if std::env::var(POINT_ENV).ok().as_deref() != Some(point) {
        return;
    }
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("unit-test crash fixture root"));
    let mut marker = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(root.join("reached.json"))
        .expect("publish unit-test checkpoint");
    marker
        .write_all(
            &serde_json::to_vec(&json!({"point": point, "pid": std::process::id()})).unwrap(),
        )
        .unwrap();
    marker.sync_all().unwrap();
    // The parent must send SIGKILL. Never return through Rust destructors or
    // finish_write(), which would only test a graceful transaction rollback.
    std::future::pending::<()>().await;
}

#[tokio::test]
async fn migration_child() {
    let Some(root) = std::env::var_os(ROOT_ENV) else {
        return; // Ordinary full-suite invocation is not a crash worker.
    };
    assert!(std::env::var_os(POINT_ENV).is_some());
    let storage = SqliteClientStorage::connect_file(&PathBuf::from(root).join("client.sqlite"))
        .await
        .expect("crash worker must reach the requested migration stage");
    storage.pool.close().await;
    panic!("crash worker completed without reaching its requested checkpoint");
}

fn business_command() -> IdempotentCommand {
    IdempotentCommand {
        command_id: "business-create".into(),
        request_fingerprint: r#"{"type":"create_run","owner_user_id":"user-1"}"#.into(),
        persist_receipt: true,
    }
}

fn legacy_fingerprint(id: &str) -> String {
    json!({"type": "legacy_write", "owner_user_id": "user-1", "id": id}).to_string()
}

fn legacy_response(letter: char) -> String {
    // Original whitespace is part of result identity and must survive v37.
    format!(
        " {{ \"revision\": 1, \"payload\": \"{}\" }} \n",
        letter.to_string().repeat(1024 * 1024)
    )
}

fn expected_response_bytes() -> usize {
    legacy_response('x').len()
        + legacy_response('y').len()
        + serde_json::to_string(&super::tests::run(1000))
            .unwrap()
            .len()
}

struct Fixture {
    directory: tempfile::TempDir,
    original_backup: PathBuf,
    backup_digest: Vec<u8>,
}

impl Fixture {
    fn database(&self) -> PathBuf {
        self.directory.path().join("client.sqlite")
    }

    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("client.sqlite");
        let storage = SqliteClientStorage::connect_file(&database).await.unwrap();
        storage
            .create_run(
                &business_command(),
                &super::tests::run(1000),
                "business-event",
            )
            .await
            .unwrap();
        let mut connection = storage.pool.acquire().await.unwrap();
        super::migration_tests::rewind_receipt_v35(&mut connection).await;
        for (id, letter) in [("a-large", 'x'), ("z-large", 'y')] {
            sqlx::query("INSERT INTO local_agent_command_receipts(command_id, request_fingerprint, response_json, created_at_unix_ms) VALUES(?, ?, ?, 1000)")
                .bind(id).bind(legacy_fingerprint(id)).bind(legacy_response(letter))
                .execute(&mut *connection).await.unwrap();
        }
        drop(connection);
        // A previously verified recovery point must survive every crash/retry.
        let original_backup = super::maintenance::create_migration_backup(
            &storage.pool,
            &database,
            34,
            super::migration::SCHEMA_VERSION,
        )
        .await
        .unwrap();
        let backup_digest = Sha256::digest(fs::read(&original_backup).unwrap()).to_vec();
        storage.pool.close().await;
        Self {
            directory,
            original_backup,
            backup_digest,
        }
    }

    fn assert_original_backup_unchanged(&self) {
        assert_eq!(
            Sha256::digest(fs::read(&self.original_backup).unwrap()).to_vec(),
            self.backup_digest
        );
    }

    async fn kill_at(&self, point: &str) {
        let marker = self.directory.path().join("reached.json");
        if marker.exists() {
            fs::remove_file(&marker).unwrap();
        }
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "migration_crash_tests::migration_child",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(POINT_ENV, point)
            .env(ROOT_ENV, self.directory.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let pid = child.id().unwrap();
        let reached = timeout(DEADLINE, async {
            loop {
                if let Ok(bytes) = tokio::fs::read(&marker).await {
                    if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                        return Ok(value);
                    }
                }
                if let Some(status) = child.try_wait().unwrap() {
                    return Err(status);
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        let reached = match reached {
            Ok(Ok(value)) => value,
            outcome => {
                let failure = match outcome {
                    Ok(Err(status)) => format!("exited early: {status}"),
                    Err(_) => {
                        timeout(DEADLINE, child.kill()).await.unwrap().unwrap();
                        "checkpoint timed out".to_string()
                    }
                    Ok(Ok(_)) => unreachable!(),
                };
                let output = timeout(DEADLINE, child.wait_with_output())
                    .await
                    .unwrap()
                    .unwrap();
                panic!(
                    "crash worker {point} {failure}\nstdout: {}\nstderr: {}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        };
        assert_eq!(reached["point"], point);
        assert_eq!(reached["pid"], pid);
        timeout(DEADLINE, child.kill()).await.unwrap().unwrap();
        assert_eq!(
            child.wait().await.unwrap().signal(),
            Some(9),
            "must actually use SIGKILL"
        );
    }
}

async fn open_read_only(path: &Path) -> SqliteConnection {
    SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path)
            .read_only(true)
            .foreign_keys(true),
    )
    .await
    .unwrap()
}

async fn assert_source(path: &Path, expected_version: i64) {
    let mut connection = open_read_only(path).await;
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut connection)
        .await
        .unwrap();
    assert_eq!(integrity, "ok");
    let foreign_keys = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&mut connection)
        .await
        .unwrap();
    assert!(foreign_keys.is_empty());
    let version: i64 = sqlx::query_scalar("SELECT MAX(version) FROM client_schema_migrations")
        .fetch_one(&mut connection)
        .await
        .unwrap();
    assert_eq!(version, expected_version);
    let versions: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM client_schema_migrations ORDER BY version")
            .fetch_all(&mut connection)
            .await
            .unwrap();
    assert_eq!(
        versions,
        (1..=expected_version).collect::<Vec<_>>(),
        "no premature/skipped version markers"
    );
    for (table, expected) in [
        ("local_agent_runs", 1),
        ("local_agent_events", 1),
        ("local_agent_command_receipts", 3),
    ] {
        let count: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                .fetch_one(&mut connection)
                .await
                .unwrap();
        assert_eq!(count, expected, "{table}");
    }
    for (table, expected, introduced) in [
        ("local_agent_command_tombstones", 3, 35),
        ("local_agent_receipt_sizes", 3, 36),
        ("local_agent_receipt_cache_usage", 1, 36),
        ("local_agent_receipt_results", 2, 37),
    ] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?)",
        )
        .bind(table)
        .fetch_one(&mut connection)
        .await
        .unwrap();
        assert_eq!(
            exists,
            expected_version >= introduced,
            "partial schema: {table}"
        );
        if exists {
            let count: i64 =
                sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                    .fetch_one(&mut connection)
                    .await
                    .unwrap();
            assert_eq!(count, expected, "partial rows: {table}");
        }
    }
    if expected_version >= 35 {
        let mismatches: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM local_agent_command_receipts r \
             LEFT JOIN local_agent_command_tombstones t ON t.command_id = r.command_id \
             WHERE t.command_id IS NULL OR t.request_fingerprint != r.request_fingerprint \
               OR t.completed_at_unix_ms != r.created_at_unix_ms",
        )
        .fetch_one(&mut connection)
        .await
        .unwrap();
        assert_eq!(
            mismatches, 0,
            "all committed identities must match receipts"
        );
    }
    if expected_version >= 36 {
        let (bytes, count, budget): (i64, i64, i64) = sqlx::query_as(
            "SELECT response_bytes, response_count, budget_bytes \
             FROM local_agent_receipt_cache_usage WHERE singleton = 1",
        )
        .fetch_one(&mut connection)
        .await
        .unwrap();
        assert_eq!(bytes as usize, expected_response_bytes());
        assert_eq!(count, 3);
        assert_eq!(budget, 128 * 1024 * 1024);
        let sum: i64 =
            sqlx::query_scalar("SELECT SUM(response_bytes) FROM local_agent_receipt_sizes")
                .fetch_one(&mut connection)
                .await
                .unwrap();
        assert_eq!(sum, bytes, "committed accounting must match size metadata");
    }
    for (id, letter) in [("a-large", 'x'), ("z-large", 'y')] {
        let fingerprint: String = sqlx::query_scalar(
            "SELECT request_fingerprint FROM local_agent_command_receipts WHERE command_id=?",
        )
        .bind(id)
        .fetch_one(&mut connection)
        .await
        .unwrap();
        let expected = legacy_fingerprint(id);
        assert_eq!(
            fingerprint,
            if expected_version >= 35 {
                super::receipt_fingerprint::compact_legacy(&expected)
            } else {
                expected
            }
        );
        let json: String = if expected_version >= 37 {
            sqlx::query_scalar("SELECT b.response_json FROM local_agent_command_receipts r JOIN local_agent_receipt_results b ON b.result_digest=r.result_digest WHERE r.command_id=?")
                .bind(id).fetch_one(&mut connection).await.unwrap()
        } else {
            sqlx::query_scalar(
                "SELECT response_json FROM local_agent_command_receipts WHERE command_id=?",
            )
            .bind(id)
            .fetch_one(&mut connection)
            .await
            .unwrap()
        };
        assert_eq!(json, legacy_response(letter));
    }
    connection.close().await.unwrap();
}

fn all_backups(fixture: &Fixture) -> Vec<PathBuf> {
    fs::read_dir(fixture.directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .contains(".backup.sqlite")
        })
        .collect()
}

async fn crash_and_recover(point: &str, committed_version: i64, incomplete: bool) {
    let fixture = Fixture::new().await;
    fixture.kill_at(point).await;
    assert_source(&fixture.database(), committed_version).await;
    fixture.assert_original_backup_unchanged();
    let before = all_backups(&fixture)
        .into_iter()
        .map(|path| {
            let bytes = fs::read(&path).unwrap();
            (path, Sha256::digest(&bytes).to_vec())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        before.len(),
        2,
        "one old recovery point plus this attempt's snapshot"
    );
    assert_eq!(
        before
            .iter()
            .filter(|(path, _)| path.to_string_lossy().ends_with(".incomplete"))
            .count(),
        usize::from(incomplete)
    );
    for (path, _) in &before {
        if !path.to_string_lossy().ends_with(".incomplete") {
            assert_source(path, 34).await;
        }
    }
    let storage = SqliteClientStorage::connect_file(&fixture.database())
        .await
        .unwrap();
    // Replay the already-committed business result without another Run/event.
    assert_eq!(
        storage
            .create_run(
                &business_command(),
                &super::tests::run(1000),
                "must-not-repeat"
            )
            .await
            .unwrap(),
        super::tests::run(1000)
    );
    for (id, letter) in [("a-large", 'x'), ("z-large", 'y')] {
        let mut connection = storage.pool.acquire().await.unwrap();
        let command = IdempotentCommand {
            command_id: id.into(),
            request_fingerprint: legacy_fingerprint(id),
            persist_receipt: true,
        };
        let result: Value = SqliteClientStorage::replay(&mut connection, &command)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            result,
            serde_json::from_str::<Value>(&legacy_response(letter)).unwrap()
        );
    }
    let usage = storage.receipt_cache_usage().await.unwrap();
    assert_eq!(usage.response_count, 3);
    assert_eq!(usage.response_bytes as usize, expected_response_bytes());
    storage.pool.close().await;
    assert_source(&fixture.database(), 37).await;
    for (path, digest) in &before {
        assert_eq!(
            Sha256::digest(fs::read(path).unwrap()).to_vec(),
            *digest,
            "recovery must not alter existing verified or incomplete backups"
        );
    }
    assert_eq!(
        all_backups(&fixture).len(),
        if committed_version < 37 { 3 } else { 2 }
    );
    if committed_version < 37 {
        let retry_backups = all_backups(&fixture)
            .into_iter()
            .filter(|path| !before.iter().any(|(existing, _)| existing == path))
            .collect::<Vec<_>>();
        assert_eq!(retry_backups.len(), 1);
        assert!(!retry_backups[0].to_string_lossy().ends_with(".incomplete"));
        // The retry must back up the latest committed state, which may be v35
        // or v36, rather than copying the earlier v34 recovery point.
        assert_source(&retry_backups[0], committed_version).await;
    }
}

macro_rules! crash_case {
    ($name:ident, $point:literal, $version:literal, $incomplete:literal) => {
        #[tokio::test]
        async fn $name() {
            crash_and_recover($point, $version, $incomplete).await;
        }
    };
}

crash_case!(kill_after_backup_reservation, "backup_reserved", 34, true);
crash_case!(
    kill_after_backup_write_before_verification,
    "backup_written",
    34,
    true
);
crash_case!(
    kill_after_verified_backup_publication,
    "backup_published",
    34,
    false
);
crash_case!(
    kill_during_identity_backfill,
    "identity_row_backfilled",
    34,
    false
);
crash_case!(
    kill_before_v35_commit,
    "schema_v35_before_commit",
    34,
    false
);
crash_case!(kill_after_v35_commit, "schema_v35_after_commit", 35, false);
crash_case!(kill_during_size_backfill, "sizes_backfilled", 35, false);
crash_case!(
    kill_before_v36_commit,
    "schema_v36_before_commit",
    35,
    false
);
crash_case!(kill_after_v36_commit, "schema_v36_after_commit", 36, false);
crash_case!(
    kill_during_result_conversion,
    "result_row_converted",
    36,
    false
);
crash_case!(
    kill_before_v37_commit,
    "schema_v37_before_commit",
    36,
    false
);
crash_case!(kill_after_v37_commit, "schema_v37_after_commit", 37, false);
