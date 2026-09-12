// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    probe_postgres_connection, BootstrapStorageProfile, PostgresBootstrapProfile,
    PostgresConnectionSettings, PostgresCredentials, PostgresEndpoint, PostgresTlsMode,
    SecretReference, SqliteBootstrapProfile, StorageEncryptionKey, StorageError, StorageResult,
    StorageSecretResolver, CLIENT_STORAGE_SCHEMA_VERSION,
};
use chatos_local_agent_protocol::{
    ClientStorageBackendKind, ClientStorageHealth, ClientStorageProfileDescriptor,
    ClientStorageProfileSelection, PostgresConnectionTestResult,
};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{
    LocalAgentPlatformCredentialReader, LocalAgentPlatformDeviceKeyReader,
    LocalAgentStoragePlatform, LocalCapabilityPlatform,
};

const PLATFORM_STATE_VERSION: u32 = 1;
const STAGED_PROFILE_FILE: &str = "staged-storage-profile.json";
const PATH_GRANT_DIRECTORY: &str = "path-grants";
const MAXIMUM_GRANT_BYTES: u64 = 16 * 1024;
const MAXIMUM_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
const MAXIMUM_PLUGIN_EXECUTABLE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NativeLocalAgentStoragePlatformError {
    #[error("local Agent platform state directory is unavailable")]
    StateDirectoryUnavailable,
    #[error("local Agent platform state directory is not private")]
    StateDirectoryNotPrivate,
    #[error("local Agent platform state is invalid")]
    InvalidState,
    #[error("local Agent secure credential is unavailable")]
    CredentialUnavailable,
    #[error("local Agent path grant is unavailable")]
    GrantUnavailable,
    #[error("local Agent archive operation failed")]
    ArchiveOperationFailed,
    #[error("local Agent PostgreSQL profile is invalid")]
    InvalidPostgresProfile,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalAgentPathGrantKind {
    SqliteDatabase,
    ArchiveRead,
    ArchiveWrite,
    PluginExecutable,
}

/// Private, native-created path authority consumed by the Rust Host. The
/// caller sends only `reference` over IPC; absolute paths never leave the
/// local platform state directory.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentPathGrant {
    pub version: u32,
    pub reference: String,
    pub kind: LocalAgentPathGrantKind,
    pub path: PathBuf,
}

impl fmt::Debug for LocalAgentPathGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LocalAgentPathGrant")
            .field("version", &self.version)
            .field("reference", &self.reference)
            .field("kind", &self.kind)
            .field("path", &"[LOCAL PATH]")
            .finish()
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct StagedStorageProfile {
    version: u32,
    owner_user_id: String,
    database_reference: Option<String>,
    profile: BootstrapStorageProfile,
}

struct PlatformSecret(Zeroizing<String>);

