// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;

use thiserror::Error;

mod components;
mod hook_set;
mod mcp_config;
mod normalized;
mod parser;
mod paths;
mod validation_support;
mod validator;

pub(crate) use components::component_key_from_path;
pub use components::*;
pub use hook_set::*;
pub use mcp_config::*;
pub use normalized::*;
pub use parser::{parse_plugin_manifest, PluginManifestSource};
pub use paths::{normalize_plugin_relative_path, plugin_manifest_source_from_path};
pub use validator::{validate_plugin_manifest, PluginManifestValidationError};

#[derive(Debug, Error)]
pub enum PluginManifestError {
    #[error("invalid plugin manifest JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid plugin manifest field {field}: {message}")]
    InvalidField { field: String, message: String },
    #[error(transparent)]
    Validation(#[from] PluginManifestValidationError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifestValidationIssue {
    pub field: String,
    pub message: String,
}

impl fmt::Display for PluginManifestValidationIssue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.field, self.message)
    }
}

#[cfg(test)]
mod tests;
