// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::WebToolsService;
use serde_json::Value;

use crate::runtime::RuntimeSessionSnapshot;

use super::{ProviderCallError, ProviderCallOutcome};

mod init;

#[derive(Clone)]
pub(super) struct EmbeddedProvider {
    web_tools: WebToolsService,
    response_limit_bytes: usize,
}

impl EmbeddedProvider {
    pub(super) async fn call_tool(
        &self,
        _snapshot: &RuntimeSessionSnapshot,
        route: &chatos_mcp_management_sdk::ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        _invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        if !self.supports(route) {
            return Err(ProviderCallError::provider_unavailable(
                "embedded Provider does not support this route",
            ));
        }
        let result = self
            .web_tools
            .call_tool(original_tool_name, arguments)
            .map_err(ProviderCallError::provider_unavailable)?;
        let response_bytes = serde_json::to_vec(&result)
            .map_err(|err| {
                ProviderCallError::invalid_response(format!(
                    "serialize embedded Provider result failed: {err}"
                ))
            })?
            .len();
        if response_bytes > self.response_limit_bytes {
            return Err(ProviderCallError::invalid_response(format!(
                "embedded Provider result exceeds {} bytes",
                self.response_limit_bytes
            )));
        }
        Ok(ProviderCallOutcome {
            result,
            response_bytes,
        })
    }
}

#[cfg(test)]
mod tests;
