// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use axum_server::tls_rustls::RustlsConfig;
use chatos_internal_tls::load_mtls_server_config;

use crate::config::AppConfig;

pub fn load_internal_mtls_config(config: &AppConfig) -> Result<RustlsConfig, String> {
    load_internal_mtls_config_from_paths(
        config.mtls_server_cert_path.as_path(),
        config.mtls_server_key_path.as_path(),
        config.mtls_client_ca_cert_path.as_path(),
    )
}

fn load_internal_mtls_config_from_paths(
    server_cert_path: &Path,
    server_key_path: &Path,
    client_ca_cert_path: &Path,
) -> Result<RustlsConfig, String> {
    load_mtls_server_config(
        server_cert_path,
        server_key_path,
        client_ca_cert_path,
        "Configuration Center",
    )
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, TcpListener};
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use axum::routing::get;
    use axum::Router;

    use super::load_internal_mtls_config_from_paths;

    fn unique_test_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "chatos-config-center-mtls-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn generate_material(output_dir: &PathBuf) {
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/generate-config-center-mtls.sh");
        let status = Command::new(script)
            .arg(output_dir)
            .status()
            .expect("run mTLS generator");
        assert!(status.success(), "mTLS generator must succeed");
    }

    #[tokio::test]
    async fn internal_listener_requires_a_trusted_client_certificate_and_tls() {
        let material_dir = unique_test_dir("handshake");
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
        let identity_pem = std::fs::read(material_dir.join("memory-engine.identity.pem"))
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

        let plaintext_url = format!("http://127.0.0.1:{}/probe", address.port());
        assert!(reqwest::get(plaintext_url).await.is_err());

        handle.shutdown();
        server
            .await
            .expect("join mTLS server")
            .expect("mTLS server");
        let _ = std::fs::remove_dir_all(material_dir);
    }

    #[test]
    fn empty_or_missing_mtls_material_is_rejected() {
        let missing = unique_test_dir("missing");
        assert!(load_internal_mtls_config_from_paths(
            missing.join("server.crt").as_path(),
            missing.join("server.key").as_path(),
            missing.join("ca.crt").as_path(),
        )
        .is_err());
    }
}
