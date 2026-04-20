mod actions;
mod context;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{json, Value};

use crate::builtin::browser_runtime::{browser_backend_available, BrowserRuntimeSession};
use crate::core::async_bridge::block_on_result;
use crate::core::tool_io::text_result;

use self::actions::{
    browser_back_with_context, browser_click_with_context, browser_console_with_context,
    browser_get_images_with_context, browser_inspect_with_context, browser_navigate_with_context,
    browser_press_with_context, browser_research_with_context, browser_scroll_with_context,
    browser_snapshot_with_context, browser_type_with_context, browser_vision_with_context,
};
use self::context::{
    optional_bool, optional_trimmed_string, optional_usize, required_trimmed_string,
};

const DEFAULT_COMMAND_TIMEOUT_SECONDS: u64 = 30;
const DEFAULT_MAX_SNAPSHOT_CHARS: usize = 8_000;

#[derive(Debug, Clone)]
pub struct BrowserToolsOptions {
    pub server_name: String,
    pub workspace_dir: PathBuf,
    pub command_timeout_seconds: u64,
    pub max_snapshot_chars: usize,
}

#[derive(Clone)]
pub struct BrowserToolsService {
    tools: HashMap<String, Tool>,
    unavailable_tools: HashMap<String, String>,
}

#[derive(Clone)]
struct Tool {
    name: String,
    description: String,
    input_schema: Value,
    handler: ToolHandler,
}

type ToolHandler = Arc<dyn Fn(Value, Option<&str>) -> Result<Value, String> + Send + Sync>;

