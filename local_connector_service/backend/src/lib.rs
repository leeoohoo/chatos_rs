// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod api;
pub mod auth;
pub mod config;
pub mod internal_tls;
mod managed_config;
mod managed_requirements;
pub mod models;
pub mod pressure;
pub mod relay;
mod relay_signature;
pub mod state;
pub mod store;
mod valkey_coordination;

pub use api::{build_internal_router, build_public_router};
#[cfg(feature = "test-support")]
pub use api::{
    build_plugin_artifact_relay_store_test_router, build_plugin_artifact_relay_test_router,
    PluginArtifactRelayTestScope,
};
pub use config::{load_local_connector_dotenv, AppConfig};
pub use state::AppState;
