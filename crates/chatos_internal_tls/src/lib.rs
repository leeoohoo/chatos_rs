// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Shared construction of mutually authenticated TLS server configuration.

use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum_server::tls_rustls::RustlsConfig;
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};

#[cfg(feature = "test-support")]
pub mod test_support;

#[derive(Debug, Clone, Copy)]
pub struct InternalMtlsEnvironment {
    pub port: &'static str,
    pub public_port: &'static str,
    pub server_cert_path: &'static str,
    pub server_key_path: &'static str,
    pub client_ca_cert_path: &'static str,
}

#[derive(Debug, Clone)]
pub struct InternalMtlsConfig {
    pub bind_addr: SocketAddr,
    pub server_cert_path: PathBuf,
    pub server_key_path: PathBuf,
    pub client_ca_cert_path: PathBuf,
}

impl InternalMtlsConfig {
    pub fn from_env(
        host: IpAddr,
        public_port: u16,
        environment: &InternalMtlsEnvironment,
    ) -> Result<Self, String> {
        let port = required_env(environment.port)?
            .parse::<u16>()
            .map_err(|err| format!("{} must be a valid port: {err}", environment.port))?;
        if port == public_port {
            return Err(format!(
                "{} must differ from {}",
                environment.port, environment.public_port
            ));
        }
        Ok(Self {
            bind_addr: SocketAddr::new(host, port),
            server_cert_path: PathBuf::from(required_env(environment.server_cert_path)?),
            server_key_path: PathBuf::from(required_env(environment.server_key_path)?),
            client_ca_cert_path: PathBuf::from(required_env(environment.client_ca_cert_path)?),
        })
    }

    pub fn load_server_config(&self, service_name: &str) -> Result<RustlsConfig, String> {
        load_mtls_server_config(
            self.server_cert_path.as_path(),
            self.server_key_path.as_path(),
            self.client_ca_cert_path.as_path(),
            service_name,
        )
    }
}

/// Load an mTLS server configuration from PEM files.
///
/// `service_name` is included in construction errors so deployments can identify
/// which listener failed without exposing certificate or private-key contents.
pub fn load_mtls_server_config(
    server_cert_path: &Path,
    server_key_path: &Path,
    client_ca_cert_path: &Path,
    service_name: &str,
) -> Result<RustlsConfig, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let server_certificates = read_certificates(server_cert_path)?;
    let server_key = read_private_key(server_key_path)?;
    let client_ca_certificates = read_certificates(client_ca_cert_path)?;

    let mut client_roots = RootCertStore::empty();
    for certificate in client_ca_certificates {
        client_roots
            .add(certificate)
            .map_err(|err| format!("invalid {service_name} mTLS client CA certificate: {err}"))?;
    }

    let client_verifier = WebPkiClientVerifier::builder(Arc::new(client_roots))
        .build()
        .map_err(|err| format!("build {service_name} mTLS client verifier failed: {err}"))?;
    let server_config = ServerConfig::builder()
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(server_certificates, server_key)
        .map_err(|err| format!("build {service_name} mTLS server config failed: {err}"))?;

    Ok(RustlsConfig::from_config(Arc::new(server_config)))
}

fn required_env(key: &str) -> Result<String, String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{key} is required as deployment Secret material"))
}

fn read_certificates(path: &Path) -> Result<Vec<CertificateDer<'static>>, String> {
    let certificates = CertificateDer::pem_file_iter(path)
        .map_err(|err| format!("open certificate file {} failed: {err}", path.display()))?
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

fn read_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, String> {
    PrivateKeyDer::from_pem_file(path)
        .map_err(|err| format!("read private key file {} failed: {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::load_mtls_server_config;

    fn unique_test_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "chatos-internal-tls-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn missing_certificate_is_rejected_with_its_path() {
        let missing = unique_test_dir("missing");
        let error = load_mtls_server_config(&missing, &missing, &missing, "test service")
            .expect_err("missing certificate must fail");
        assert!(error.contains(&missing.display().to_string()));
    }

    #[test]
    fn empty_certificate_is_rejected() {
        let directory = unique_test_dir("empty-certificate");
        std::fs::create_dir_all(&directory).expect("create test directory");
        let empty = directory.join("empty.pem");
        std::fs::write(&empty, []).expect("write empty PEM");

        let error = load_mtls_server_config(&empty, &empty, &empty, "test service")
            .expect_err("empty certificate must fail");
        assert!(error.contains("contains no certificates"));

        std::fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn malformed_certificate_is_rejected() {
        let directory = unique_test_dir("malformed-certificate");
        std::fs::create_dir_all(&directory).expect("create test directory");
        let malformed = directory.join("malformed.pem");
        std::fs::write(&malformed, b"-----BEGIN CERTIFICATE-----\nnot-base64\n")
            .expect("write malformed PEM");

        let error = load_mtls_server_config(&malformed, &malformed, &malformed, "test service")
            .expect_err("malformed certificate must fail");
        assert!(error.contains("parse certificate file"));

        std::fs::remove_dir_all(directory).expect("remove test directory");
    }
}
