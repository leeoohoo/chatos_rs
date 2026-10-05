// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(test)]
use std::path::Path;

use axum_server::tls_rustls::RustlsConfig;
#[cfg(test)]
use chatos_internal_tls::load_mtls_server_config;
use chatos_internal_tls::{InternalMtlsConfig, InternalMtlsEnvironment};

pub type MemoryEngineInternalTlsConfig = InternalMtlsConfig;

pub const MEMORY_ENGINE_INTERNAL_MTLS_ENV: InternalMtlsEnvironment = InternalMtlsEnvironment {
    port: "MEMORY_ENGINE_INTERNAL_MTLS_PORT",
    public_port: "MEMORY_ENGINE_PORT",
    server_cert_path: "MEMORY_ENGINE_MTLS_SERVER_CERT_PATH",
    server_key_path: "MEMORY_ENGINE_MTLS_SERVER_KEY_PATH",
    client_ca_cert_path: "MEMORY_ENGINE_MTLS_CLIENT_CA_CERT_PATH",
};

pub fn load_internal_mtls_config(
    config: &MemoryEngineInternalTlsConfig,
) -> Result<RustlsConfig, String> {
    config.load_server_config("Memory Engine")
}

#[cfg(test)]
fn load_internal_mtls_config_from_paths(
    server_cert_path: &Path,
    server_key_path: &Path,
    client_ca_cert_path: &Path,
) -> Result<RustlsConfig, String> {
    load_mtls_server_config(
        server_cert_path,
        server_key_path,
        client_ca_cert_path,
        "Memory Engine",
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
            .join("../../scripts/generate-memory-engine-mtls.sh");
        assert_mtls_listener_contract(
            "memory-engine",
            &script,
            "user-service.identity.pem",
            load_internal_mtls_config_from_paths,
        )
        .await;
    }

    #[test]
    fn missing_mtls_material_is_rejected() {
        assert_missing_material_rejected(
            "memory-engine",
            load_internal_mtls_config_from_paths,
        );
    }
}
