use std::sync::Arc;

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::request_support::{format_error_response, truncate_log};
use super::*;
use crate::core::mcp_tools::ToolResult;
use crate::services::ai_client_common::AiClientCallbacks;
use crate::utils::abort_registry;

mod metadata;
mod stream;
mod tools;
