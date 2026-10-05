// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Shared integration-test support for internal mTLS listeners.

use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::routing::get;
use axum::Router;
use axum_server::tls_rustls::RustlsConfig;

pub async fn assert_mtls_listener_contract<F>(
    namespace: &str,
    generator_script: &Path,
    trusted_identity_file: &str,
    load_config: F,
) where
    F: Fn(&Path, &Path, &Path) -> Result<RustlsConfig, String>,
{
    let material_dir = unique_test_dir(namespace, "trusted");
    let status = Command::new(generator_script)
        .arg(&material_dir)
        .status()
        .expect("run mTLS material generator");
    assert!(status.success(), "mTLS material generator must succeed");

    let tls = load_config(
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
    let identity_pem = std::fs::read(material_dir.join(trusted_identity_file))
        .expect("read trusted client identity");
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

pub fn assert_missing_material_rejected<F>(namespace: &str, load_config: F)
where
    F: Fn(&Path, &Path, &Path) -> Result<RustlsConfig, String>,
{
    let missing = unique_test_dir(namespace, "missing");
    assert!(load_config(
        missing.join("server.crt").as_path(),
        missing.join("server.key").as_path(),
        missing.join("ca.crt").as_path(),
    )
    .is_err());
}

fn unique_test_dir(namespace: &str, label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "chatos-{namespace}-mtls-{label}-{}-{nonce}",
        std::process::id()
    ))
}
