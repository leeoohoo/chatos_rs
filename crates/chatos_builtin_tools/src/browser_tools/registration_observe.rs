use serde_json::json;

use crate::tool_registry::async_text_tool_handler_with_optional_string;

use super::actions::{
    browser_console_with_context, browser_inspect_with_context, browser_research_with_context,
    browser_vision_with_context,
};
use super::context::{
    optional_bool, optional_trimmed_string, optional_usize, required_trimmed_string,
};
use super::{
    BoundContext, BrowserToolsService, DEFAULT_BROWSER_RESEARCH_LIMIT,
    MAX_BROWSER_RESEARCH_EXTRACT_URLS, MAX_BROWSER_RESEARCH_LIMIT,
};

impl BrowserToolsService {
    pub(super) fn register_browser_console(&mut self, bound: BoundContext) {
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
            async_text_tool_handler_with_optional_string(move |args, conversation_id| {
                let clear = optional_bool(&args, "clear");
                let expression = optional_trimmed_string(&args, "expression");
                let ctx = bound.clone();
                Ok(async move {
                    browser_console_with_context(ctx, conversation_id.as_deref(), clear, expression)
                        .await
                })
            }),
        );
    }

    pub(super) fn register_browser_inspect(&mut self, bound: BoundContext) {
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
            async_text_tool_handler_with_optional_string(move |args, conversation_id| {
                let question = optional_trimmed_string(&args, "question");
                let full = optional_bool(&args, "full");
                let annotate = optional_bool(&args, "annotate");
                let ctx = bound.clone();
                Ok(async move {
                    browser_inspect_with_context(
                        ctx,
                        conversation_id.as_deref(),
                        question,
                        full,
                        annotate,
                    )
                    .await
                })
            }),
        );
    }

    pub(super) fn register_browser_research(&mut self, bound: BoundContext) {
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
            async_text_tool_handler_with_optional_string(move |args, conversation_id| {
                let question = required_trimmed_string(&args, "question")?;
                let web_query = optional_trimmed_string(&args, "web_query");
                let include_web = args
                    .get("include_web")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true);
                let web_limit = optional_usize(&args, "web_limit")
                    .map(|value| value.clamp(1, MAX_BROWSER_RESEARCH_LIMIT))
                    .or(Some(DEFAULT_BROWSER_RESEARCH_LIMIT));
                let extract_top = optional_usize(&args, "extract_top")
                    .map(|value| value.min(MAX_BROWSER_RESEARCH_EXTRACT_URLS));
                let full = optional_bool(&args, "full");
                let annotate = optional_bool(&args, "annotate");
                let ctx = bound.clone();
                Ok(async move {
                    browser_research_with_context(
                        ctx,
                        conversation_id.as_deref(),
                        question,
                        web_query,
                        include_web,
                        web_limit,
                        extract_top,
                        full,
                        annotate,
                    )
                    .await
                })
            }),
        );
    }

    pub(super) fn register_browser_vision(&mut self, bound: BoundContext) {
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
            async_text_tool_handler_with_optional_string(move |args, conversation_id| {
                let question = required_trimmed_string(&args, "question")?;
                let annotate = optional_bool(&args, "annotate");
                let ctx = bound.clone();
                Ok(async move {
                    browser_vision_with_context(
                        ctx,
                        conversation_id.as_deref(),
                        question,
                        annotate,
                    )
                    .await
                })
            }),
        );
    }
}
