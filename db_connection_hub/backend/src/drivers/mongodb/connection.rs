// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
    drivers::connection_common::{
        require_network_host, require_network_port, validate_network_host_port,
        validate_password_auth, validate_supported_auth_mode, validate_tls_client_cert_auth,
        validate_token_auth,
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

    let host = require_network_host(datasource)?;
    let port = require_network_port(datasource)?;

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
    validate_network_host_port(datasource)?;
    validate_supported_auth_mode(
        datasource,
        &[
            AuthMode::NoAuth,
            AuthMode::Password,
            AuthMode::Token,
            AuthMode::TlsClientCert,
        ],
        "mongodb currently supports no_auth/password/token/tls_client_cert",
    )?;

    match datasource.auth.mode {
        AuthMode::NoAuth => {}
        AuthMode::Password => {
            validate_password_auth(datasource)?;
        }
        AuthMode::Token => {
            validate_token_auth(datasource)?;
        }
        AuthMode::TlsClientCert => {
            validate_tls_client_cert_auth(datasource)?;
        }
        _ => {}
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
