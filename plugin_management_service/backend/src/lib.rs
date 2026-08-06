// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod api;
pub mod auth;
pub(crate) mod catalog_sync_queue;
mod cloud_secrets;
pub mod config;
pub mod internal_tls;
pub mod models;
pub mod pressure;
pub mod seed;
pub mod state;
pub mod store;
mod tool_catalog;

pub use api::{build_internal_router, build_public_router};
pub use catalog_sync_queue::start as start_plugin_catalog_sync_queue;
pub use config::{load_plugin_management_dotenv, AppConfig};
pub use state::AppState;
