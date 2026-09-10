// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub mod client;
pub mod config;
pub mod dto;
pub mod error;
pub mod project_context;

pub use client::{McpManagementClient, McpManagementRuntimeSessionHandle};
pub use config::McpManagementClientConfig;
pub use dto::*;
pub use error::McpManagementClientError;
pub use project_context::{
    AuthorizeProjectContextRequest, ClientProjectContextSnapshot, ClientProjectExecutionTarget,
    ProjectContextAuthorization,
};