impl PlatformSecret {
    fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl<'de> Deserialize<'de> for PlatformSecret {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(|value| Self(Zeroizing::new(value)))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PostgresCredentialEnvelope {
    host: String,
    port: u16,
    database: String,
    tls_mode: PostgresCredentialTlsMode,
    username: String,
    password: PlatformSecret,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PostgresCredentialTlsMode {
    Disabled,
    VerifyFull,
}

impl From<PostgresCredentialTlsMode> for PostgresTlsMode {
    fn from(value: PostgresCredentialTlsMode) -> Self {
        match value {
            PostgresCredentialTlsMode::Disabled => Self::Disabled,
            PostgresCredentialTlsMode::VerifyFull => Self::VerifyFull,
        }
    }
}

pub struct NativeLocalAgentStoragePlatform {
    owner_user_id: String,
    state_directory: PathBuf,
    active_profile: BootstrapStorageProfile,
    credentials: Arc<dyn LocalAgentPlatformCredentialReader>,
    device_keys: Arc<dyn LocalAgentPlatformDeviceKeyReader>,
}

impl NativeLocalAgentStoragePlatform {
    pub fn new(
        owner_user_id: impl Into<String>,
        state_directory: impl Into<PathBuf>,
        active_profile: BootstrapStorageProfile,
        credentials: Arc<dyn LocalAgentPlatformCredentialReader>,
        device_keys: Arc<dyn LocalAgentPlatformDeviceKeyReader>,
    ) -> Result<Self, NativeLocalAgentStoragePlatformError> {
        let owner_user_id = owner_user_id.into();
        if !valid_opaque_identity(owner_user_id.as_str()) || active_profile.validate().is_err() {
            return Err(NativeLocalAgentStoragePlatformError::InvalidState);
        }
        let state_directory = state_directory.into();
        validate_private_state_directory(state_directory.as_path())?;
        Ok(Self {
            owner_user_id,
            state_directory,
            active_profile,
            credentials,
            device_keys,
        })
    }

    fn active_descriptor(&self) -> ClientStorageProfileDescriptor {
        match &self.active_profile {
            BootstrapStorageProfile::Sqlite(profile) => ClientStorageProfileDescriptor {
                backend: ClientStorageBackendKind::Sqlite,
                health: ClientStorageHealth::Active,
                sqlite_database_reference: Some(opaque_sqlite_reference(&profile.database_path)),
                postgres_connection_secret_reference: None,
                schema_version: CLIENT_STORAGE_SCHEMA_VERSION,
                last_error_code: None,
            },
            BootstrapStorageProfile::Postgres(profile) => ClientStorageProfileDescriptor {
                backend: ClientStorageBackendKind::Postgres,
                health: ClientStorageHealth::Active,
                sqlite_database_reference: None,
                postgres_connection_secret_reference: Some(
                    profile.connection_secret.as_str().to_string(),
                ),
                schema_version: CLIENT_STORAGE_SCHEMA_VERSION,
                last_error_code: None,
            },
        }
    }

    fn postgres_settings(
        &self,
        reference: &str,
    ) -> Result<PostgresConnectionSettings, NativeLocalAgentStoragePlatformError> {
        if !valid_opaque_identity(reference) {
            return Err(NativeLocalAgentStoragePlatformError::InvalidPostgresProfile);
        }
        let encoded = self
            .credentials
            .read(self.owner_user_id.as_str(), reference)
            .map_err(|_| NativeLocalAgentStoragePlatformError::CredentialUnavailable)?;
        if encoded.is_empty() || encoded.len() > MAXIMUM_GRANT_BYTES as usize {
            return Err(NativeLocalAgentStoragePlatformError::InvalidPostgresProfile);
        }
        let envelope: PostgresCredentialEnvelope = serde_json::from_slice(encoded.as_slice())
            .map_err(|_| NativeLocalAgentStoragePlatformError::InvalidPostgresProfile)?;
        let settings = PostgresConnectionSettings {
            endpoint: PostgresEndpoint {
                host: envelope.host,
                port: envelope.port,
                database: envelope.database,
                tls_mode: envelope.tls_mode.into(),
            },
            credentials: PostgresCredentials::new(
                envelope.username,
                envelope.password.expose().to_string(),
            )
            .map_err(|_| NativeLocalAgentStoragePlatformError::InvalidPostgresProfile)?,
        };
        settings
            .validate()
            .map_err(|_| NativeLocalAgentStoragePlatformError::InvalidPostgresProfile)?;
        Ok(settings)
    }

    fn resolve_path_grant(
        &self,
        reference: &str,
        expected_kind: LocalAgentPathGrantKind,
    ) -> Result<PathBuf, NativeLocalAgentStoragePlatformError> {
        validate_private_state_directory(self.state_directory.as_path())?;
        if !valid_opaque_identity(reference) {
            return Err(NativeLocalAgentStoragePlatformError::GrantUnavailable);
        }
        let grant_directory = self.state_directory.join(PATH_GRANT_DIRECTORY);
        validate_private_state_directory(grant_directory.as_path())?;
        let path = grant_directory.join(path_grant_file_name(reference));
        let mut file = open_regular_bounded_file(path.as_path(), MAXIMUM_GRANT_BYTES)?;
        validate_private_grant_file_metadata(
            &file
                .metadata()
                .map_err(|_| NativeLocalAgentStoragePlatformError::GrantUnavailable)?,
        )?;
        let mut encoded = Vec::with_capacity(
            file.metadata()
                .map_err(|_| NativeLocalAgentStoragePlatformError::GrantUnavailable)?
                .len() as usize,
        );
        file.read_to_end(&mut encoded)
            .map_err(|_| NativeLocalAgentStoragePlatformError::GrantUnavailable)?;
        let grant: LocalAgentPathGrant = serde_json::from_slice(encoded.as_slice())
            .map_err(|_| NativeLocalAgentStoragePlatformError::GrantUnavailable)?;
        if grant.version != PLATFORM_STATE_VERSION
            || grant.reference != reference
            || grant.kind != expected_kind
            || !grant.path.is_absolute()
        {
            return Err(NativeLocalAgentStoragePlatformError::GrantUnavailable);
        }
        Ok(grant.path)
    }

    fn stage_profile_file(
        &self,
        staged: &StagedStorageProfile,
    ) -> Result<(), NativeLocalAgentStoragePlatformError> {
        validate_private_state_directory(self.state_directory.as_path())?;
        let encoded = serde_json::to_vec(staged)
            .map_err(|_| NativeLocalAgentStoragePlatformError::InvalidState)?;
        atomic_write_private(
            self.state_directory.join(STAGED_PROFILE_FILE).as_path(),
            encoded.as_slice(),
        )
    }
}

impl fmt::Debug for NativeLocalAgentStoragePlatform {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeLocalAgentStoragePlatform")
            .field("owner_user_id", &"[OWNER]")
            .field("state_directory", &"[PRIVATE DIRECTORY]")
            .field("active_backend", &self.active_profile.backend())
            .finish()
    }
}

#[async_trait]
impl StorageSecretResolver for NativeLocalAgentStoragePlatform {
    async fn resolve_sqlite_encryption_key(
        &self,
        reference: &SecretReference,
    ) -> StorageResult<StorageEncryptionKey> {
        let key = self
            .device_keys
            .read_device_key(self.owner_user_id.as_str(), reference.as_str())
            .map_err(|_| secure_store_unavailable())?;
        let key = <[u8; 32]>::try_from(key.as_slice()).map_err(|_| secure_store_unavailable())?;
        Ok(StorageEncryptionKey::new(key))
    }

    async fn resolve_postgres(
        &self,
        reference: &SecretReference,
    ) -> StorageResult<PostgresConnectionSettings> {
        self.postgres_settings(reference.as_str())
            .map_err(|_| secure_store_unavailable())
    }
}

#[async_trait]
impl LocalAgentStoragePlatform for NativeLocalAgentStoragePlatform {
    async fn current_profile(&self) -> Result<ClientStorageProfileDescriptor, String> {
        validate_private_state_directory(self.state_directory.as_path())
            .map_err(|error| error.to_string())?;
        Ok(self.active_descriptor())
    }

    async fn test_postgres(
        &self,
        connection_secret_reference: &str,
    ) -> Result<PostgresConnectionTestResult, String> {
        let settings = self
            .postgres_settings(connection_secret_reference)
            .map_err(|error| error.to_string())?;
        let probe = probe_postgres_connection(&settings)
            .await
            .map_err(|error| error.to_string())?;
        Ok(PostgresConnectionTestResult {
            server_version: probe.server_version,
            tls_active: probe.tls_active,
            authentication_ok: probe.authentication_ok,
            transaction_ok: probe.transaction_ok,
            migration_permission_ok: probe.migration_permission_ok,
        })
    }

    async fn stage_profile(
        &self,
        profile: &ClientStorageProfileSelection,
    ) -> Result<ClientStorageProfileDescriptor, String> {
        let (profile, database_reference, descriptor) = match profile {
            ClientStorageProfileSelection::Sqlite {
                database_reference,
                encryption_secret_reference,
            } => {
                let database_path = self
                    .resolve_path_grant(database_reference, LocalAgentPathGrantKind::SqliteDatabase)
                    .map_err(|error| error.to_string())?;
                let key = self
                    .device_keys
                    .read_device_key(
                        self.owner_user_id.as_str(),
                        encryption_secret_reference.as_str(),
                    )
                    .map_err(|_| {
                        NativeLocalAgentStoragePlatformError::CredentialUnavailable.to_string()
                    })?;
                if key.len() != 32 {
                    return Err(
                        NativeLocalAgentStoragePlatformError::CredentialUnavailable.to_string()
                    );
                }
                let secret = SecretReference::new(encryption_secret_reference.clone())
                    .map_err(|_| NativeLocalAgentStoragePlatformError::InvalidState.to_string())?;
                let profile = BootstrapStorageProfile::Sqlite(SqliteBootstrapProfile {
                    database_path,
                    encryption_secret: secret,
                });
                let descriptor = ClientStorageProfileDescriptor {
                    backend: ClientStorageBackendKind::Sqlite,
                    health: ClientStorageHealth::RestartRequired,
                    sqlite_database_reference: Some(database_reference.clone()),
                    postgres_connection_secret_reference: None,
                    schema_version: CLIENT_STORAGE_SCHEMA_VERSION,
                    last_error_code: None,
                };
                (profile, Some(database_reference.clone()), descriptor)
            }
            ClientStorageProfileSelection::Postgres {
                connection_secret_reference,
            } => {
                let settings = self
                    .postgres_settings(connection_secret_reference)
                    .map_err(|error| error.to_string())?;
                probe_postgres_connection(&settings)
                    .await
                    .map_err(|error| error.to_string())?;
                let secret = SecretReference::new(connection_secret_reference.clone())
                    .map_err(|_| NativeLocalAgentStoragePlatformError::InvalidState.to_string())?;
                let profile = BootstrapStorageProfile::Postgres(PostgresBootstrapProfile {
                    connection_secret: secret,
                });
                let descriptor = ClientStorageProfileDescriptor {
                    backend: ClientStorageBackendKind::Postgres,
                    health: ClientStorageHealth::RestartRequired,
                    sqlite_database_reference: None,
                    postgres_connection_secret_reference: Some(connection_secret_reference.clone()),
                    schema_version: CLIENT_STORAGE_SCHEMA_VERSION,
                    last_error_code: None,
                };
                (profile, None, descriptor)
            }
        };
        profile
            .validate()
            .map_err(|_| NativeLocalAgentStoragePlatformError::InvalidState.to_string())?;
        self.stage_profile_file(&StagedStorageProfile {
            version: PLATFORM_STATE_VERSION,
            owner_user_id: self.owner_user_id.clone(),
            database_reference,
            profile,
        })
        .map_err(|error| error.to_string())?;
        Ok(descriptor)
    }

    async fn write_archive(
        &self,
        destination_reference: &str,
        archive: &[u8],
    ) -> Result<String, String> {
        if archive.is_empty() || archive.len() > MAXIMUM_ARCHIVE_BYTES as usize {
            return Err(NativeLocalAgentStoragePlatformError::ArchiveOperationFailed.to_string());
        }
        let path = self
            .resolve_path_grant(destination_reference, LocalAgentPathGrantKind::ArchiveWrite)
            .map_err(|error| error.to_string())?;
        reject_existing_reparse_or_non_file(path.as_path()).map_err(|error| error.to_string())?;
        atomic_write_private(path.as_path(), archive).map_err(|error| error.to_string())?;
        Ok(destination_reference.to_string())
    }

    async fn read_archive(&self, source_reference: &str) -> Result<Vec<u8>, String> {
        let path = self
            .resolve_path_grant(source_reference, LocalAgentPathGrantKind::ArchiveRead)
            .map_err(|error| error.to_string())?;
        let mut file = open_regular_bounded_file(path.as_path(), MAXIMUM_ARCHIVE_BYTES)
            .map_err(|error| error.to_string())?;
        let expected = file
            .metadata()
            .map_err(|_| NativeLocalAgentStoragePlatformError::ArchiveOperationFailed.to_string())?
            .len() as usize;
        let mut archive = Vec::with_capacity(expected);
        file.read_to_end(&mut archive).map_err(|_| {
            NativeLocalAgentStoragePlatformError::ArchiveOperationFailed.to_string()
        })?;
        if archive.len() != expected {
            return Err(NativeLocalAgentStoragePlatformError::ArchiveOperationFailed.to_string());
        }
        Ok(archive)
    }
}

#[async_trait]
impl LocalCapabilityPlatform for NativeLocalAgentStoragePlatform {
    async fn resolve_plugin_executable(
        &self,
        reference: &str,
        expected_sha256: &str,
    ) -> Result<PathBuf, String> {
        let path = self
            .resolve_path_grant(reference, LocalAgentPathGrantKind::PluginExecutable)
            .map_err(|error| error.to_string())?;
        validate_plugin_executable(path.as_path(), expected_sha256)
            .map_err(|error| error.to_string())?;
        Ok(path)
    }

    async fn resolve_plugin_environment_secret(&self, reference: &str) -> Result<String, String> {
        if !valid_opaque_identity(reference) {
            return Err(NativeLocalAgentStoragePlatformError::CredentialUnavailable.to_string());
        }
        let value = self
            .credentials
            .read(self.owner_user_id.as_str(), reference)
            .map_err(|_| NativeLocalAgentStoragePlatformError::CredentialUnavailable.to_string())?;
        if value.is_empty() || value.len() > MAXIMUM_GRANT_BYTES as usize || value.contains(&0) {
            return Err(NativeLocalAgentStoragePlatformError::CredentialUnavailable.to_string());
        }
        String::from_utf8(value.to_vec())
            .map_err(|_| NativeLocalAgentStoragePlatformError::CredentialUnavailable.to_string())
    }
}

pub fn path_grant_file_name(reference: &str) -> String {
    format!("{:x}.json", Sha256::digest(reference.as_bytes()))
}

fn opaque_sqlite_reference(path: &Path) -> String {
    format!(
        "sqlite-{:x}",
        Sha256::digest(path.as_os_str().to_string_lossy().as_bytes())
    )
}

fn valid_opaque_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value.trim() == value
        && !value.chars().any(char::is_control)
        && !value.contains('/')
        && !value.contains('\\')
}

fn secure_store_unavailable() -> StorageError {
    StorageError::Unavailable {
        reason: "platform secure storage could not resolve the selected client storage profile"
            .to_string(),
    }
}

fn open_regular_bounded_file(
    path: &Path,
    maximum_bytes: u64,
) -> Result<File, NativeLocalAgentStoragePlatformError> {
    reject_existing_reparse_or_non_file(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    }
    let file = options
        .open(path)
        .map_err(|_| NativeLocalAgentStoragePlatformError::GrantUnavailable)?;
    let metadata = file
        .metadata()
        .map_err(|_| NativeLocalAgentStoragePlatformError::GrantUnavailable)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > maximum_bytes {
        return Err(NativeLocalAgentStoragePlatformError::GrantUnavailable);
    }
    Ok(file)
}

fn validate_plugin_executable(
    path: &Path,
    expected_sha256: &str,
) -> Result<(), NativeLocalAgentStoragePlatformError> {
    if expected_sha256.len() != 64
        || !expected_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(NativeLocalAgentStoragePlatformError::GrantUnavailable);
    }
    let mut file = open_regular_bounded_file(path, MAXIMUM_PLUGIN_EXECUTABLE_BYTES)?;
    validate_platform_executable(
        path,
        &file
            .metadata()
            .map_err(|_| NativeLocalAgentStoragePlatformError::GrantUnavailable)?,
    )?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| NativeLocalAgentStoragePlatformError::GrantUnavailable)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    if format!("{:x}", digest.finalize()) != expected_sha256 {
        return Err(NativeLocalAgentStoragePlatformError::GrantUnavailable);
    }
    Ok(())
}

