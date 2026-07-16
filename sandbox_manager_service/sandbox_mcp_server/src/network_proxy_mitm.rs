// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{BufReader, Cursor, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose, SanType, PKCS_ECDSA_P256_SHA256,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream as ClientTlsStream;
use tokio_rustls::server::TlsStream as ServerTlsStream;
use tokio_rustls::{TlsAcceptor, TlsConnector};
use tracing::warn;

pub(crate) const CUSTOM_CA_ENV_KEYS: [&str; 10] = [
    "CODEX_CA_CERTIFICATE",
    "SSL_CERT_FILE",
    "REQUESTS_CA_BUNDLE",
    "CURL_CA_BUNDLE",
    "NODE_EXTRA_CA_CERTS",
    "GIT_SSL_CAINFO",
    "PIP_CERT",
    "BUNDLE_SSL_CA_CERT",
    "npm_config_cafile",
    "NPM_CONFIG_CAFILE",
];

pub(crate) struct MitmAuthority {
    issuer: Issuer<'static, KeyPair>,
    upstream_config: Arc<ClientConfig>,
    leaf_configs: Mutex<BTreeMap<String, Arc<ServerConfig>>>,
    ca_bundle_path: PathBuf,
    owned_bundle_directory: Option<PathBuf>,
}

impl std::fmt::Debug for MitmAuthority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MitmAuthority")
            .field("ca_bundle_path", &self.ca_bundle_path)
            .finish_non_exhaustive()
    }
}

impl MitmAuthority {
    pub(crate) fn new(state_dir: &Path) -> Result<Self, String> {
        Self::new_with_additional_roots(state_dir, Vec::new())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn new_with_additional_roots(
        state_dir: &Path,
        additional_roots: Vec<CertificateDer<'static>>,
    ) -> Result<Self, String> {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let (ca_pem, issuer) = generate_ca()?;

        let mut root_certificates = load_platform_root_certificates();
        root_certificates.extend(load_startup_root_certificates()?);
        root_certificates.extend(additional_roots);
        deduplicate_certificates(&mut root_certificates);
        if root_certificates.is_empty() {
            return Err("sandbox HTTPS proxy found no upstream TLS root certificates".to_string());
        }

        let mut roots = RootCertStore::empty();
        let (added, ignored) = roots.add_parsable_certificates(root_certificates.iter().cloned());
        if ignored > 0 {
            warn!(ignored, "ignored invalid upstream TLS root certificates");
        }
        if added == 0 {
            return Err("sandbox HTTPS proxy could not parse any upstream TLS roots".to_string());
        }

        let (bundle_directory, owned_bundle_directory) = trust_bundle_directory(state_dir)?;
        let ca_bundle_path =
            match persist_trust_bundle(&bundle_directory, &root_certificates, ca_pem.as_str()) {
                Ok(path) => path,
                Err(err) => {
                    if owned_bundle_directory.is_some() {
                        let _ = fs::remove_dir(bundle_directory.as_path());
                    }
                    return Err(err);
                }
            };
        let mut upstream_config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        upstream_config.alpn_protocols = vec![b"http/1.1".to_vec()];

        Ok(Self {
            issuer,
            upstream_config: Arc::new(upstream_config),
            leaf_configs: Mutex::new(BTreeMap::new()),
            ca_bundle_path,
            owned_bundle_directory,
        })
    }

    pub(crate) fn ca_bundle_path(&self) -> &Path {
        self.ca_bundle_path.as_path()
    }

    pub(crate) async fn accept_client(
        &self,
        host: &str,
        stream: TcpStream,
    ) -> Result<ServerTlsStream<TcpStream>, String> {
        let config = self.server_config(host)?;
        TlsAcceptor::from(config)
            .accept(stream)
            .await
            .map_err(|err| format!("sandbox HTTPS client TLS handshake failed: {err}"))
    }

    pub(crate) async fn connect_upstream(
        &self,
        host: &str,
        stream: TcpStream,
    ) -> Result<ClientTlsStream<TcpStream>, String> {
        let server_name = tls_server_name(host)?;
        TlsConnector::from(self.upstream_config.clone())
            .connect(server_name, stream)
            .await
            .map_err(|err| format!("sandbox HTTPS upstream TLS handshake failed: {err}"))
    }

    fn server_config(&self, host: &str) -> Result<Arc<ServerConfig>, String> {
        let mut configs = self
            .leaf_configs
            .lock()
            .map_err(|_| "sandbox HTTPS certificate cache is poisoned".to_string())?;
        if let Some(config) = configs.get(host) {
            return Ok(config.clone());
        }

        let mut params = if let Ok(ip) = host.parse::<IpAddr>() {
            let mut params = CertificateParams::new(Vec::new())
                .map_err(|err| format!("create sandbox HTTPS IP certificate failed: {err}"))?;
            params.subject_alt_names.push(SanType::IpAddress(ip));
            params
        } else {
            CertificateParams::new(vec![host.to_string()])
                .map_err(|err| format!("create sandbox HTTPS host certificate failed: {err}"))?
        };
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];

        let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)
            .map_err(|err| format!("generate sandbox HTTPS leaf key failed: {err}"))?;
        let certificate = params
            .signed_by(&key, &self.issuer)
            .map_err(|err| format!("sign sandbox HTTPS leaf certificate failed: {err}"))?;
        let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der()));
        let mut config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![certificate.der().clone()], private_key)
            .map_err(|err| format!("build sandbox HTTPS server config failed: {err}"))?;
        // The native proxy currently parses HTTP/1.x itself. Advertising only HTTP/1.1 makes
        // HTTP/2-capable clients negotiate a protocol the broker can inspect safely.
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let config = Arc::new(config);
        configs.insert(host.to_string(), config.clone());
        Ok(config)
    }
}

