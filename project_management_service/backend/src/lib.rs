// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod api;
pub mod auth;
pub mod config;
pub mod domain;
mod http_body;
pub mod mcp_server;
mod mcp_tools;
pub mod models;
pub mod services;
pub mod state;
pub mod store;
pub mod task_runner_api_client;

pub use api::build_router;
pub use config::{load_project_service_dotenv, AppConfig};
pub use state::AppState;
