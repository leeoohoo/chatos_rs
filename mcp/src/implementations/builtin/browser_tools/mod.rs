// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod actions;
mod context;
mod managed_preview;
mod managed_screencast;
mod registration_basic;
mod registration_observe;
mod registration_privileged;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chatos_mcp_runtime::{ToolCallContext, ToolCallerModelRuntime};
use parking_lot::Mutex;
use serde_json::Value;

use crate::browser_command_support::{browser_command_error_text, browser_command_succeeded};
use crate::browser_runtime::{
    browser_backend_available, run_browser_command as runtime_run_browser_command,
    BrowserRuntimeSession,
};
use crate::tool_registry::ToolRegistry;
use crate::tool_registry::{block_on_result, text_result};

const DEFAULT_COMMAND_TIMEOUT_SECONDS: u64 = 30;
const DEFAULT_MAX_SNAPSHOT_CHARS: usize = 8_000;
const BROWSER_SESSION_SCREENSHOT_MAX_BYTES: usize = 5 * 1024 * 1024;
pub(super) const DEFAULT_BROWSER_RESEARCH_LIMIT: usize = 5;
pub(super) const MAX_BROWSER_RESEARCH_LIMIT: usize = 20;
pub(super) const MAX_BROWSER_RESEARCH_EXTRACT_URLS: usize = 5;
const BROWSER_TOOL_NAMES: [&str; 30] = [
    "browser_tabs",
    "browser_tab_new",
    "browser_tab_switch",
    "browser_tab_close",
    "browser_navigate",
    "browser_snapshot",
    "browser_click",
    "browser_type",
    "browser_scroll",
    "browser_back",
    "browser_press",
    "browser_upload",
    "browser_download",
    "browser_console",
    "browser_network",
    "browser_network_request",
    "browser_har_start",
    "browser_har_stop",
    "browser_websocket_start",
    "browser_websocket_frames",
    "browser_websocket_stop",
    "browser_route_add",
    "browser_route_list",
    "browser_route_remove",
    "browser_route_clear",
    "browser_cdp_command",
    "browser_get_images",
    "browser_inspect",
    "browser_research",
    "browser_vision",
];

#[derive(Debug, Clone)]
pub struct BrowserToolsOptions {
    pub server_name: String,
    pub workspace_dir: PathBuf,
    pub command_timeout_seconds: u64,
    pub max_snapshot_chars: usize,
    pub vision_adapter: Option<BrowserVisionAdapterRef>,
    /// Enable the bounded, session-scoped route interception tools. Hosts must
    /// enforce explicit local approval before calling browser_route_add.
    pub route_interception_enabled: bool,
    /// Enable the high-risk arbitrary CDP command tool. Hosts must expose this
    /// only after an explicit device-local risk acknowledgement and must obtain
    /// explicit local approval for every call.
    pub full_cdp_access_enabled: bool,
    /// Register the authoritative schemas even when the browser executable is
    /// unavailable. This is intended for descriptor/catalog generation only.
    pub schema_catalog_only: bool,
}

#[derive(Clone)]
pub struct BrowserToolsService {
    registry: ToolRegistry<ToolHandler>,
    bound: BoundContext,
}

#[derive(Debug, Clone)]
pub struct BrowserSessionPreviewFrame {
    pub bytes: Vec<u8>,
    pub media_type: &'static str,
    pub sequence: u64,
    pub width: u32,
    pub height: u32,
    pub page_scale_factor: f64,
    pub offset_top: f64,
    pub scroll_offset_x: f64,
    pub scroll_offset_y: f64,
    pub crop_offset_y: f64,
    pub timestamp: u64,
    pub source: &'static str,
    pub warning: Option<String>,
}

type ToolHandler =
    Arc<dyn Fn(Value, BrowserToolCallContext) -> Result<Value, String> + Send + Sync>;

#[derive(Debug, Clone, Default)]
pub struct BrowserToolCallContext {
    pub conversation_id: Option<String>,
    pub caller_model_runtime: Option<ToolCallerModelRuntime>,
}

impl BrowserToolCallContext {
    pub fn from_conversation_id(conversation_id: Option<&str>) -> Self {
        Self {
            conversation_id: conversation_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            caller_model_runtime: None,
        }
    }

