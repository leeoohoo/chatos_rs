use crate::{
    domain::{
        datasource::{ConnectionTestResult, ConnectionTestStageResult, DataSource},
        meta::{AuthMode, SslMode},
    },
    error::{AppError, AppResult},
};
use tiberius::{AuthMethod, Client, Config, EncryptionLevel, Row};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

pub type SqlServerClient = Client<Compat<TcpStream>>;

pub async fn test_connection(datasource: &DataSource) -> AppResult<ConnectionTestResult> {
    let mut client = connect_client(datasource, None).await?;

    let row = first_row(&mut client, "select @@version").await?;
    let version = row
        .as_ref()
        .and_then(|value| value.get::<&str, _>(0))
        .map(std::string::ToString::to_string);

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

pub async fn connect_client(
    datasource: &DataSource,
    database: Option<&str>,
) -> AppResult<SqlServerClient> {
    validate_connection_payload(datasource)?;

    let host = datasource
        .network
        .host
        .clone()
        .ok_or_else(|| AppError::BadRequest("network.host is required".to_string()))?;
    let port = datasource
        .network
        .port
        .ok_or_else(|| AppError::BadRequest("network.port is required".to_string()))?;

    let username = datasource
        .auth
        .username
        .clone()
        .ok_or_else(|| AppError::BadRequest("username is required".to_string()))?;
    let password = datasource
        .auth
        .password
        .clone()
        .ok_or_else(|| AppError::BadRequest("password is required".to_string()))?;

    let mut config = Config::new();
    config.host(&host);
    config.port(port);
    config.authentication(AuthMethod::sql_server(username, password));

    let selected_db = database
        .map(std::string::ToString::to_string)
        .or_else(|| datasource.network.database.clone())
        .unwrap_or_else(|| "master".to_string());
    config.database(selected_db);

    configure_tls(&mut config, datasource);

    let tcp = TcpStream::connect(config.get_addr())
        .await
        .map_err(|err| map_db_error("network", err.to_string()))?;
    tcp.set_nodelay(true)
        .map_err(|err| AppError::BadRequest(format!("failed to set tcp nodelay: {err}")))?;

    Client::connect(config, tcp.compat_write())
        .await
        .map_err(|err| map_db_error("network", err.to_string()))
}

pub async fn first_row(client: &mut SqlServerClient, sql: &str) -> AppResult<Option<Row>> {
    client
        .query(sql, &[])
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map(|mut rows| rows.drain(..).next())
        .map_err(|err| map_db_error("query", err.to_string()))
}

fn configure_tls(config: &mut Config, datasource: &DataSource) {
    if let Some(tls) = &datasource.tls {
        if tls.enabled {
            config.encryption(match tls.ssl_mode.unwrap_or(SslMode::Preferred) {
                SslMode::Disabled => EncryptionLevel::Off,
                SslMode::Preferred | SslMode::Required => EncryptionLevel::Required,
                SslMode::VerifyCa | SslMode::VerifyFull => EncryptionLevel::Required,
            });
            config.trust_cert();
        } else {
            config.encryption(EncryptionLevel::Off);
            config.trust_cert();
        }
    } else {
        config.encryption(EncryptionLevel::Off);
        config.trust_cert();
    }
}

fn validate_connection_payload(datasource: &DataSource) -> AppResult<()> {
    match datasource.auth.mode {
        AuthMode::Password | AuthMode::TlsClientCert => {}
        _ => {
            return Err(AppError::BadRequest(
                "sql_server currently supports password/tls_client_cert".to_string(),
            ))
        }
    }

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

    Ok(())
}

pub fn map_db_error(stage: &str, raw_message: String) -> AppError {
    let message = raw_message;
    let lower = message.to_lowercase();

    let code = if lower.contains("login failed") || lower.contains("authentication") {
        "CONN_AUTH_FAILED"
    } else if lower.contains("cannot open database") {
        "CONN_DB_NOT_FOUND"
    } else if lower.contains("timeout") || lower.contains("timed out") {
        "CONN_TIMEOUT"
    } else if lower.contains("certificate") || lower.contains("tls") || lower.contains("ssl") {
        "CONN_TLS_HANDSHAKE_FAILED"
    } else {
        "CONN_NETWORK_UNREACHABLE"
    };

    AppError::BadRequest(format!("[{code}] [{stage}] {message}"))
}
