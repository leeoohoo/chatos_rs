// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

use chatos_client_storage::{
    BootstrapStorageProfile, PostgresTlsMode, SecretReference, SqliteBootstrapProfile,
    StorageSecretResolver,
};
use chatos_local_agent_host::{
    path_grant_file_name, LocalAgentPathGrant, LocalAgentPathGrantKind,
    LocalAgentPlatformCredentialError, LocalAgentPlatformCredentialReader,
    LocalAgentPlatformDeviceKeyReader, LocalAgentStoragePlatform, NativeLocalAgentStoragePlatform,
};
use chatos_local_agent_protocol::{
    ClientStorageBackendKind, ClientStorageHealth, ClientStorageProfileSelection,
};
use zeroize::Zeroizing;

struct Credentials;

impl LocalAgentPlatformCredentialReader for Credentials {
    fn read(
        &self,
        owner_user_id: &str,
        reference: &str,
    ) -> Result<Zeroizing<Vec<u8>>, LocalAgentPlatformCredentialError> {
        if owner_user_id == "platform-user" && reference == "postgres-secret" {
            Ok(Zeroizing::new(
                br#"{"host":"database.example.test","port":5432,"database":"chatos","tls_mode":"verify_full","username":"chatos-user","password":"private-password"}"#.to_vec(),
            ))
        } else {
            Err(LocalAgentPlatformCredentialError::Unavailable)
        }
    }
}

#[tokio::test]
async fn resolves_active_storage_secrets_only_through_platform_readers() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    private_directory(state.as_path());
    let platform = platform(state.as_path(), &root.path().join("active.sqlite3"));

    platform
        .resolve_sqlite_encryption_key(&SecretReference::new("sqlite-key").unwrap())
        .await
        .unwrap();
    assert!(platform
        .resolve_sqlite_encryption_key(&SecretReference::new("wrong-key").unwrap())
        .await
        .is_err());
    let postgres = platform
        .resolve_postgres(&SecretReference::new("postgres-secret").unwrap())
        .await
        .unwrap();
    assert_eq!(postgres.endpoint.port, 5432);
    assert_eq!(postgres.endpoint.tls_mode, PostgresTlsMode::VerifyFull);
    assert!(!format!("{postgres:?}").contains("private-password"));
    assert!(!format!("{postgres:?}").contains("database.example.test"));
}

impl LocalAgentPlatformDeviceKeyReader for Credentials {
    fn read_device_key(
        &self,
        owner_user_id: &str,
        reference: &str,
    ) -> Result<Zeroizing<Vec<u8>>, LocalAgentPlatformCredentialError> {
        if owner_user_id == "platform-user" && reference == "sqlite-key" {
            Ok(Zeroizing::new(vec![7; 32]))
        } else {
            Err(LocalAgentPlatformCredentialError::Unavailable)
        }
    }
}

fn private_directory(path: &std::path::Path) {
    fs::create_dir_all(path).unwrap();
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn grant(
    state_directory: &std::path::Path,
    reference: &str,
    kind: LocalAgentPathGrantKind,
    path: &std::path::Path,
) {
    let directory = state_directory.join("path-grants");
    private_directory(directory.as_path());
    let encoded = serde_json::to_vec(&LocalAgentPathGrant {
        version: 1,
        reference: reference.to_string(),
        kind,
        path: path.to_path_buf(),
    })
    .unwrap();
    let destination = directory.join(path_grant_file_name(reference));
    fs::write(&destination, encoded).unwrap();
    #[cfg(unix)]
    fs::set_permissions(destination, fs::Permissions::from_mode(0o600)).unwrap();
}

fn platform(
    state_directory: &std::path::Path,
    active_database: &std::path::Path,
) -> NativeLocalAgentStoragePlatform {
    let credentials = Arc::new(Credentials);
    NativeLocalAgentStoragePlatform::new(
        "platform-user",
        state_directory,
        BootstrapStorageProfile::Sqlite(SqliteBootstrapProfile {
            database_path: active_database.to_path_buf(),
            encryption_secret: SecretReference::new("sqlite-key").unwrap(),
        }),
        credentials.clone(),
        credentials,
    )
    .unwrap()
}

#[tokio::test]
async fn stages_only_a_non_secret_profile_and_reports_restart_required() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    private_directory(state.as_path());
    let next_database = root.path().join("next.sqlite3");
    grant(
        state.as_path(),
        "next-database",
        LocalAgentPathGrantKind::SqliteDatabase,
        next_database.as_path(),
    );
    let platform = platform(state.as_path(), &root.path().join("active.sqlite3"));

    let active = platform.current_profile().await.unwrap();
    assert_eq!(active.backend, ClientStorageBackendKind::Sqlite);
    assert_eq!(active.health, ClientStorageHealth::Active);
    assert!(!active
        .sqlite_database_reference
        .as_deref()
        .unwrap()
        .contains(root.path().to_string_lossy().as_ref()));

    let staged = platform
        .stage_profile(&ClientStorageProfileSelection::Sqlite {
            database_reference: "next-database".to_string(),
            encryption_secret_reference: "sqlite-key".to_string(),
        })
        .await
        .unwrap();
    assert_eq!(staged.health, ClientStorageHealth::RestartRequired);
    assert_eq!(
        staged.sqlite_database_reference.as_deref(),
        Some("next-database")
    );
    let persisted = fs::read_to_string(state.join("staged-storage-profile.json")).unwrap();
    assert!(persisted.contains("next.sqlite3"));
    assert!(persisted.contains("sqlite-key"));
    assert!(!persisted.contains("BwcHBwcH"));
}

#[tokio::test]
async fn archive_io_uses_distinct_opaque_read_and_write_grants() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    private_directory(state.as_path());
    let archive = root.path().join("client-data.chatos");
    grant(
        state.as_path(),
        "archive-output",
        LocalAgentPathGrantKind::ArchiveWrite,
        archive.as_path(),
    );
    let platform = platform(state.as_path(), &root.path().join("active.sqlite3"));

    assert_eq!(
        platform
            .write_archive("archive-output", b"bounded archive")
            .await
            .unwrap(),
        "archive-output"
    );
    assert_eq!(fs::read(&archive).unwrap(), b"bounded archive");
    assert!(platform.read_archive("archive-output").await.is_err());

    grant(
        state.as_path(),
        "archive-input",
        LocalAgentPathGrantKind::ArchiveRead,
        archive.as_path(),
    );
    assert_eq!(
        platform.read_archive("archive-input").await.unwrap(),
        b"bounded archive"
    );
    assert!(!format!("{platform:?}").contains(root.path().to_string_lossy().as_ref()));
}

#[cfg(unix)]
#[test]
fn rejects_a_state_directory_visible_to_other_users() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    fs::create_dir(&state).unwrap();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o755)).unwrap();
    let credentials = Arc::new(Credentials);
    let result = NativeLocalAgentStoragePlatform::new(
        "platform-user",
        &state,
        BootstrapStorageProfile::Sqlite(SqliteBootstrapProfile {
            database_path: root.path().join("active.sqlite3"),
            encryption_secret: SecretReference::new("sqlite-key").unwrap(),
        }),
        credentials.clone(),
        credentials,
    );
    assert!(result.is_err());
}