    pub fn from_tool_call_context(context: &ToolCallContext) -> Self {
        Self {
            conversation_id: context.conversation_id.clone(),
            caller_model_runtime: context.caller_model_runtime.clone(),
        }
    }
}

#[derive(Clone)]
pub(super) struct BoundContext {
    pub(super) _server_name: String,
    pub(super) workspace_dir: PathBuf,
    pub(super) command_timeout_seconds: u64,
    pub(super) max_snapshot_chars: usize,
    pub(super) sessions: Arc<Mutex<HashMap<String, BrowserRuntimeSession>>>,
    pub(super) routes: Arc<Mutex<HashMap<String, Vec<actions::BrowserRouteRecord>>>>,
    pub(super) route_mutation_lock: Arc<tokio::sync::Mutex<()>>,
    pub(super) vision_adapter: Option<BrowserVisionAdapterRef>,
}

#[derive(Debug, Clone)]
pub struct BrowserVisionRequest {
    pub question: String,
    pub screenshot_path: String,
    pub conversation_id: Option<String>,
    pub caller_model_runtime: Option<ToolCallerModelRuntime>,
    pub annotate: bool,
}

#[derive(Debug, Clone)]
pub struct BrowserVisionResponse {
    pub analysis: String,
    pub vision: Value,
}

#[derive(Debug, Clone)]
pub struct BrowserVisionFailure {
    pub error: String,
    pub attempts: Vec<Value>,
    pub warnings: Vec<String>,
}

#[async_trait]
pub trait BrowserVisionAdapter: Send + Sync {
    async fn analyze_screenshot(
        &self,
        request: BrowserVisionRequest,
    ) -> Result<BrowserVisionResponse, BrowserVisionFailure>;
}

#[derive(Clone)]
pub struct BrowserVisionAdapterRef {
    inner: Arc<dyn BrowserVisionAdapter>,
}

impl std::fmt::Debug for BrowserVisionAdapterRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BrowserVisionAdapterRef")
    }
}

impl BrowserVisionAdapterRef {
    pub fn new(inner: Arc<dyn BrowserVisionAdapter>) -> Self {
        Self { inner }
    }

    pub(crate) async fn analyze_screenshot(
        &self,
        request: BrowserVisionRequest,
    ) -> Result<BrowserVisionResponse, BrowserVisionFailure> {
        self.inner.analyze_screenshot(request).await
    }
}

impl BrowserToolsService {
    pub fn new(opts: BrowserToolsOptions) -> Result<Self, String> {
        std::fs::create_dir_all(&opts.workspace_dir)
            .map_err(|err| format!("create browser workspace dir failed: {}", err))?;
        let workspace_dir = opts
            .workspace_dir
            .canonicalize()
            .unwrap_or_else(|_| opts.workspace_dir.clone());
        let bound = BoundContext {
            _server_name: opts.server_name,
            workspace_dir,
            command_timeout_seconds: opts
                .command_timeout_seconds
                .max(DEFAULT_COMMAND_TIMEOUT_SECONDS),
            max_snapshot_chars: opts.max_snapshot_chars.clamp(1, DEFAULT_MAX_SNAPSHOT_CHARS),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            routes: Arc::new(Mutex::new(HashMap::new())),
            route_mutation_lock: Arc::new(tokio::sync::Mutex::new(())),
            vision_adapter: opts.vision_adapter,
        };
        let mut service = Self {
            registry: ToolRegistry::new(),
            bound: bound.clone(),
        };

        if opts.schema_catalog_only {
            service.register_basic_tools(bound.clone());
            service.register_observe_tools(bound.clone());
            service.register_route_tools(bound.clone());
            service.register_full_cdp_tool(bound);
        } else if let Err(reason) = browser_backend_available() {
            service
                .registry
                .register_unavailable_tools(BROWSER_TOOL_NAMES, reason.clone());
        } else {
            service.register_basic_tools(bound.clone());
            service.register_observe_tools(bound.clone());
            if opts.route_interception_enabled {
                service.register_route_tools(bound.clone());
            } else {
                service.registry.register_unavailable_tools(
                    [
                        "browser_route_add",
                        "browser_route_list",
                        "browser_route_remove",
                        "browser_route_clear",
                    ],
                    "browser route interception requires the Local Connector approval host"
                        .to_string(),
                );
            }
            if opts.full_cdp_access_enabled {
                service.register_full_cdp_tool(bound);
            } else {
                service.registry.register_unavailable_tool(
                    "browser_cdp_command",
                    "full CDP access is disabled in Local Connector settings".to_string(),
                );
            }
        }

        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.registry.list_tools()
    }

