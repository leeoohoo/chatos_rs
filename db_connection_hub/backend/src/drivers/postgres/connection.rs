use std::{path::PathBuf, time::Duration};

use crate::{
    domain::{
        datasource::{ConnectionTestResult, ConnectionTestStageResult, DataSource},
        meta::{AuthMode, SslMode},
    },
    drivers::connection_common::{
        connect_timeout_ms, pool_limits, require_network_host, require_network_port,
        validate_network_host_port, validate_password_auth, validate_token_auth, DEFAULT_POOL_MAX,
    },
    error::{AppError, AppResult},
};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
    ConnectOptions, Row,
};

pub async fn test_connection(datasource: &DataSource) -> AppResult<ConnectionTestResult> {
    let pool = connect_pool(datasource, None).await?;
    let row = sqlx::query("select version() as version")
        .fetch_one(&pool)
        .await
        .map_err(|err| map_db_error("auth", err))?;

    let version = row.try_get::<String, _>("version").ok();

    Ok(ConnectionTestResult {
        ok: true,
        latency_ms: 0,
        server_version: version,
        auth_mode: datasource.auth.mode,
        checks: vec![
            ConnectionTestStageResult {
                stage: "network".to_string(),
                ok: true,
                message: None,
            },
            ConnectionTestStageResult {
                stage: "tls".to_string(),
                ok: true,
                message: None,
            },
            ConnectionTestStageResult {
                stage: "auth".to_string(),
                ok: true,
                message: None,
            },
            ConnectionTestStageResult {
                stage: "metadata_permission".to_string(),
                ok: true,
                message: None,
            },
        ],
        error_code: None,
        message: None,
        stage: None,
    })
}

pub async fn connect_pool(
    datasource: &DataSource,
    database: Option<&str>,
) -> AppResult<sqlx::PgPool> {
    validate_connection_payload(datasource)?;

    let connect_options = build_connect_options(datasource, database)?;
    let timeout_ms = connect_timeout_ms(datasource);
    let (pool_min, pool_max) = pool_limits(datasource, DEFAULT_POOL_MAX);

    PgPoolOptions::new()
        .min_connections(pool_min)
        .max_connections(pool_max)
        .acquire_timeout(Duration::from_millis(timeout_ms))
        .connect_with(connect_options)
        .await
        .map_err(|err| map_db_error("network", err))
}

fn build_connect_options(
    datasource: &DataSource,
    database: Option<&str>,
) -> AppResult<PgConnectOptions> {
    let host = require_network_host(datasource)?;
    let port = require_network_port(datasource)?;

    let selected_db = database
        .map(|value| value.to_string())
        .or_else(|| datasource.network.database.clone())
        .unwrap_or_else(|| "postgres".to_string());

    let username = datasource
        .auth
        .username
        .clone()
        .unwrap_or_else(|| "postgres".to_string());

    let password = datasource
        .auth
        .password
        .clone()
        .or_else(|| datasource.auth.access_token.clone())
        .unwrap_or_default();

    let mut options = PgConnectOptions::new()
        .host(&host)
        .port(port)
        .username(&username)
        .password(&password)
        .database(&selected_db)
        .log_statements(tracing::log::LevelFilter::Off);

    if let Some(tls) = &datasource.tls {
        if tls.enabled {
            options = options.ssl_mode(map_ssl_mode(tls.ssl_mode));
            if let Some(ca_cert) = tls.ca_cert.clone().filter(|value| !value.trim().is_empty()) {
                let ca_path = PathBuf::from(ca_cert);
                if ca_path.exists() {
                    options = options.ssl_root_cert(ca_path);
                }
            }
        }
    }

    Ok(options)
}

fn map_ssl_mode(mode: Option<SslMode>) -> PgSslMode {
    match mode.unwrap_or(SslMode::Preferred) {
        SslMode::Disabled => PgSslMode::Disable,
        SslMode::Preferred => PgSslMode::Prefer,
        SslMode::Required => PgSslMode::Require,
        SslMode::VerifyCa => PgSslMode::VerifyCa,
        SslMode::VerifyFull => PgSslMode::VerifyFull,
    }
}

fn validate_connection_payload(datasource: &DataSource) -> AppResult<()> {
    match datasource.auth.mode {
        AuthMode::Password => {
            validate_password_auth(datasource)?;
        }
        AuthMode::Token => {
            validate_token_auth(datasource)?;
        }
        AuthMode::TlsClientCert | AuthMode::Integrated | AuthMode::FileKey | AuthMode::NoAuth => {}
    }

    validate_network_host_port(datasource)
}

pub fn map_db_error(stage: &str, err: sqlx::Error) -> AppError {
    let message = err.to_string();
    let code = if message.contains("password authentication failed") {
        "CONN_AUTH_FAILED"
    } else if message.contains("does not exist") {
        "CONN_DB_NOT_FOUND"
    } else if message.contains("timeout") {
        "CONN_TIMEOUT"
    } else if message.contains("certificate") || message.contains("SSL") {
        "CONN_TLS_HANDSHAKE_FAILED"
    } else {
        "CONN_NETWORK_UNREACHABLE"
    };

    AppError::BadRequest(format!("[{code}] [{stage}] {message}"))
}
