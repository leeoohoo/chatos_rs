// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use crate::PostgresError;

#[derive(Debug, Clone)]
pub struct PostgresConfig {
    pub database_url: String,
    pub application_name: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout: Duration,
    pub idle_timeout: Duration,
    pub max_lifetime: Duration,
    pub statement_timeout: Duration,
    pub lock_timeout: Duration,
    pub idle_in_transaction_session_timeout: Duration,
}

impl PostgresConfig {
    pub fn new(database_url: impl Into<String>) -> Result<Self, PostgresError> {
        let database_url = database_url.into();
        let trimmed = database_url.trim();
        if !(trimmed.starts_with("postgres://") || trimmed.starts_with("postgresql://")) {
            return Err(PostgresError::Configuration(
                "database URL must use postgres:// or postgresql://".to_string(),
            ));
        }
        Ok(Self {
            database_url: trimmed.to_string(),
            application_name: "chatos".to_string(),
            max_connections: 10,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(10 * 60),
            max_lifetime: Duration::from_secs(30 * 60),
            statement_timeout: Duration::from_secs(30),
            lock_timeout: Duration::from_secs(5),
            idle_in_transaction_session_timeout: Duration::from_secs(30),
        })
    }

    pub fn from_env(
        database_url: impl Into<String>,
        application_name: impl Into<String>,
        env_prefix: &str,
    ) -> Result<Self, PostgresError> {
        let mut config = Self::new(database_url)?.with_application_name(application_name)?;
        config.max_connections = env_u32(env_prefix, "POOL_MAX_CONNECTIONS")?;
        config.min_connections = env_u32(env_prefix, "POOL_MIN_CONNECTIONS")?;
        config.acquire_timeout = env_duration(env_prefix, "POOL_ACQUIRE_TIMEOUT_MS")?;
        config.idle_timeout = env_duration(env_prefix, "POOL_IDLE_TIMEOUT_MS")?;
        config.max_lifetime = env_duration(env_prefix, "POOL_MAX_LIFETIME_MS")?;
        config.statement_timeout = env_duration(env_prefix, "STATEMENT_TIMEOUT_MS")?;
        config.lock_timeout = env_duration(env_prefix, "LOCK_TIMEOUT_MS")?;
        config.validate()?;
        Ok(config)
    }

    pub fn with_application_name(
        mut self,
        application_name: impl Into<String>,
    ) -> Result<Self, PostgresError> {
        let application_name = application_name.into();
        let application_name = application_name.trim();
        if application_name.is_empty() || application_name.len() > 63 {
            return Err(PostgresError::Configuration(
                "application_name must contain 1 to 63 characters".to_string(),
            ));
        }
        self.application_name = application_name.to_string();
        Ok(self)
    }

    pub fn validate(&self) -> Result<(), PostgresError> {
        if self.max_connections == 0 {
            return Err(PostgresError::Configuration(
                "max_connections must be greater than zero".to_string(),
            ));
        }
        if self.min_connections > self.max_connections {
            return Err(PostgresError::Configuration(
                "min_connections cannot exceed max_connections".to_string(),
            ));
        }
        if self.application_name.trim().is_empty() || self.application_name.len() > 63 {
            return Err(PostgresError::Configuration(
                "application_name must contain 1 to 63 characters".to_string(),
            ));
        }
        for (name, value) in [
            ("acquire_timeout", self.acquire_timeout),
            ("idle_timeout", self.idle_timeout),
            ("max_lifetime", self.max_lifetime),
            ("statement_timeout", self.statement_timeout),
            ("lock_timeout", self.lock_timeout),
        ] {
            if value.is_zero() {
                return Err(PostgresError::Configuration(format!(
                    "{name} must be greater than zero"
                )));
            }
        }
        Ok(())
    }
}

fn env_u32(prefix: &str, suffix: &str) -> Result<u32, PostgresError> {
    let key = env_key(prefix, suffix);
    let value = required_env(&key)?;
    value.parse::<u32>().map_err(|error| {
        PostgresError::Configuration(format!("{key} must be an unsigned integer: {error}"))
    })
}

fn env_duration(prefix: &str, suffix: &str) -> Result<Duration, PostgresError> {
    let key = env_key(prefix, suffix);
    let millis = required_env(&key)?.parse::<u64>().map_err(|error| {
        PostgresError::Configuration(format!("{key} must be milliseconds: {error}"))
    })?;
    Ok(Duration::from_millis(millis))
}

fn env_key(prefix: &str, suffix: &str) -> String {
    format!("{}_POSTGRES_{suffix}", prefix.trim().to_ascii_uppercase())
}

fn required_env(key: &str) -> Result<String, PostgresError> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            PostgresError::Configuration(format!("{key} is required from Configuration Center"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_postgres_urls() {
        assert!(PostgresConfig::new("mysql://localhost/chatos").is_err());
    }

    #[test]
    fn applies_pool_defaults() {
        let config = PostgresConfig::new("postgresql://localhost/chatos").expect("config");
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.min_connections, 1);
        assert_eq!(config.acquire_timeout, Duration::from_secs(5));
        assert_eq!(config.application_name, "chatos");
    }

    #[test]
    fn validates_application_name_and_timeouts() {
        let config = PostgresConfig::new("postgresql://localhost/chatos")
            .expect("config")
            .with_application_name("task-runner-worker")
            .expect("application name");
        assert_eq!(config.application_name, "task-runner-worker");
        assert!(config.validate().is_ok());

        let mut invalid = config;
        invalid.lock_timeout = Duration::ZERO;
        assert!(invalid.validate().is_err());
    }
}
