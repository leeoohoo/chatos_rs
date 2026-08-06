// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod api;
pub mod async_dispatch;
pub mod auth;
pub mod capabilities;
pub mod config;
pub mod error;
mod internal_tls;
pub mod pressure;
pub mod project_context;
pub mod providers;
pub mod result_events;
pub mod routing;
pub mod runtime;
pub mod state;

pub use api::{build_internal_router, build_public_router};
pub use config::{load_mcp_management_dotenv, AppConfig};
pub use internal_tls::load_internal_mtls_config;
pub use state::AppState;
