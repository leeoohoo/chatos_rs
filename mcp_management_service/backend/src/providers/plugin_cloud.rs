// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use serde_json::Value;

use crate::runtime::ExternalHttpProviderBinding;

use super::external_http::ExternalHttpProvider;
use super::{ProviderCallError, ProviderCallOutcome, ProviderCancelOutcome};

const MAX_PLUGIN_TOOLS: usize = 200;
const MAX_PLUGIN_TOOL_SNAPSHOT_BYTES: usize = 512 * 1024;

#[path = "plugin_cloud/cloud_prepare.rs"]
mod cloud_prepare;
#[path = "plugin_cloud/cloud_runtime.rs"]
mod cloud_runtime;
mod init;
#[path = "plugin_cloud/prepare.rs"]
mod prepare;
#[path = "plugin_cloud/validation.rs"]
mod validation;

#[derive(Clone)]
pub(super) struct PluginCloudProvider {
    external_http: ExternalHttpProvider,
}

enum PreparedPluginCloudRoute {
    Http {
        binding: Box<ExternalHttpProviderBinding>,
        tools: Vec<Value>,
    },
}

#[cfg(test)]
mod tests;
