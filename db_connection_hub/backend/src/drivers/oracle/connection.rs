use std::time::Duration;

use tokio::{net::TcpStream, time::timeout};

use crate::{
    domain::{
        datasource::{ConnectionTestResult, ConnectionTestStageResult, DataSource},
        meta::AuthMode,
    },
    error::{AppError, AppResult},
};

pub async fn test_connection(datasource: &DataSource) -> AppResult<ConnectionTestResult> {
    validate_connection_payload(datasource)?;
    probe_tcp(datasource).await?;

    Ok(ConnectionTestResult {
        ok: true,
        latency_ms: 0,
        server_version: None,
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
                message: datasource.tls.as_ref().and_then(|tls| {
                    if tls.enabled {
                        Some("oracle tls parameters accepted in this stage".to_string())
                    } else {
                        None
                    }
                }),
            },
            ConnectionTestStageResult {
                stage: "auth".to_string(),
                ok: true,
                message: Some("oracle first-stage driver validates auth payload shape".to_string()),
            },
            ConnectionTestStageResult {
                stage: "metadata_permission".to_string(),
                ok: true,
                message: Some("metadata is currently partial for oracle".to_string()),
            },
        ],
        error_code: None,
        message: None,
        stage: None,
    })
}

pub async fn probe_tcp(datasource: &DataSource) -> AppResult<()> {
    let host = datasource
        .network
        .host
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("network.host is required".to_string()))?;
    let port = datasource
        .network
        .port
        .ok_or_else(|| AppError::BadRequest("network.port is required".to_string()))?;
    let connect_timeout_ms = datasource.options.connect_timeout_ms.unwrap_or(5_000);
    let address = format!("{host}:{port}");

    timeout(
        Duration::from_millis(connect_timeout_ms),
        TcpStream::connect(address),
    )
    .await
    .map_err(|_| map_db_error("network", "oracle tcp connect timeout".to_string()))?
    .map(|_| ())
    .map_err(|err| map_db_error("network", err.to_string()))
}

fn validate_connection_payload(datasource: &DataSource) -> AppResult<()> {
    if datasource
        .network
        .host
        .as_deref()
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return Err(AppError::BadRequest("network.host is required".to_string()));
    }

    if datasource.network.port.is_none() {
        return Err(AppError::BadRequest("network.port is required".to_string()));
    }

    let has_target = datasource
        .network
        .database
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        || datasource
            .network
            .service_name
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || datasource
            .network
            .sid
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
    if !has_target {
        return Err(AppError::BadRequest(
            "oracle requires database or service_name or sid".to_string(),
        ));
    }

    match datasource.auth.mode {
        AuthMode::Password => {
            if datasource
                .auth
                .username
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
            {
                return Err(AppError::BadRequest("username is required".to_string()));
            }
            if datasource
                .auth
                .password
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
            {
                return Err(AppError::BadRequest("password is required".to_string()));
            }
        }
        AuthMode::TlsClientCert => {
            if datasource
                .auth
                .client_cert
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
            {
                return Err(AppError::BadRequest("client_cert is required".to_string()));
            }
            if datasource
                .auth
                .client_key
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
            {
                return Err(AppError::BadRequest("client_key is required".to_string()));
            }
        }
        AuthMode::FileKey => {
            let has_ref = datasource
                .auth
                .key_ref
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
                || datasource
                    .auth
                    .wallet_ref
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty());
            if !has_ref {
                return Err(AppError::BadRequest(
                    "key_ref or wallet_ref is required".to_string(),
                ));
            }
        }
        AuthMode::Integrated => {}
        _ => {
            return Err(AppError::BadRequest(
                "oracle currently supports password/tls_client_cert/file_key/integrated"
                    .to_string(),
            ));
        }
    }

    Ok(())
}

pub fn map_db_error(stage: &str, raw_message: String) -> AppError {
    let message = raw_message;
    let lower = message.to_lowercase();

    let code = if lower.contains("timeout") || lower.contains("timed out") {
        "CONN_TIMEOUT"
    } else if lower.contains("tls") || lower.contains("ssl") || lower.contains("certificate") {
        "CONN_TLS_HANDSHAKE_FAILED"
    } else if lower.contains("auth") || lower.contains("credential") {
        "CONN_AUTH_FAILED"
    } else {
        "CONN_NETWORK_UNREACHABLE"
    };

    AppError::BadRequest(format!("[{code}] [{stage}] {message}"))
}

#[cfg(test)]
mod tests {
    use super::validate_connection_payload;
    use chrono::Utc;

    use crate::{
        domain::{
            datasource::{
                AuthConfig, ConnectionStatus, DataSource, DataSourceOptions, NetworkConfig,
            },
            meta::{AuthMode, DbType, NetworkMode},
        },
        error::AppError,
    };

    #[test]
    fn oracle_target_can_be_service_name() {
        let mut datasource = sample_datasource();
        datasource.network.database = None;
        datasource.network.service_name = Some("orclpdb1".to_string());
        datasource.network.sid = None;

        let result = validate_connection_payload(&datasource);
        assert!(result.is_ok());
    }

    #[test]
    fn oracle_target_can_be_sid() {
        let mut datasource = sample_datasource();
        datasource.network.database = None;
        datasource.network.service_name = None;
        datasource.network.sid = Some("ORCL".to_string());

        let result = validate_connection_payload(&datasource);
        assert!(result.is_ok());
    }

    #[test]
    fn oracle_requires_target_identifier() {
        let mut datasource = sample_datasource();
        datasource.network.database = None;
        datasource.network.service_name = Some("  ".to_string());
        datasource.network.sid = None;

        let result = validate_connection_payload(&datasource);
        match result {
            Err(AppError::BadRequest(message)) => {
                assert!(message.contains("oracle requires database or service_name or sid"));
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }

    fn sample_datasource() -> DataSource {
        DataSource {
            id: "oracle-test-id".to_string(),
            name: "oracle-test".to_string(),
            db_type: DbType::Oracle,
            network: NetworkConfig {
                mode: NetworkMode::Direct,
                host: Some("127.0.0.1".to_string()),
                port: Some(1521),
                database: Some("orclpdb1".to_string()),
                service_name: None,
                sid: None,
                file_path: None,
                ssh: None,
            },
            auth: AuthConfig {
                mode: AuthMode::Password,
                username: Some("system".to_string()),
                password: Some("secret".to_string()),
                access_token: None,
                client_cert: None,
                client_key: None,
                key_ref: None,
                wallet_ref: None,
                principal: None,
                realm: None,
                kdc: None,
                service_name: None,
            },
            tls: None,
            options: DataSourceOptions {
                connect_timeout_ms: Some(5_000),
                statement_timeout_ms: Some(15_000),
                pool_min: Some(1),
                pool_max: Some(20),
            },
            tags: vec!["oracle".to_string()],
            status: ConnectionStatus::Unknown,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_test: None,
        }
    }
}
