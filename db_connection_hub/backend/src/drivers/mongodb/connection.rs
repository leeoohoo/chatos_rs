use std::time::Duration;

use mongodb::{
    bson::doc,
    options::{ClientOptions, Credential, Tls, TlsOptions},
    Client,
};

use crate::{
    domain::{
        datasource::{ConnectionTestResult, ConnectionTestStageResult, DataSource},
        meta::AuthMode,
    },
    error::{AppError, AppResult},
};

pub async fn test_connection(datasource: &DataSource) -> AppResult<ConnectionTestResult> {
    let client = connect_client(datasource).await?;
    let db_name = target_database(datasource, None);
    let db = client.database(db_name.as_str());

    db.run_command(doc! { "ping": 1 }, None)
        .await
        .map_err(|err| map_db_error("network", err.to_string()))?;

    let server_version = db
        .run_command(doc! { "buildInfo": 1 }, None)
        .await
        .ok()
        .and_then(|result| {
            result
                .get_str("version")
                .ok()
                .map(std::string::ToString::to_string)
        });

    Ok(ConnectionTestResult {
        ok: true,
        latency_ms: 0,
        server_version,
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

pub async fn connect_client(datasource: &DataSource) -> AppResult<Client> {
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

    let uri = format!("mongodb://{host}:{port}/");
    let mut options = ClientOptions::parse(uri.as_str())
        .await
        .map_err(|err| map_db_error("network", err.to_string()))?;

    options.app_name = Some("db_connection_hub".to_string());
    options.connect_timeout = datasource
        .options
        .connect_timeout_ms
        .map(Duration::from_millis);
    options.server_selection_timeout = datasource
        .options
        .connect_timeout_ms
        .map(Duration::from_millis);
    options.credential = credential_from_datasource(datasource);

    configure_tls(&mut options, datasource);

    Client::with_options(options).map_err(|err| map_db_error("network", err.to_string()))
}

pub fn target_database(datasource: &DataSource, override_database: Option<&str>) -> String {
    override_database
        .map(std::string::ToString::to_string)
        .or_else(|| datasource.network.database.clone())
        .unwrap_or_else(|| "admin".to_string())
}

fn credential_from_datasource(datasource: &DataSource) -> Option<Credential> {
    let auth_db = datasource
        .network
        .database
        .clone()
        .unwrap_or_else(|| "admin".to_string());

    match datasource.auth.mode {
        AuthMode::NoAuth => None,
        AuthMode::Password => Some(
            Credential::builder()
                .username(datasource.auth.username.clone())
                .password(datasource.auth.password.clone())
                .source(Some(auth_db))
                .build(),
        ),
        AuthMode::Token => Some(
            Credential::builder()
                .username(datasource.auth.username.clone())
                .password(datasource.auth.access_token.clone())
                .source(Some(auth_db))
                .build(),
        ),
        AuthMode::TlsClientCert => {
            // For x509 auth, username is optional; MongoDB can derive subject from cert.
            Some(
                Credential::builder()
                    .username(datasource.auth.username.clone())
                    .source(Some("$external".to_string()))
                    .build(),
            )
        }
        _ => None,
    }
}

fn configure_tls(options: &mut ClientOptions, datasource: &DataSource) {
    if let Some(tls) = &datasource.tls {
        if tls.enabled {
            options.tls = Some(Tls::Enabled(TlsOptions::default()));
        } else {
            options.tls = Some(Tls::Disabled);
        }
    } else {
        options.tls = Some(Tls::Disabled);
    }
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

    match datasource.auth.mode {
        AuthMode::NoAuth => {}
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
        AuthMode::Token => {
            if datasource
                .auth
                .access_token
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
            {
                return Err(AppError::BadRequest("access_token is required".to_string()));
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
        _ => {
            return Err(AppError::BadRequest(
                "mongodb currently supports no_auth/password/token/tls_client_cert".to_string(),
            ));
        }
    }

    Ok(())
}

pub fn map_db_error(stage: &str, raw_message: String) -> AppError {
    let message = raw_message;
    let lower = message.to_lowercase();

    let code = if lower.contains("auth")
        || lower.contains("authentication failed")
        || lower.contains("sasl")
    {
        "CONN_AUTH_FAILED"
    } else if lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("server selection")
    {
        "CONN_TIMEOUT"
    } else if lower.contains("tls") || lower.contains("ssl") || lower.contains("certificate") {
        "CONN_TLS_HANDSHAKE_FAILED"
    } else {
        "CONN_NETWORK_UNREACHABLE"
    };

    AppError::BadRequest(format!("[{code}] [{stage}] {message}"))
}
