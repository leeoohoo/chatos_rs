// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use super::ProviderCallError;

mod init;
mod prepare;
mod runtime_calls;
mod validation;
use validation::*;
pub(crate) use validation::{build_pinned_external_http_client, header_is_managed_or_unsafe};

const JSON_CONTENT_TYPE: &str = "application/json";
const MAX_CONFIGURED_HEADERS: usize = 64;
const MAX_CONFIGURED_HEADER_BYTES: usize = 32 * 1024;
const MAX_TOOL_POLICY_ITEMS: usize = 512;
const MAX_TOOL_NAME_BYTES: usize = 256;

#[derive(Clone)]
pub(super) struct ExternalHttpProvider {
    request_timeout: Duration,
    response_limit_bytes: usize,
}

#[cfg(test)]
mod tests;
