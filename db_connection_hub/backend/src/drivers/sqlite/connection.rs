use std::{path::PathBuf, time::Duration};

use crate::{
    domain::{
        datasource::{ConnectionTestResult, ConnectionTestStageResult, DataSource},
        meta::AuthMode,
    },
    drivers::connection_common::{
        connect_timeout_ms, pool_limits, require_sqlite_file_path, validate_supported_auth_mode,
        DEFAULT_SQLITE_POOL_MAX,
    },
    error::{AppError, AppResult},
};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    ConnectOptions, Row,
};

pub async fn test_connection(datasource: &DataSource) -> AppResult<ConnectionTestResult> {
    validate_connection_payload(datasource)?;

    let start = std::time::Instant::now();
    let pool = connect_pool(datasource).await?;
    let row = sqlx::query("select sqlite_version() as version")
        .fetch_one(&pool)
        .await
        .map_err(|err| map_db_error("network", err))?;

    let version = row.try_get::<String, _>("version").ok();

    Ok(ConnectionTestResult {
        ok: true,
        latency_ms: start.elapsed().as_millis() as u64,
        server_version: version,
        auth_mode: datasource.auth.mode,
        checks: vec![
            ConnectionTestStageResult {
                stage: "network".to_string(),
                ok: true,
                message: None,
            },
            ConnectionTestStageResult {
                stage: "auth".to_string(),
                ok: true,
                message: Some("sqlite file mode".to_string()),
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

pub async fn connect_pool(datasource: &DataSource) -> AppResult<sqlx::SqlitePool> {
    validate_connection_payload(datasource)?;

    let path = require_sqlite_file_path(datasource)?;

    let timeout_ms = connect_timeout_ms(datasource);
    let (pool_min, pool_max) = pool_limits(datasource, DEFAULT_SQLITE_POOL_MAX);

    let options = SqliteConnectOptions::new()
        .filename(PathBuf::from(path))
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .log_statements(tracing::log::LevelFilter::Off);

    SqlitePoolOptions::new()
        .min_connections(pool_min)
        .max_connections(pool_max)
        .acquire_timeout(Duration::from_millis(timeout_ms))
        .connect_with(options)
        .await
        .map_err(|err| map_db_error("network", err))
}

fn validate_connection_payload(datasource: &DataSource) -> AppResult<()> {
    validate_supported_auth_mode(
        datasource,
        &[AuthMode::NoAuth, AuthMode::FileKey],
        "sqlite only supports no_auth/file_key in this stage",
    )?;
    require_sqlite_file_path(datasource)?;
    Ok(())
}

pub fn map_db_error(stage: &str, err: sqlx::Error) -> AppError {
    let message = err.to_string();
    let lower = message.to_lowercase();
    let code = if lower.contains("unable to open database file") || lower.contains("no such file") {
        "CONN_DB_NOT_FOUND"
    } else if lower.contains("database is locked") || lower.contains("busy") {
        "CONN_TIMEOUT"
    } else {
        "CONN_NETWORK_UNREACHABLE"
    };

    AppError::BadRequest(format!("[{code}] [{stage}] {message}"))
}