#[cfg(unix)]
fn validate_platform_executable(
    _path: &Path,
    metadata: &fs::Metadata,
) -> Result<(), NativeLocalAgentStoragePlatformError> {
    use std::os::unix::fs::PermissionsExt;

    if metadata.permissions().mode() & 0o111 == 0 {
        Err(NativeLocalAgentStoragePlatformError::GrantUnavailable)
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn validate_platform_executable(
    path: &Path,
    metadata: &fs::Metadata,
) -> Result<(), NativeLocalAgentStoragePlatformError> {
    reject_windows_reparse_metadata(metadata)?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    if matches!(extension.as_deref(), Some("exe" | "com" | "cmd" | "bat")) {
        Ok(())
    } else {
        Err(NativeLocalAgentStoragePlatformError::GrantUnavailable)
    }
}

fn reject_existing_reparse_or_non_file(
    path: &Path,
) -> Result<(), NativeLocalAgentStoragePlatformError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(NativeLocalAgentStoragePlatformError::GrantUnavailable)
        }
        Ok(metadata) => reject_windows_reparse_metadata(&metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(NativeLocalAgentStoragePlatformError::GrantUnavailable),
    }
}

#[cfg(windows)]
fn reject_windows_reparse_metadata(
    metadata: &fs::Metadata,
) -> Result<(), NativeLocalAgentStoragePlatformError> {
    use std::os::windows::fs::MetadataExt;
    use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
        Err(NativeLocalAgentStoragePlatformError::GrantUnavailable)
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn reject_windows_reparse_metadata(
    _metadata: &fs::Metadata,
) -> Result<(), NativeLocalAgentStoragePlatformError> {
    Ok(())
}

fn atomic_write_private(
    path: &Path,
    bytes: &[u8],
) -> Result<(), NativeLocalAgentStoragePlatformError> {
    let parent = path
        .parent()
        .ok_or(NativeLocalAgentStoragePlatformError::ArchiveOperationFailed)?;
    if !parent.is_dir() {
        return Err(NativeLocalAgentStoragePlatformError::ArchiveOperationFailed);
    }
    reject_existing_reparse_or_non_file(path)?;
    let temporary = parent.join(format!(
        ".chatos-write-{}-{}.tmp",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(temporary.as_path())
        .map_err(|_| NativeLocalAgentStoragePlatformError::ArchiveOperationFailed)?;
    let result = (|| {
        file.write_all(bytes)
            .map_err(|_| NativeLocalAgentStoragePlatformError::ArchiveOperationFailed)?;
        file.sync_all()
            .map_err(|_| NativeLocalAgentStoragePlatformError::ArchiveOperationFailed)?;
        drop(file);
        replace_file_atomic(temporary.as_path(), path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(not(windows))]
fn replace_file_atomic(
    temporary: &Path,
    destination: &Path,
) -> Result<(), NativeLocalAgentStoragePlatformError> {
    fs::rename(temporary, destination)
        .map_err(|_| NativeLocalAgentStoragePlatformError::ArchiveOperationFailed)
}

#[cfg(windows)]
fn replace_file_atomic(
    temporary: &Path,
    destination: &Path,
) -> Result<(), NativeLocalAgentStoragePlatformError> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let temporary = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: both UTF-16 buffers are NUL-terminated and remain live for the
    // duration of the call. The source is a newly created file in the same
    // directory, so this is an atomic same-volume replacement.
    unsafe {
        MoveFileExW(
            PCWSTR(temporary.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|_| NativeLocalAgentStoragePlatformError::ArchiveOperationFailed)
}

fn validate_private_state_directory(
    path: &Path,
) -> Result<(), NativeLocalAgentStoragePlatformError> {
    if !path.is_absolute() {
        return Err(NativeLocalAgentStoragePlatformError::StateDirectoryUnavailable);
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| NativeLocalAgentStoragePlatformError::StateDirectoryUnavailable)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(NativeLocalAgentStoragePlatformError::StateDirectoryNotPrivate);
    }
    validate_platform_private_directory_metadata(&metadata)
}

#[cfg(unix)]
fn validate_private_grant_file_metadata(
    metadata: &fs::Metadata,
) -> Result<(), NativeLocalAgentStoragePlatformError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if metadata.uid() != unsafe { libc::geteuid() } || metadata.permissions().mode() & 0o077 != 0 {
        Err(NativeLocalAgentStoragePlatformError::GrantUnavailable)
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn validate_private_grant_file_metadata(
    metadata: &fs::Metadata,
) -> Result<(), NativeLocalAgentStoragePlatformError> {
    reject_windows_reparse_metadata(metadata)
}

#[cfg(unix)]
fn validate_platform_private_directory_metadata(
    metadata: &fs::Metadata,
) -> Result<(), NativeLocalAgentStoragePlatformError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if metadata.uid() != unsafe { libc::geteuid() } || metadata.permissions().mode() & 0o077 != 0 {
        Err(NativeLocalAgentStoragePlatformError::StateDirectoryNotPrivate)
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn validate_platform_private_directory_metadata(
    metadata: &fs::Metadata,
) -> Result<(), NativeLocalAgentStoragePlatformError> {
    use std::os::windows::fs::MetadataExt;
    use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
        Err(NativeLocalAgentStoragePlatformError::StateDirectoryNotPrivate)
    } else {
        Ok(())
    }
}
