// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;

use chatos_client_storage::{
    BootstrapStorageProfile, PostgresBootstrapProfile, SecretReference, SqliteBootstrapProfile,
    StorageBackend, StorageConfigurationError,
};

#[test]
fn sqlite_profile_is_the_default_backend_shape() {
    let profile = BootstrapStorageProfile::Sqlite(SqliteBootstrapProfile {
        database_path: PathBuf::from("/tmp/chatos/client.sqlite3"),
        encryption_secret: SecretReference::new("keychain:client-storage/sqlite-key").unwrap(),
    });

    assert_eq!(profile.backend(), StorageBackend::Sqlite);
    assert_eq!(profile.validate(), Ok(()));
    assert_eq!(
        serde_json::to_value(&profile).unwrap(),
        serde_json::json!({
            "backend": "sqlite",
            "database_path": "/tmp/chatos/client.sqlite3",
            "encryption_secret": "keychain:client-storage/sqlite-key"
        })
    );
}

#[test]
fn postgres_profile_persists_only_an_opaque_secret_reference() {
    let profile = BootstrapStorageProfile::Postgres(PostgresBootstrapProfile {
        connection_secret: SecretReference::new("keychain:client-storage/main").unwrap(),
    });

    assert_eq!(profile.backend(), StorageBackend::Postgres);
    assert_eq!(profile.validate(), Ok(()));
    let encoded = serde_json::to_string(&profile).unwrap();
    assert_eq!(
        encoded,
        r#"{"backend":"postgres","connection_secret":"keychain:client-storage/main"}"#
    );
    assert!(!encoded.contains("password"));
    assert!(!encoded.contains("postgres://"));
}

#[test]
fn a_relative_sqlite_path_is_rejected() {
    let path = PathBuf::from("client.sqlite3");
    let profile = BootstrapStorageProfile::Sqlite(SqliteBootstrapProfile {
        database_path: path.clone(),
        encryption_secret: SecretReference::new("keychain:client-storage/sqlite-key").unwrap(),
    });

    assert_eq!(
        profile.validate(),
        Err(StorageConfigurationError::SqlitePathNotAbsolute(path))
    );
}

#[test]
fn profile_deserialization_does_not_accept_secret_fields() {
    let error = serde_json::from_value::<BootstrapStorageProfile>(serde_json::json!({
        "backend": "postgres",
        "connection_secret": "keychain:client-storage/main",
        "password": "must-not-be-here"
    }))
    .unwrap_err();

    assert!(error.to_string().contains("unknown field `password`"));
}