#[derive(Clone)]
pub(super) struct BoundContext {
    pub(super) _server_name: String,
    pub(super) workspace_dir: PathBuf,
    pub(super) command_timeout_seconds: u64,
    pub(super) max_snapshot_chars: usize,
    pub(super) sessions: Arc<Mutex<HashMap<String, BrowserRuntimeSession>>>,
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
            tools: HashMap::new(),
            unavailable_tools: HashMap::new(),
        };
        let bound = BoundContext {
            _server_name: opts.server_name,
            workspace_dir,
            command_timeout_seconds: opts
                .command_timeout_seconds
                .max(DEFAULT_COMMAND_TIMEOUT_SECONDS),
            max_snapshot_chars: opts
                .max_snapshot_chars
                .max(1)
                .min(DEFAULT_MAX_SNAPSHOT_CHARS),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        };

        if let Err(reason) = browser_backend_available() {
            for name in [
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
            ] {
                service
                    .unavailable_tools
                    .insert(name.to_string(), reason.clone());
            }
        } else {
            service.register_browser_navigate(bound.clone());
            service.register_browser_snapshot(bound.clone());
            service.register_browser_click(bound.clone());
            service.register_browser_type(bound.clone());
            service.register_browser_scroll(bound.clone());
            service.register_browser_back(bound.clone());
            service.register_browser_press(bound.clone());
            service.register_browser_console(bound.clone());
            service.register_browser_get_images(bound.clone());
            service.register_browser_inspect(bound.clone());
            service.register_browser_research(bound.clone());
            service.register_browser_vision(bound);
        }

        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema
                })
            })
            .collect()
    }

    pub fn call_tool(
        &self,
        name: &str,
        args: Value,
        conversation_id: Option<&str>,
    ) -> Result<Value, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;
        (tool.handler)(args, conversation_id)
    }

    pub fn unavailable_tools(&self) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = self
            .unavailable_tools
            .iter()
            .map(|(name, reason)| (name.clone(), reason.clone()))
            .collect();
        pairs.sort_by(|left, right| left.0.cmp(&right.0));
        pairs
    }

    fn register_tool(
        &mut self,
        name: &str,
        description: &str,
        input_schema: Value,
        handler: ToolHandler,
    ) {
        self.tools.insert(
            name.to_string(),
            Tool {
                name: name.to_string(),
                description: description.to_string(),
                input_schema,
                handler,
            },
        );
    }

    fn register_browser_navigate(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_navigate",
            "Navigate to a URL in browser automation backend and return a compact snapshot. After navigation, prefer browser_inspect before clicking or typing so refs and page state are current.",
            json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string" }
                },
                "required": ["url"],
                "additionalProperties": false
            }),
            Arc::new(move |args, conversation_id| {
                let url = required_trimmed_string(&args, "url")?;
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    browser_navigate_with_context(ctx, conversation_id, url).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_browser_snapshot(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_snapshot",
            "Get the current page snapshot text and element refs (compact by default). Prefer browser_inspect for a one-shot overview with console/vision context; use browser_snapshot when you specifically need raw refs or a full snapshot dump.",
            json!({
                "type": "object",
                "properties": {
                    "full": { "type": "boolean", "default": false }
                },
                "additionalProperties": false
            }),
            Arc::new(move |args, conversation_id| {
                let full = optional_bool(&args, "full");
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    browser_snapshot_with_context(ctx, conversation_id, full).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_browser_click(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_click",
            "Click an element reference from browser_snapshot/browser_inspect output (e.g. @e5). Re-run browser_inspect or browser_snapshot after major page changes to refresh refs.",
            json!({
                "type": "object",
                "properties": {
                    "ref": { "type": "string" }
                },
                "required": ["ref"],
                "additionalProperties": false
            }),
            Arc::new(move |args, conversation_id| {
                let reference = required_trimmed_string(&args, "ref")?;
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    browser_click_with_context(ctx, conversation_id, reference).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_browser_type(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_type",
            "Type text into an element reference from browser_snapshot/browser_inspect output. Re-run browser_inspect or browser_snapshot if the page changed and refs may be stale.",
            json!({
                "type": "object",
                "properties": {
                    "ref": { "type": "string" },
                    "text": { "type": "string" }
                },
                "required": ["ref", "text"],
                "additionalProperties": false
            }),
            Arc::new(move |args, conversation_id| {
                let reference = required_trimmed_string(&args, "ref")?;
                let text = required_trimmed_string(&args, "text")?;
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    browser_type_with_context(ctx, conversation_id, reference, text).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_browser_scroll(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_scroll",
            "Scroll the current browser page up or down. After scrolling reveals new content, prefer browser_inspect or browser_snapshot to refresh what is visible.",
            json!({
                "type": "object",
                "properties": {
                    "direction": { "type": "string", "enum": ["up", "down"] }
                },
                "required": ["direction"],
                "additionalProperties": false
            }),
            Arc::new(move |args, conversation_id| {
                let direction = required_trimmed_string(&args, "direction")?;
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    browser_scroll_with_context(ctx, conversation_id, direction).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_browser_back(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_back",
            "Navigate browser history back. Prefer browser_inspect afterwards if you need the refreshed page state before acting.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            Arc::new(move |_args, conversation_id| {
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    browser_back_with_context(ctx, conversation_id).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_browser_press(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_press",
            "Press a keyboard key in the active browser page. Use this for Enter/Escape/Tab-style actions, then inspect again if the page state changed.",
            json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string" }
                },
                "required": ["key"],
                "additionalProperties": false
            }),
            Arc::new(move |args, conversation_id| {
                let key = required_trimmed_string(&args, "key")?;
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    browser_press_with_context(ctx, conversation_id, key).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_browser_console(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_console",
            "Get browser console/errors or evaluate JavaScript in the current page. Prefer browser_inspect for the default observe-first workflow; use browser_console when you specifically need raw console output, JS evaluation, or to clear/read console state.",
            json!({
                "type": "object",
                "properties": {
                    "clear": { "type": "boolean", "default": false },
                    "expression": { "type": "string" }
                },
                "additionalProperties": false
            }),
            Arc::new(move |args, conversation_id| {
                let clear = optional_bool(&args, "clear");
                let expression = optional_trimmed_string(&args, "expression");
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    browser_console_with_context(ctx, conversation_id, clear, expression).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_browser_get_images(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_get_images",
            "List visible images from the active browser page. Use when image assets matter more than generic page refs.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            Arc::new(move |_args, conversation_id| {
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    browser_get_images_with_context(ctx, conversation_id).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_browser_inspect(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_inspect",
            "Observe the current page before acting. This is the default read-only browser tool: it returns page metadata, snapshot refs, console summary, and optional screenshot-based vision analysis for a question in one step.",
            json!({
                "type": "object",
                "properties": {
                    "question": { "type": "string" },
                    "full": { "type": "boolean", "default": false },
                    "annotate": { "type": "boolean", "default": false }
                },
                "additionalProperties": false
            }),
            Arc::new(move |args, conversation_id| {
                let question = optional_trimmed_string(&args, "question");
                let full = optional_bool(&args, "full");
                let annotate = optional_bool(&args, "annotate");
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    browser_inspect_with_context(ctx, conversation_id, question, full, annotate).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_browser_research(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_research",
            "Observe the current browser page and optionally supplement it with public web research in one step. Best when the answer depends on both the current page and external verification or source-backed context.",
            json!({
                "type": "object",
                "properties": {
                    "question": { "type": "string" },
                    "web_query": { "type": "string" },
                    "include_web": { "type": "boolean", "default": true },
                    "web_limit": { "type": "integer", "minimum": 1, "maximum": 20 },
                    "extract_top": { "type": "integer", "minimum": 0, "maximum": 5 },
                    "full": { "type": "boolean", "default": false },
                    "annotate": { "type": "boolean", "default": false }
                },
                "required": ["question"],
                "additionalProperties": false
            }),
            Arc::new(move |args, conversation_id| {
                let question = required_trimmed_string(&args, "question")?;
                let web_query = optional_trimmed_string(&args, "web_query");
                let include_web = args
                    .get("include_web")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true);
                let web_limit = optional_usize(&args, "web_limit");
                let extract_top = optional_usize(&args, "extract_top");
                let full = optional_bool(&args, "full");
                let annotate = optional_bool(&args, "annotate");
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    browser_research_with_context(
                        ctx,
                        conversation_id,
                        question,
                        web_query,
                        include_web,
                        web_limit,
                        extract_top,
                        full,
                        annotate,
                    )
                    .await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_browser_vision(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_vision",
            "Capture a screenshot and analyze it with the best available vision model, preferring current session/contact context and automatically falling back between responses/chat-completions transports when needed. Use this when visual layout or screenshot-only details matter; browser_inspect with question or browser_research are usually better first steps if you also need refs, console context, or outside verification.",
            json!({
                "type": "object",
                "properties": {
                    "question": { "type": "string" },
                    "annotate": { "type": "boolean", "default": false }
                },
                "required": ["question"],
                "additionalProperties": false
            }),
            Arc::new(move |args, conversation_id| {
                let question = required_trimmed_string(&args, "question")?;
                let annotate = optional_bool(&args, "annotate");
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    browser_vision_with_context(ctx, conversation_id, question, annotate).await
                })?;
                Ok(text_result(result))
            }),
        );
    }
}

impl Default for BrowserToolsOptions {
    fn default() -> Self {
        Self {
            server_name: "browser_tools".to_string(),
            workspace_dir: PathBuf::from("."),
            command_timeout_seconds: DEFAULT_COMMAND_TIMEOUT_SECONDS,
            max_snapshot_chars: DEFAULT_MAX_SNAPSHOT_CHARS,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use uuid::Uuid;

    use super::{BrowserToolsOptions, BrowserToolsService};

    #[test]
    fn list_tools_contains_browser_navigate_and_vision() {
        let dir = std::env::temp_dir().join(format!("browser_tools_test_{}", Uuid::new_v4()));
        let service = BrowserToolsService::new(BrowserToolsOptions {
            workspace_dir: PathBuf::from(&dir),
            ..Default::default()
        })
        .expect("init browser tools");

        let names: Vec<String> = service
            .list_tools()
            .into_iter()
            .filter_map(|item| {
                item.get("name")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .collect();
        let unavailable = service.unavailable_tools();
        if unavailable.is_empty() {
            assert!(names.contains(&"browser_navigate".to_string()));
            assert!(names.contains(&"browser_inspect".to_string()));
            assert!(names.contains(&"browser_research".to_string()));
            assert!(names.contains(&"browser_vision".to_string()));
        } else {
            assert!(names.is_empty());
            assert_eq!(unavailable.len(), 12);
            assert!(unavailable
                .iter()
                .all(|(_, reason)| reason.contains("agent-browser")));
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn call_unknown_tool_returns_error() {
        let dir = std::env::temp_dir().join(format!("browser_tools_test_{}", Uuid::new_v4()));
        let service = BrowserToolsService::new(BrowserToolsOptions {
            workspace_dir: PathBuf::from(&dir),
            ..Default::default()
        })
        .expect("init browser tools");
        let err = service
            .call_tool("browser_not_exists", serde_json::json!({}), None)
            .expect_err("unknown tool should fail");
        assert!(err.contains("Tool not found"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