    pub fn call_tool(
        &self,
        name: &str,
        args: Value,
        conversation_id: Option<&str>,
    ) -> Result<Value, String> {
        self.call_tool_with_context(
            name,
            args,
            BrowserToolCallContext::from_conversation_id(conversation_id),
        )
    }

    pub fn call_tool_with_context(
        &self,
        name: &str,
        args: Value,
        context: BrowserToolCallContext,
    ) -> Result<Value, String> {
        let tool = self
            .registry
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;
        (tool.handler)(args, context)
    }

    pub fn unavailable_tools(&self) -> Vec<(String, String)> {
        self.registry.unavailable_tools()
    }

    pub fn attach_managed_session(
        &self,
        conversation_id: &str,
        session_id: &str,
    ) -> Result<(), String> {
        let conversation_key = normalize_browser_session_conversation_id(conversation_id)?;
        let session_id = normalize_managed_browser_session_id(session_id)?;
        self.bound.sessions.lock().insert(
            conversation_key,
            BrowserRuntimeSession {
                session_name: session_id,
                cdp_url: None,
            },
        );
        Ok(())
    }

    pub async fn capture_attached_managed_session_screenshot(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<u8>, String> {
        let conversation_key = normalize_browser_session_conversation_id(conversation_id)?;
        let session = self
            .bound
            .sessions
            .lock()
            .get(conversation_key.as_str())
            .cloned()
            .ok_or_else(|| "managed browser session is not attached".to_string())?;
        if session.cdp_url.is_some() {
            return Err(
                "CDP browser sessions cannot be captured by the managed session UI".to_string(),
            );
        }
        let screenshot_dir = std::env::temp_dir().join("chatos-browser-session-ui");
        std::fs::create_dir_all(screenshot_dir.as_path()).map_err(|error| {
            format!("create browser session screenshot directory failed: {error}")
        })?;
        let screenshot_path = screenshot_dir.join(format!(
            "{}_{}.png",
            session.session_name,
            uuid::Uuid::new_v4().simple()
        ));
        let result = runtime_run_browser_command(
            self.bound.workspace_dir.as_path(),
            &session,
            "screenshot",
            vec![screenshot_path.to_string_lossy().to_string()],
            self.bound.command_timeout_seconds.max(60),
        )
        .await?;
        if !browser_command_succeeded(&result) {
            let _ = tokio::fs::remove_file(screenshot_path.as_path()).await;
            return Err(browser_command_error_text(
                &result,
                "failed to capture managed browser session screenshot",
            ));
        }
        let bytes = tokio::fs::read(screenshot_path.as_path())
            .await
            .map_err(|error| format!("read browser session screenshot failed: {error}"));
        let _ = tokio::fs::remove_file(screenshot_path.as_path()).await;
        let bytes = bytes?;
        if bytes.len() > BROWSER_SESSION_SCREENSHOT_MAX_BYTES {
            return Err(format!(
                "browser session screenshot exceeded {} bytes",
                BROWSER_SESSION_SCREENSHOT_MAX_BYTES
            ));
        }
        Ok(bytes)
    }

    pub async fn close_attached_managed_session(
        &self,
        conversation_id: &str,
    ) -> Result<Value, String> {
        let conversation_key = normalize_browser_session_conversation_id(conversation_id)?;
        let session = self
            .bound
            .sessions
            .lock()
            .get(conversation_key.as_str())
            .cloned()
            .ok_or_else(|| "managed browser session is not attached".to_string())?;
        let result = runtime_run_browser_command(
            self.bound.workspace_dir.as_path(),
            &session,
            "close",
            Vec::new(),
            self.bound.command_timeout_seconds,
        )
        .await;
        actions::mark_browser_session_closed(session.session_name.as_str());
        managed_screencast::stop_browser_screencast(&self.bound, conversation_key.as_str());
        actions::stop_browser_websocket_observer(&self.bound, conversation_key.as_str());
        actions::discard_browser_routes(&self.bound, conversation_key.as_str());
        self.bound.sessions.lock().remove(conversation_key.as_str());
        result
    }

    fn register_tool(
        &mut self,
        name: &str,
        description: &str,
        input_schema: Value,
        handler: ToolHandler,
    ) {
        self.registry
            .register_tool(name, description, input_schema, handler);
    }
    fn register_basic_tools(&mut self, bound: BoundContext) {
        self.register_browser_tabs(bound.clone());
        self.register_browser_tab_new(bound.clone());
        self.register_browser_tab_switch(bound.clone());
        self.register_browser_tab_close(bound.clone());
        self.register_browser_navigate(bound.clone());
        self.register_browser_snapshot(bound.clone());
        self.register_browser_click(bound.clone());
        self.register_browser_type(bound.clone());
        self.register_browser_scroll(bound.clone());
        self.register_browser_back(bound.clone());
        self.register_browser_press(bound.clone());
        self.register_browser_upload(bound.clone());
        self.register_browser_download(bound.clone());
        self.register_browser_get_images(bound);
    }

    fn register_observe_tools(&mut self, bound: BoundContext) {
        self.register_browser_console(bound.clone());
        self.register_browser_network(bound.clone());
        self.register_browser_network_request(bound.clone());
        self.register_browser_har_start(bound.clone());
        self.register_browser_har_stop(bound.clone());
        self.register_browser_websocket_start(bound.clone());
        self.register_browser_websocket_frames(bound.clone());
        self.register_browser_websocket_stop(bound.clone());
        self.register_browser_inspect(bound.clone());
        self.register_browser_research(bound.clone());
        if bound.vision_adapter.is_some() {
            self.register_browser_vision(bound);
        } else {
            self.registry.register_unavailable_tool(
                "browser_vision",
                "browser_vision requires a host-provided vision model adapter".to_string(),
            );
        }
    }
}

fn normalize_browser_session_conversation_id(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 160 {
        return Err("browser session conversation id is invalid".to_string());
    }
    Ok(value.to_string())
}

fn normalize_managed_browser_session_id(value: &str) -> Result<String, String> {
    let value = value.trim();
    if !value.starts_with("h_")
        || value.len() < 4
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err("managed browser session id is invalid".to_string());
    }
    Ok(value.to_string())
}