impl Drop for MitmAuthority {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.ca_bundle_path.as_path());
        if let Some(directory) = self.owned_bundle_directory.as_ref() {
            let _ = fs::remove_dir(directory);
        }
    }
}

fn generate_ca() -> Result<(String, Issuer<'static, KeyPair>), String> {
    let mut params = CertificateParams::default();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(DnType::CommonName, "ChatOS sandbox network proxy MITM CA");
    params.distinguished_name = distinguished_name;

    let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)
        .map_err(|err| format!("generate sandbox MITM CA key failed: {err}"))?;
    let certificate = params
        .self_signed(&key)
        .map_err(|err| format!("generate sandbox MITM CA certificate failed: {err}"))?;
    let issuer = Issuer::new(params, key);
    Ok((certificate.pem(), issuer))
}

fn tls_server_name(host: &str) -> Result<ServerName<'static>, String> {
    let host = if let Some((address, _scope)) = host.rsplit_once('%') {
        if address.parse::<IpAddr>().is_ok() {
            address
        } else {
            host
        }
    } else {
        host
    };
    ServerName::try_from(host.to_string())
        .map_err(|_| format!("invalid TLS server name for sandbox HTTPS proxy: {host:?}"))
}

#[cfg(target_os = "linux")]
fn trust_bundle_directory(_state_dir: &Path) -> Result<(PathBuf, Option<PathBuf>), String> {
    use std::os::unix::fs::PermissionsExt;

    // Bubblewrap replaces /tmp with a private mount. Keep the CA in the same kind of short,
    // explicitly re-bound host directory used by the network bridge so the path remains visible
    // inside the network namespace even when state/workspace paths themselves live under /tmp.
    let directory = PathBuf::from("/tmp").join(format!(
        "chatos-proxy-ca-{}-{}",
        std::process::id(),
        &uuid::Uuid::new_v4().simple().to_string()[..12]
    ));
    fs::create_dir(&directory).map_err(|err| {
        format!(
            "create Linux sandbox HTTPS trust directory {} failed: {err}",
            directory.display()
        )
    })?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).map_err(|err| {
        let _ = fs::remove_dir(&directory);
        format!(
            "secure Linux sandbox HTTPS trust directory {} failed: {err}",
            directory.display()
        )
    })?;
    Ok((directory.clone(), Some(directory)))
}

