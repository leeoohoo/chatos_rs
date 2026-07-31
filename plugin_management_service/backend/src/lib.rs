// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod api;
pub mod auth;
mod cloud_secrets;
pub mod config;
pub mod models;
pub mod seed;
pub mod state;
pub mod store;
mod tool_catalog;

pub use api::{build_router, start_plugin_catalog_sync_loop};
pub use config::{load_plugin_management_dotenv, AppConfig};
pub use state::AppState;
