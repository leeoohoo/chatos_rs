// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::File;
use std::io::BufReader;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum_server::tls_rustls::RustlsConfig;
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};

#[derive(Debug, Clone)]
pub struct ProjectServiceInternalTlsConfig {
    pub bind_addr: SocketAddr,
    pub server_cert_path: PathBuf,
    pub server_key_path: PathBuf,
    pub client_ca_cert_path: PathBuf,
}

impl ProjectServiceInternalTlsConfig {
    pub fn from_env(host: IpAddr, public_port: u16) -> Result<Self, String> {
        let port = required_env("PROJECT_SERVICE_INTERNAL_MTLS_PORT")?
            .parse::<u16>()
            .map_err(|err| {
                format!("PROJECT_SERVICE_INTERNAL_MTLS_PORT must be a valid port: {err}")
            })?;
        if port == public_port {
            return Err(
                "PROJECT_SERVICE_INTERNAL_MTLS_PORT must differ from PROJECT_SERVICE_PORT"
                    .to_string(),
            );
        }
        Ok(Self {
            bind_addr: SocketAddr::new(host, port),
            server_cert_path: PathBuf::from(required_env("PROJECT_SERVICE_MTLS_SERVER_CERT_PATH")?),
            server_key_path: PathBuf::from(required_env("PROJECT_SERVICE_MTLS_SERVER_KEY_PATH")?),
            client_ca_cert_path: PathBuf::from(required_env(
                "PROJECT_SERVICE_MTLS_CLIENT_CA_CERT_PATH",
            )?),
        })
    }
}

pub fn load_internal_mtls_config(
    config: &ProjectServiceInternalTlsConfig,
) -> Result<RustlsConfig, String> {
    load_internal_mtls_config_from_paths(
        config.server_cert_path.as_path(),
        config.server_key_path.as_path(),
        config.client_ca_cert_path.as_path(),
    )
}

fn load_internal_mtls_config_from_paths(
    server_cert_path: &Path,
    server_key_path: &Path,
    client_ca_cert_path: &Path,
) -> Result<RustlsConfig, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let server_certificates = read_certificates(server_cert_path)?;
    let server_key = read_private_key(server_key_path)?;
    let mut client_roots = RootCertStore::empty();
    for certificate in read_certificates(client_ca_cert_path)? {
        client_roots
            .add(certificate)
            .map_err(|err| format!("invalid Project Service mTLS client CA: {err}"))?;
    }
    if client_roots.is_empty() {
        return Err("Project Service mTLS client CA contains no certificates".to_string());
    }
    let verifier = WebPkiClientVerifier::builder(Arc::new(client_roots))
        .build()
        .map_err(|err| format!("build Project Service mTLS client verifier failed: {err}"))?;
    let server_config = ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(server_certificates, server_key)
        .map_err(|err| format!("build Project Service mTLS server config failed: {err}"))?;
    Ok(RustlsConfig::from_config(Arc::new(server_config)))
}

fn read_certificates(
    path: &Path,
) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>, String> {
    let file = File::open(path)
        .map_err(|err| format!("open certificate file {} failed: {err}", path.display()))?;
    let certificates = rustls_pemfile::certs(&mut BufReader::new(file))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("parse certificate file {} failed: {err}", path.display()))?;
    if certificates.is_empty() {
        return Err(format!(
            "certificate file {} contains no certificates",
            path.display()
        ));
    }
    Ok(certificates)
}

fn read_private_key(path: &Path) -> Result<rustls::pki_types::PrivateKeyDer<'static>, String> {
    let file = File::open(path)
        .map_err(|err| format!("open private key file {} failed: {err}", path.display()))?;
    rustls_pemfile::private_key(&mut BufReader::new(file))
        .map_err(|err| format!("parse private key file {} failed: {err}", path.display()))?
        .ok_or_else(|| {
            format!(
                "private key file {} contains no private key",
                path.display()
            )
        })
}

fn required_env(key: &str) -> Result<String, String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{key} is required as deployment Secret material"))
}

#[cfg(test)]
mod tests {
    use super::load_internal_mtls_config_from_paths;
    use std::net::{Ipv4Addr, TcpListener};
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use axum::routing::get;
    use axum::Router;

    fn unique_test_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "chatos-project-service-mtls-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn generate_material(output_dir: &PathBuf) {
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/generate-project-service-mtls.sh");
        let status = Command::new(script)
            .arg(output_dir)
            .status()
            .expect("run Project Service mTLS generator");
        assert!(status.success(), "mTLS generator must succeed");
    }

    #[tokio::test]
    async fn internal_listener_requires_a_trusted_client_certificate_and_tls() {
        let material_dir = unique_test_dir("trusted");
        generate_material(&material_dir);
        let tls = load_internal_mtls_config_from_paths(
            material_dir.join("server.crt").as_path(),
            material_dir.join("server.key").as_path(),
            material_dir.join("ca.crt").as_path(),
        )
        .expect("load server mTLS config");
        let probe = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("reserve test port");
        let address = probe.local_addr().expect("test address");
        drop(probe);
        let handle = axum_server::Handle::new();
        let server_handle = handle.clone();
        let server = tokio::spawn(async move {
            axum_server::bind_rustls(address, tls)
                .handle(server_handle)
                .serve(
                    Router::new()
                        .route("/probe", get(|| async { "ok" }))
                        .into_make_service(),
                )
                .await
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let ca_pem = std::fs::read(material_dir.join("ca.crt")).expect("read CA");
        let identity_pem = std::fs::read(material_dir.join("chatos-backend.identity.pem"))
            .expect("read client identity");
        let trusted_client = reqwest::Client::builder()
            .use_rustls_tls()
            .add_root_certificate(reqwest::Certificate::from_pem(&ca_pem).expect("parse CA"))
            .identity(reqwest::Identity::from_pem(&identity_pem).expect("parse identity"))
            .build()
            .expect("build trusted client");
        let url = format!("https://127.0.0.1:{}/probe", address.port());
        let response = trusted_client
            .get(url.as_str())
            .send()
            .await
            .expect("mTLS request");
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let no_identity_client = reqwest::Client::builder()
            .use_rustls_tls()
            .add_root_certificate(reqwest::Certificate::from_pem(&ca_pem).expect("parse CA"))
            .build()
            .expect("build client without identity");
        assert!(no_identity_client.get(url.as_str()).send().await.is_err());
        assert!(
            reqwest::get(format!("http://127.0.0.1:{}/probe", address.port()))
                .await
                .is_err()
        );

        handle.shutdown();
        server
            .await
            .expect("join mTLS server")
            .expect("mTLS server");
        let _ = std::fs::remove_dir_all(material_dir);
    }

    #[test]
    fn missing_mtls_material_is_rejected() {
        let missing = unique_test_dir("missing");
        assert!(load_internal_mtls_config_from_paths(
            missing.join("server.crt").as_path(),
            missing.join("server.key").as_path(),
            missing.join("ca.crt").as_path(),
        )
        .is_err());
    }
}
