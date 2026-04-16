use crate::{
    domain::{
        datasource::{AuthConfig, DataSource},
        meta::{AuthMode, DbType, SslMode},
    },
    error::{AppError, AppResult},
};

pub fn validate_auth_payload(auth: &AuthConfig) -> AppResult<()> {
    match auth.mode {
        AuthMode::Password => {
            required(auth.username.as_deref(), "username")?;
            required(auth.password.as_deref(), "password")
        }
        AuthMode::TlsClientCert => {
            required(auth.client_cert.as_deref(), "auth.client_cert")?;
            required(auth.client_key.as_deref(), "auth.client_key")
        }
        AuthMode::Token => required(auth.access_token.as_deref(), "access_token"),
        AuthMode::Integrated => Ok(()),
        AuthMode::FileKey => {
            if auth.key_ref.is_none() && auth.wallet_ref.is_none() {
                return Err(AppError::BadRequest(
                    "file_key mode requires key_ref or wallet_ref".to_string(),
                ));
            }
            Ok(())
        }
        AuthMode::NoAuth => Ok(()),
    }
}

pub fn validate_network_payload(datasource: &DataSource) -> AppResult<()> {
    match datasource.db_type {
        DbType::Sqlite => required(datasource.network.file_path.as_deref(), "network.file_path"),
        _ => {
            required(datasource.network.host.as_deref(), "network.host")?;
            if datasource.network.port.is_none() {
                return Err(AppError::BadRequest("network.port is required".to_string()));
            }
            Ok(())
        }
    }
}

pub fn validate_tls_payload(datasource: &DataSource) -> AppResult<()> {
    if let Some(tls) = &datasource.tls {
        if tls.enabled {
            if let Some(mode) = tls.ssl_mode {
                if matches!(mode, SslMode::VerifyCa | SslMode::VerifyFull) {
                    required(tls.ca_cert.as_deref(), "tls.ca_cert")?;
                }
            }
        }
    }

    Ok(())
}

pub fn mock_server_version(db_type: DbType) -> &'static str {
    match db_type {
        DbType::Postgres => "PostgreSQL 16.1",
        DbType::MySql => "MySQL 8.0.35",
        DbType::Sqlite => "SQLite 3.45",
        DbType::SqlServer => "SQL Server 2022",
        DbType::Oracle => "Oracle 21c",
        DbType::MongoDb => "MongoDB 7.0",
    }
}

fn required(value: Option<&str>, field_name: &str) -> AppResult<()> {
    if value.map(|item| item.trim().is_empty()).unwrap_or(true) {
        return Err(AppError::BadRequest(format!("{field_name} is required")));
    }
    Ok(())
}
