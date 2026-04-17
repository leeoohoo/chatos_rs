mod actions;
mod context;
pub(crate) mod provider;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::core::async_bridge::block_on_result;
use crate::core::tool_io::text_result;

use self::actions::{web_extract_with_context, web_research_with_context, web_search_with_context};
use self::context::{optional_usize, required_string_array, required_trimmed_string};

const DEFAULT_REQUEST_TIMEOUT_SECONDS: u64 = 30;
const DEFAULT_SEARCH_LIMIT: usize = 5;
const MAX_SEARCH_LIMIT: usize = 20;
const MAX_EXTRACT_URLS: usize = 5;
const DEFAULT_MAX_EXTRACT_CHARS: usize = 100_000;

#[derive(Debug, Clone)]
pub struct WebToolsOptions {
    pub server_name: String,
    pub workspace_dir: PathBuf,
    pub request_timeout_seconds: u64,
    pub default_search_limit: usize,
    pub max_search_limit: usize,
    pub max_extract_urls: usize,
    pub max_extract_chars: usize,
}

#[derive(Clone)]
pub struct WebToolsService {
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

type ToolHandler = Arc<dyn Fn(Value) -> Result<Value, String> + Send + Sync>;

#[derive(Clone)]
pub(super) struct BoundContext {
    pub(super) _server_name: String,
    pub(super) workspace_dir: PathBuf,
    pub(super) client: reqwest::Client,
    pub(super) browser_command_timeout_seconds: u64,
    pub(super) default_search_limit: usize,
    pub(super) max_search_limit: usize,
    pub(super) max_extract_urls: usize,
    pub(super) max_extract_chars: usize,
}

impl WebToolsService {
    pub fn new(opts: WebToolsOptions) -> Result<Self, String> {
        let timeout = opts
            .request_timeout_seconds
            .max(DEFAULT_REQUEST_TIMEOUT_SECONDS);
        std::fs::create_dir_all(&opts.workspace_dir)
            .map_err(|err| format!("create web_tools workspace dir failed: {}", err))?;
        let workspace_dir = opts
            .workspace_dir
            .canonicalize()
            .unwrap_or_else(|_| opts.workspace_dir.clone());
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout))
            .user_agent("chatos-rs-web-tools/0.1")
            .build()
            .map_err(|err| format!("build web_tools client failed: {}", err))?;

        let mut service = Self {
            tools: HashMap::new(),
            unavailable_tools: HashMap::new(),
        };
        let bound = BoundContext {
            _server_name: opts.server_name,
            workspace_dir,
            client,
            browser_command_timeout_seconds: timeout,
            default_search_limit: opts.default_search_limit.clamp(1, MAX_SEARCH_LIMIT),
            max_search_limit: opts.max_search_limit.max(1).min(MAX_SEARCH_LIMIT),
            max_extract_urls: opts.max_extract_urls.max(1).min(MAX_EXTRACT_URLS),
            max_extract_chars: opts.max_extract_chars.max(1).min(DEFAULT_MAX_EXTRACT_CHARS),
        };

        service.register_web_search(bound.clone());
        service.register_web_research(bound.clone());
        service.register_web_extract(bound);
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

    pub fn call_tool(&self, name: &str, args: Value) -> Result<Value, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;
        (tool.handler)(args)
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

