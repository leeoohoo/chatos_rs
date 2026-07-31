// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod api;
pub mod auth;
pub mod capabilities;
pub mod config;
pub mod error;
pub mod project_context;
pub mod routing;
pub mod runtime;
pub mod state;

pub use api::build_router;
pub use config::{load_mcp_management_dotenv, AppConfig};
pub use state::AppState;
