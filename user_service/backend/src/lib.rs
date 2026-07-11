// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod api;
mod auth;
mod config;
mod db;
mod email;
mod integrations;
mod models;
mod secrets;
mod state;
mod store;

pub use api::build_router;
pub use config::{load_user_service_dotenv, AppConfig};
pub use state::AppState;
