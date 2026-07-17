// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod api;
pub mod auth;
pub mod catalog;
pub mod config;
pub mod models;
pub mod state;
pub mod store;

pub use api::build_router;
pub use config::{load_config_center_dotenv, AppConfig};
pub use state::AppState;