pub(super) fn async_browser_text_tool_handler<F, Fut>(builder: F) -> ToolHandler
where
    F: Fn(Value, BrowserToolCallContext) -> Result<Fut, String> + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, String>>,
{
    Arc::new(move |args, context| {
        let future = builder(args, context)?;
        let result = block_on_result(future)?;
        Ok(text_result(result))
    })
}

impl Default for BrowserToolsOptions {
    fn default() -> Self {
        Self {
            server_name: "browser_tools".to_string(),
            workspace_dir: PathBuf::from("."),
            command_timeout_seconds: DEFAULT_COMMAND_TIMEOUT_SECONDS,
            max_snapshot_chars: DEFAULT_MAX_SNAPSHOT_CHARS,
            vision_adapter: None,
            route_interception_enabled: false,
            full_cdp_access_enabled: false,
            schema_catalog_only: false,
        }
    }
}

pub fn browser_interactive_approval_command(
    tool_name: &str,
    arguments: &Value,
) -> Result<(String, Vec<String>), String> {
    actions::browser_interactive_approval_command(tool_name, arguments)
}

#[cfg(test)]
mod managed_session_tests {
    use super::*;

    #[test]
    fn attach_managed_session_rejects_cdp_and_malformed_identifiers() {
        let service = BrowserToolsService::new(BrowserToolsOptions {
            workspace_dir: std::env::temp_dir(),
            schema_catalog_only: true,
            ..BrowserToolsOptions::default()
        })
        .expect("browser service");

        assert!(service.attach_managed_session("ui", "cdp_secret").is_err());
        assert!(service.attach_managed_session("ui", "h_bad/value").is_err());
        assert!(service
            .attach_managed_session("ui", "h_valid_session")
            .is_ok());
    }
}
