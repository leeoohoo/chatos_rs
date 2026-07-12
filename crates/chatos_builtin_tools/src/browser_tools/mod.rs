// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod actions;
mod context;
mod registration_basic;
mod registration_observe;

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

use crate::browser_runtime::{browser_backend_available, BrowserRuntimeSession};
use crate::tool_registry::ToolRegistry;
use crate::tool_registry::{block_on_result, text_result};

const DEFAULT_COMMAND_TIMEOUT_SECONDS: u64 = 30;
const DEFAULT_MAX_SNAPSHOT_CHARS: usize = 8_000;
pub(super) const DEFAULT_BROWSER_RESEARCH_LIMIT: usize = 5;
pub(super) const MAX_BROWSER_RESEARCH_LIMIT: usize = 20;
pub(super) const MAX_BROWSER_RESEARCH_EXTRACT_URLS: usize = 5;
const BROWSER_TOOL_NAMES: [&str; 12] = [
    "browser_navigate",
    "browser_snapshot",
    "browser_click",
    "browser_type",
    "browser_scroll",
    "browser_back",
    "browser_press",
    "browser_console",
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
}

#[derive(Clone)]
pub struct BrowserToolsService {
    registry: ToolRegistry<ToolHandler>,
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
        let mut service = Self {
            registry: ToolRegistry::new(),
        };
        let bound = BoundContext {
            _server_name: opts.server_name,
            workspace_dir,
            command_timeout_seconds: opts
                .command_timeout_seconds
                .max(DEFAULT_COMMAND_TIMEOUT_SECONDS),
            max_snapshot_chars: opts.max_snapshot_chars.clamp(1, DEFAULT_MAX_SNAPSHOT_CHARS),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            vision_adapter: opts.vision_adapter,
        };

        if let Err(reason) = browser_backend_available() {
            service
                .registry
                .register_unavailable_tools(BROWSER_TOOL_NAMES, reason.clone());
        } else {
            service.register_basic_tools(bound.clone());
            service.register_observe_tools(bound);
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
        self.register_browser_navigate(bound.clone());
        self.register_browser_snapshot(bound.clone());
        self.register_browser_click(bound.clone());
        self.register_browser_type(bound.clone());
        self.register_browser_scroll(bound.clone());
        self.register_browser_back(bound.clone());
        self.register_browser_press(bound.clone());
        self.register_browser_get_images(bound);
    }

    fn register_observe_tools(&mut self, bound: BoundContext) {
        self.register_browser_console(bound.clone());
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
        }
    }
}
