// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod cancel_response;
mod canonical_json;
mod chatos;
mod dispatcher_init;
mod dispatcher_prepare;
#[path = "dispatcher_prepare/plugins.rs"]
mod dispatcher_prepare_plugins;
#[path = "dispatcher_prepare/system.rs"]
mod dispatcher_prepare_system;
mod dispatcher_runtime;
mod dispatcher_runtime_lifecycle;
mod dispatcher_support;
mod local_connector;
pub(crate) mod plugin_components;
mod plugin_local;
mod plugin_routes;
#[path = "plugin_routes/prepare.rs"]
mod plugin_routes_prepare;
#[path = "plugin_routes/runtime.rs"]
mod plugin_routes_runtime;
mod project_service;
mod task_runner;

use std::time::Duration;

use serde_json::{json, Value};

pub(super) use cancel_response::decode_cancel_notification_response;
pub(crate) use chatos::memory_provider_ref as chatos_memory_provider_ref;
use chatos::ChatosProvider;
use local_connector::LocalConnectorProvider;
use plugin_components::PluginComponentProvider;
use plugin_local::PluginLocalProvider;
use plugin_routes::PluginRouteDispatcher;
use project_service::ProjectServiceProvider;
pub use project_service::{ProviderCallError, ProviderCallOutcome, ProviderWaitingForUser};
use task_runner::TaskRunnerProvider;

pub struct TaskRunnerProviderConfig {
    pub http: reqwest::Client,
    pub base_url: String,
    pub internal_secret: Option<String>,
    pub request_timeout: Duration,
    pub ask_user_request_timeout: Duration,
}

pub struct ChatosProviderConfig {
    pub http: reqwest::Client,
    pub base_url: String,
    pub internal_secret: Option<String>,
    pub request_timeout: Duration,
    pub ask_user_request_timeout: Duration,
    pub browser_request_timeout: Duration,
}

pub struct ProviderRuntimeConfig {
    pub downstream_request_timeout: Duration,
    pub local_connector_request_timeout: Duration,
    pub response_limit_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCancelOutcome {
    Cancelled,
    CancelRequested,
    NotSupported,
}

#[derive(Clone)]
pub struct ProviderDispatcher {
    local_connector: LocalConnectorProvider,
    plugins: PluginRouteDispatcher,
    project_service: ProjectServiceProvider,
    task_runner: TaskRunnerProvider,
    chatos: ChatosProvider,
}

const TOOL_RESULT_MAX_CHARS_META_KEY: &str = "chatos/toolResultMaxChars";

fn managed_tool_call_params(
    original_tool_name: &str,
    arguments: Value,
    tool_result_max_chars: Option<usize>,
) -> Value {
    let mut params = json!({
        "name": original_tool_name,
        "arguments": arguments,
    });
    if let Some(max_chars) = tool_result_max_chars {
        params["_meta"] = json!({
            TOOL_RESULT_MAX_CHARS_META_KEY: max_chars.max(1),
        });
    }
    params
}
