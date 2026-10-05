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
    use std::path::PathBuf;

    use chatos_internal_tls::test_support::{
        assert_missing_material_rejected, assert_mtls_listener_contract,
    };

    use super::load_internal_mtls_config_from_paths;

    #[tokio::test]
    async fn internal_listener_requires_a_trusted_client_certificate_and_tls() {
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/generate-config-center-mtls.sh");
        assert_mtls_listener_contract(
            "config-center",
            &script,
            "memory-engine.identity.pem",
            load_internal_mtls_config_from_paths,
        )
        .await;
    }

    #[test]
    fn empty_or_missing_mtls_material_is_rejected() {
        assert_missing_material_rejected("config-center", load_internal_mtls_config_from_paths);
    }
}