    fn register_web_search(&mut self, bound: BoundContext) {
        self.register_tool(
            "web_search",
            "Search the public web with chatos native search. Use this when you mainly need discovery or candidate URLs; prefer web_research when you also want extracted source content in one step.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query text" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 20 }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            Arc::new(move |args| {
                let query = required_trimmed_string(&args, "query")?;
                let limit = optional_usize(&args, "limit");
                let ctx = bound.clone();
                let result =
                    block_on_result(
                        async move { web_search_with_context(ctx, query, limit).await },
                    )?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_web_extract(&mut self, bound: BoundContext) {
        self.register_tool(
            "web_extract",
            "Extract page content from known URLs with chatos native extraction. Use this after web_search or when the user already provided target links. Prefer web_research when the query is still open and you need both discovery and extraction.",
            json!({
                "type": "object",
                "properties": {
                    "urls": {
                        "type": "array",
                        "items": { "type": "string" },
                        "maxItems": 5
                    }
                },
                "required": ["urls"],
                "additionalProperties": false
            }),
            Arc::new(move |args| {
                let urls = required_string_array(&args, "urls")?;
                let ctx = bound.clone();
                let result =
                    block_on_result(async move { web_extract_with_context(ctx, urls).await })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_web_research(&mut self, bound: BoundContext) {
        self.register_tool(
            "web_research",
            "Run web_search and then extract the top results into a single research bundle. Prefer this over plain web_search when the model needs both discovery and source content, especially when the current page is insufficient and external verification or source-backed synthesis is needed.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Research query text" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 20 },
                    "extract_top": { "type": "integer", "minimum": 0, "maximum": 5 }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            Arc::new(move |args| {
                let query = required_trimmed_string(&args, "query")?;
                let limit = optional_usize(&args, "limit");
                let extract_top = optional_usize(&args, "extract_top");
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    web_research_with_context(ctx, query, limit, extract_top).await
                })?;
                Ok(text_result(result))
            }),
        );
    }
}

impl Default for WebToolsOptions {
    fn default() -> Self {
        Self {
            server_name: "web_tools".to_string(),
            workspace_dir: PathBuf::from("."),
            request_timeout_seconds: DEFAULT_REQUEST_TIMEOUT_SECONDS,
            default_search_limit: DEFAULT_SEARCH_LIMIT,
            max_search_limit: MAX_SEARCH_LIMIT,
            max_extract_urls: MAX_EXTRACT_URLS,
            max_extract_chars: DEFAULT_MAX_EXTRACT_CHARS,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{WebToolsOptions, WebToolsService};

    #[test]
    fn list_tools_contains_web_search_and_extract() {
        let service = WebToolsService::new(WebToolsOptions::default()).expect("init web tools");
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
        for tool_name in ["web_search", "web_extract", "web_research"] {
            let is_unavailable = unavailable.iter().any(|(name, _)| name == tool_name);
            if is_unavailable {
                assert!(
                    !names.contains(&tool_name.to_string()),
                    "{} should be hidden when unavailable",
                    tool_name
                );
            } else {
                assert!(
                    names.contains(&tool_name.to_string()),
                    "{} should be listed when available",
                    tool_name
                );
            }
        }
    }

    #[test]
    fn web_search_requires_query_arg() {
        let service = WebToolsService::new(WebToolsOptions::default()).expect("init web tools");
        let search_unavailable = service
            .unavailable_tools()
            .iter()
            .any(|(name, _)| name == "web_search");
        if !search_unavailable {
            let err = service
                .call_tool("web_search", json!({}))
                .expect_err("missing query should fail");
            assert!(err.contains("query"));
        } else {
            let err = service
                .call_tool("web_search", json!({}))
                .expect_err("tool should be hidden without key");
            assert!(err.contains("Tool not found"));
        }
    }

    #[test]
    fn web_extract_requires_urls_arg() {
        let service = WebToolsService::new(WebToolsOptions::default()).expect("init web tools");
        let extract_unavailable = service
            .unavailable_tools()
            .iter()
            .any(|(name, _)| name == "web_extract");
        if !extract_unavailable {
            let err = service
                .call_tool("web_extract", json!({}))
                .expect_err("missing urls should fail");
            assert!(err.contains("urls"));
        } else {
            let err = service
                .call_tool("web_extract", json!({}))
                .expect_err("tool should be hidden without key");
            assert!(err.contains("Tool not found"));
        }
    }

    #[test]
    fn web_research_requires_query_arg() {
        let service = WebToolsService::new(WebToolsOptions::default()).expect("init web tools");
        let research_unavailable = service
            .unavailable_tools()
            .iter()
            .any(|(name, _)| name == "web_research");
        if !research_unavailable {
            let err = service
                .call_tool("web_research", json!({}))
                .expect_err("missing query should fail");
            assert!(err.contains("query"));
        } else {
            let err = service
                .call_tool("web_research", json!({}))
                .expect_err("tool should be hidden without search availability");
            assert!(err.contains("Tool not found"));
        }
    }
}
