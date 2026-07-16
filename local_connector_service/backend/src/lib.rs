// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod api;
pub mod auth;
pub mod config;
mod managed_requirements;
pub mod models;
pub mod relay;
pub mod state;
pub mod store;

pub use api::build_router;
pub use config::{load_local_connector_dotenv, AppConfig};
pub use state::AppState;
