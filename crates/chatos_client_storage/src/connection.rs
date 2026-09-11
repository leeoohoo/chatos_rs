// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;
use std::net::IpAddr;

use zeroize::Zeroizing;

use crate::{StorageConfigurationError, StorageResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostgresTlsMode {
    /// Allowed only for loopback development databases.
    Disabled,
    /// Encrypt the channel and verify the server certificate and hostname.
    VerifyFull,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresEndpoint {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub tls_mode: PostgresTlsMode,
}

impl PostgresEndpoint {
    pub fn validate(&self) -> StorageResult<()> {
        require_non_empty("host", &self.host)?;
        require_non_empty("database", &self.database)?;
        if self.port == 0 {
            return Err(StorageConfigurationError::InvalidPostgresPort);
        }
        if self.tls_mode == PostgresTlsMode::Disabled && !is_loopback_host(&self.host) {
            return Err(StorageConfigurationError::TlsRequiredForRemotePostgres);
        }
        Ok(())
    }
}

/// Resolved credentials live only for the duration of opening a connection.
/// The password is zeroized on drop and deliberately omitted from `Debug`.
pub struct PostgresCredentials {
    username: String,
    password: Zeroizing<String>,
}

impl PostgresCredentials {
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> StorageResult<Self> {
        let credentials = Self {
            username: username.into(),
            password: Zeroizing::new(password.into()),
        };
        require_non_empty("username", &credentials.username)?;
        require_non_empty("password", credentials.password.as_str())?;
        Ok(credentials)
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn expose_password(&self) -> &str {
        self.password.as_str()
    }
}

impl fmt::Debug for PostgresCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresCredentials")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

/// Fully resolved connection material. This type is intentionally not
/// serializable so callers cannot accidentally persist it in client data.
pub struct PostgresConnectionSettings {
    pub endpoint: PostgresEndpoint,
    pub credentials: PostgresCredentials,
}

impl PostgresConnectionSettings {
    pub fn validate(&self) -> StorageResult<()> {
        self.endpoint.validate()?;
        require_non_empty("username", self.credentials.username())?;
        require_non_empty("password", self.credentials.expose_password())
    }
}

impl fmt::Debug for PostgresConnectionSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresConnectionSettings")
            .field("endpoint", &self.endpoint)
            .field("credentials", &self.credentials)
            .finish()
    }
}

fn require_non_empty(field: &'static str, value: &str) -> StorageResult<()> {
    if value.trim().is_empty() {
        return Err(StorageConfigurationError::EmptyField { field });
    }
    Ok(())
}

fn is_loopback_host(host: &str) -> bool {
    let normalized = host.trim().trim_matches(['[', ']']);
    normalized.eq_ignore_ascii_case("localhost")
        || normalized
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_postgres_requires_verified_tls() {
        let endpoint = PostgresEndpoint {
            host: "db.example.com".to_string(),
            port: 5432,
            database: "chatos".to_string(),
            tls_mode: PostgresTlsMode::Disabled,
        };

        assert_eq!(
            endpoint.validate(),
            Err(StorageConfigurationError::TlsRequiredForRemotePostgres)
        );
    }

    #[test]
    fn loopback_postgres_may_disable_tls() {
        for host in ["localhost", "127.0.0.1", "::1", "[::1]"] {
            let endpoint = PostgresEndpoint {
                host: host.to_string(),
                port: 5432,
                database: "chatos".to_string(),
                tls_mode: PostgresTlsMode::Disabled,
            };
            assert_eq!(endpoint.validate(), Ok(()), "host: {host}");
        }
    }

    #[test]
    fn debug_output_redacts_password() {
        let credentials = PostgresCredentials::new("chatos", "do-not-log-this").unwrap();
        let rendered = format!("{credentials:?}");

        assert!(rendered.contains("[REDACTED]"));
        assert!(!rendered.contains("do-not-log-this"));
    }
}