#[cfg(not(target_os = "linux"))]
fn trust_bundle_directory(state_dir: &Path) -> Result<(PathBuf, Option<PathBuf>), String> {
    Ok((state_dir.to_path_buf(), None))
}

fn persist_trust_bundle(
    state_dir: &Path,
    roots: &[CertificateDer<'static>],
    ca_pem: &str,
) -> Result<PathBuf, String> {
    fs::create_dir_all(state_dir).map_err(|err| {
        format!(
            "create sandbox HTTPS trust bundle directory {} failed: {err}",
            state_dir.display()
        )
    })?;
    let path = state_dir.join(format!(
        ".chatos-network-proxy-ca-bundle-{}.pem",
        uuid::Uuid::new_v4()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o444);
    }
    let mut file = options.open(path.as_path()).map_err(|err| {
        format!(
            "create sandbox HTTPS trust bundle {} failed: {err}",
            path.display()
        )
    })?;
    for root in roots {
        append_certificate_pem(&mut file, root.as_ref())?;
    }
    file.write_all(ca_pem.as_bytes())
        .map_err(|err| format!("write sandbox MITM CA certificate failed: {err}"))?;
    file.sync_all()
        .map_err(|err| format!("sync sandbox HTTPS trust bundle failed: {err}"))?;
    Ok(path)
}

fn append_certificate_pem(writer: &mut impl Write, der: &[u8]) -> Result<(), String> {
    writer
        .write_all(b"-----BEGIN CERTIFICATE-----\n")
        .map_err(|err| err.to_string())?;
    let encoded = BASE64_STANDARD.encode(der);
    for line in encoded.as_bytes().chunks(64) {
        writer.write_all(line).map_err(|err| err.to_string())?;
        writer.write_all(b"\n").map_err(|err| err.to_string())?;
    }
    writer
        .write_all(b"-----END CERTIFICATE-----\n")
        .map_err(|err| err.to_string())
}

fn load_startup_root_certificates() -> Result<Vec<CertificateDer<'static>>, String> {
    let mut paths = BTreeSet::new();
    for key in CUSTOM_CA_ENV_KEYS {
        if let Some(value) = std::env::var_os(key).filter(|value| !value.is_empty()) {
            paths.insert(PathBuf::from(value));
        }
    }

    let mut certificates = Vec::new();
    for path in paths {
        certificates.extend(read_certificate_file(path.as_path()).map_err(|err| {
            format!(
                "read startup CA bundle {} for sandbox HTTPS proxy failed: {err}",
                path.display()
            )
        })?);
    }

    if let Some(value) = std::env::var_os("SSL_CERT_DIR").filter(|value| !value.is_empty()) {
        for directory in std::env::split_paths(&value) {
            let result = rustls_native_certs::load_certs_from_paths(None, Some(&directory));
            if !result.errors.is_empty() {
                warn!(
                    path = %directory.display(),
                    errors = result.errors.len(),
                    "some startup CA directory entries could not be loaded"
                );
            }
            certificates.extend(result.certs);
        }
    }
    Ok(certificates)
}

fn read_certificate_file(path: &Path) -> Result<Vec<CertificateDer<'static>>, String> {
    let bytes = fs::read(path).map_err(|err| err.to_string())?;
    let normalized = String::from_utf8_lossy(bytes.as_slice())
        .replace("BEGIN TRUSTED CERTIFICATE", "BEGIN CERTIFICATE")
        .replace("END TRUSTED CERTIFICATE", "END CERTIFICATE");
    let mut reader = BufReader::new(Cursor::new(normalized.into_bytes()));
    let certificates = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;
    if certificates.is_empty() {
        return Err("CA bundle contained no certificates".to_string());
    }
    Ok(certificates)
}

fn deduplicate_certificates(certificates: &mut Vec<CertificateDer<'static>>) {
    let mut seen = HashSet::new();
    certificates.retain(|certificate| seen.insert(certificate.as_ref().to_vec()));
}

#[cfg(all(unix, not(target_os = "macos")))]
fn load_platform_root_certificates() -> Vec<CertificateDer<'static>> {
    const FILES: &[&str] = &[
        "/etc/ssl/certs/ca-certificates.crt",
        "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem",
        "/etc/pki/tls/certs/ca-bundle.crt",
        "/etc/ssl/ca-bundle.pem",
        "/etc/pki/tls/cacert.pem",
        "/etc/ssl/cert.pem",
        "/opt/etc/ssl/certs/ca-certificates.crt",
        "/etc/ssl/certs/cacert.pem",
    ];
    const DIRECTORIES: &[&str] = &[
        "/etc/ssl/certs",
        "/etc/pki/tls/certs",
        "/etc/security/certificates",
    ];

    let mut certificates = Vec::new();
    if let Some(file) = FILES.iter().map(Path::new).find(|path| path.exists()) {
        let result = rustls_native_certs::load_certs_from_paths(Some(file), None);
        if !result.errors.is_empty() {
            warn!(errors = result.errors.len(), path = %file.display(), "some platform CA certificates could not be loaded");
        }
        certificates.extend(result.certs);
    }
    for directory in DIRECTORIES
        .iter()
        .map(Path::new)
        .filter(|path| path.exists())
    {
        let result = rustls_native_certs::load_certs_from_paths(None, Some(directory));
        if !result.errors.is_empty() {
            warn!(errors = result.errors.len(), path = %directory.display(), "some platform CA directory entries could not be loaded");
        }
        certificates.extend(result.certs);
    }
    certificates
}

#[cfg(target_os = "macos")]
fn load_platform_root_certificates() -> Vec<CertificateDer<'static>> {
    use security_framework::trust_settings::{Domain, TrustSettings, TrustSettingsForCertificate};

    let mut certificates = BTreeMap::new();
    for domain in [Domain::User, Domain::Admin, Domain::System] {
        let settings = TrustSettings::new(domain);
        let iterator = match settings.iter() {
            Ok(iterator) => iterator,
            Err(err) => {
                warn!(?domain, error = %err, "failed to load a macOS trust-settings domain");
                continue;
            }
        };
        for certificate in iterator {
            let trust = match settings.tls_trust_settings_for_certificate(&certificate) {
                Ok(value) => value.unwrap_or(TrustSettingsForCertificate::TrustRoot),
                Err(err) => {
                    warn!(error = %err, "failed to inspect a macOS root certificate");
                    continue;
                }
            };
            certificates.entry(certificate.to_der()).or_insert(trust);
        }
    }
    certificates
        .into_iter()
        .filter_map(|(der, trust)| {
            matches!(
                trust,
                TrustSettingsForCertificate::TrustRoot | TrustSettingsForCertificate::TrustAsRoot
            )
            .then(|| CertificateDer::from(der))
        })
        .collect()
}

#[cfg(not(any(unix, target_os = "macos")))]
fn load_platform_root_certificates() -> Vec<CertificateDer<'static>> {
    let result = rustls_native_certs::load_native_certs();
    if !result.errors.is_empty() {
        warn!(
            errors = result.errors.len(),
            "some platform CA certificates could not be loaded"
        );
    }
    result.certs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_server_name_accepts_dns_ip_and_scoped_ip() {
        assert!(tls_server_name("example.com").is_ok());
        assert!(tls_server_name("127.0.0.1").is_ok());
        assert!(tls_server_name("fe80::1%lo0").is_ok());
    }

    #[test]
    fn trust_bundle_contains_only_public_ca_material() {
        let directory = tempfile::tempdir().unwrap();
        let authority = MitmAuthority::new(directory.path()).unwrap();
        let bundle = fs::read_to_string(authority.ca_bundle_path()).unwrap();
        assert!(bundle.contains("-----BEGIN CERTIFICATE-----"));
        assert!(!bundle.contains("PRIVATE KEY"));
    }
}
