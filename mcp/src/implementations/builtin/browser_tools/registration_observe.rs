// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use super::actions::{
    browser_console_with_context, browser_har_start_with_context, browser_har_stop_with_context,
    browser_inspect_with_context, browser_network_request_with_context,
    browser_network_with_context, browser_research_with_context, browser_vision_with_context,
    browser_websocket_frames_with_context, browser_websocket_start_with_context,
    browser_websocket_stop_with_context, DEFAULT_BROWSER_HAR_MAX_ENTRIES,
    DEFAULT_BROWSER_NETWORK_BODY_CHARS, DEFAULT_BROWSER_NETWORK_LIMIT,
    DEFAULT_BROWSER_WEBSOCKET_FRAME_LIMIT, DEFAULT_BROWSER_WEBSOCKET_PAYLOAD_CHARS,
    MAX_BROWSER_HAR_ENTRIES, MAX_BROWSER_NETWORK_BODY_CHARS, MAX_BROWSER_NETWORK_LIMIT,
    MAX_BROWSER_WEBSOCKET_FRAME_LIMIT, MAX_BROWSER_WEBSOCKET_PAYLOAD_CHARS,
};
use super::context::{
    optional_bool, optional_trimmed_string, optional_usize, required_trimmed_string,
};
use super::{
    async_browser_text_tool_handler, BoundContext, BrowserToolsService,
    DEFAULT_BROWSER_RESEARCH_LIMIT, MAX_BROWSER_RESEARCH_EXTRACT_URLS, MAX_BROWSER_RESEARCH_LIMIT,
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
            async_browser_text_tool_handler(move |args, browser_context| {
                let clear = optional_bool(&args, "clear");
                let expression = optional_trimmed_string(&args, "expression");
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_console_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        clear,
                        expression,
                    )
                    .await
                })
            }),
        );
    }

    pub(super) fn register_browser_network(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_network",
            "Read a bounded list of real CDP network requests captured for the current page. Query values and credential-like or unknown header values are redacted. Bodies are omitted; use browser_network_request for one explicit request detail.",
            json!({
                "type": "object",
                "properties": {
                    "clear": { "type": "boolean", "default": false },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_BROWSER_NETWORK_LIMIT, "default": DEFAULT_BROWSER_NETWORK_LIMIT },
                    "filter": { "type": "string", "description": "Optional bounded URL substring filter." },
                    "resource_types": {
                        "type": "array",
                        "items": { "type": "string", "enum": ["document", "stylesheet", "image", "media", "font", "script", "xhr", "fetch", "websocket", "other"] },
                        "maxItems": 10
                    },
                    "method": { "type": "string", "description": "Optional HTTP method filter such as GET or POST." },
                    "status": { "type": "string", "description": "Optional status code, class such as 2xx, or range such as 400-499." }
                },
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |args, browser_context| {
                let clear = optional_bool(&args, "clear");
                let limit = optional_usize(&args, "limit")
                    .unwrap_or(DEFAULT_BROWSER_NETWORK_LIMIT)
                    .clamp(1, MAX_BROWSER_NETWORK_LIMIT);
                let filter = optional_trimmed_string(&args, "filter");
                let resource_types = args
                    .get("resource_types")
                    .and_then(|value| value.as_array())
                    .map(|values| {
                        values
                            .iter()
                            .map(|value| {
                                value
                                    .as_str()
                                    .map(ToOwned::to_owned)
                                    .ok_or_else(|| "resource_types must contain only strings".to_string())
                            })
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?
                    .unwrap_or_default();
                let method = optional_trimmed_string(&args, "method");
                let status = optional_trimmed_string(&args, "status");
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_network_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        clear,
                        limit,
                        filter,
                        resource_types,
                        method,
                        status,
                    )
                    .await
                })
            }),
        );
    }

    pub(super) fn register_browser_network_request(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_network_request",
            "Inspect one captured CDP request by request_id. Headers are bounded and credential-like or unknown values stay redacted. Request/response text bodies are returned only when explicitly requested, capped at 64 KiB each, and sensitive JSON/form/common credential fields are redacted.",
            json!({
                "type": "object",
                "properties": {
                    "request_id": { "type": "string" },
                    "include_request_body": { "type": "boolean", "default": false },
                    "include_response_body": { "type": "boolean", "default": false },
                    "max_body_chars": { "type": "integer", "minimum": 1, "maximum": MAX_BROWSER_NETWORK_BODY_CHARS, "default": DEFAULT_BROWSER_NETWORK_BODY_CHARS }
                },
                "required": ["request_id"],
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |args, browser_context| {
                let request_id = required_trimmed_string(&args, "request_id")?;
                let include_request_body = optional_bool(&args, "include_request_body");
                let include_response_body = optional_bool(&args, "include_response_body");
                let max_body_chars = optional_usize(&args, "max_body_chars")
                    .unwrap_or(DEFAULT_BROWSER_NETWORK_BODY_CHARS)
                    .clamp(1, MAX_BROWSER_NETWORK_BODY_CHARS);
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_network_request_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        request_id,
                        include_request_body,
                        include_response_body,
                        max_body_chars,
                    )
                    .await
                })
            }),
        );
    }

    pub(super) fn register_browser_har_start(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_har_start",
            "Start HAR capture for the current managed browser session. This does not publish raw traffic. Use browser_har_stop to write one sanitized, bounded .har file inside the workspace.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |_args, browser_context| {
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_har_start_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                    )
                    .await
                })
            }),
        );
    }

    pub(super) fn register_browser_har_stop(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_har_stop",
            "Stop HAR capture and publish a sanitized workspace-relative .har file without overwriting. Query and cookie values, credential-like and unknown header values are always redacted. Bodies are omitted unless explicitly requested; only bounded text bodies are eligible and sensitive fields are redacted.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "New workspace-relative .har output path whose parent already exists." },
                    "include_request_bodies": { "type": "boolean", "default": false },
                    "include_response_bodies": { "type": "boolean", "default": false },
                    "max_body_chars": { "type": "integer", "minimum": 1, "maximum": MAX_BROWSER_NETWORK_BODY_CHARS, "default": DEFAULT_BROWSER_NETWORK_BODY_CHARS },
                    "max_entries": { "type": "integer", "minimum": 1, "maximum": MAX_BROWSER_HAR_ENTRIES, "default": DEFAULT_BROWSER_HAR_MAX_ENTRIES }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |args, browser_context| {
                let path = required_trimmed_string(&args, "path")?;
                let include_request_bodies = optional_bool(&args, "include_request_bodies");
                let include_response_bodies = optional_bool(&args, "include_response_bodies");
                let max_body_chars = optional_usize(&args, "max_body_chars")
                    .unwrap_or(DEFAULT_BROWSER_NETWORK_BODY_CHARS)
                    .clamp(1, MAX_BROWSER_NETWORK_BODY_CHARS);
                let max_entries = optional_usize(&args, "max_entries")
                    .unwrap_or(DEFAULT_BROWSER_HAR_MAX_ENTRIES)
                    .clamp(1, MAX_BROWSER_HAR_ENTRIES);
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_har_stop_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        path,
                        include_request_bodies,
                        include_response_bodies,
                        max_body_chars,
                        max_entries,
                    )
                    .await
                })
            }),
        );
    }

    pub(super) fn register_browser_websocket_start(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_websocket_start",
            "Start bounded read-only WebSocket frame observation for the current managed page. The observer attaches only through the approved loopback browser CDP endpoint, expires after 30 minutes, stores at most 1000 sanitized frames in memory, and never returns binary payloads.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |_args, browser_context| {
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_websocket_start_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                    )
                    .await
                })
            }),
        );
    }

    pub(super) fn register_browser_websocket_frames(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_websocket_frames",
            "Read bounded WebSocket frame metadata captured after browser_websocket_start. Text payloads are omitted by default and returned only with explicit opt-in after sensitive-field, assignment, bearer-token, URL-query, and likely-token redaction. Binary payloads are never returned.",
            json!({
                "type": "object",
                "properties": {
                    "clear": { "type": "boolean", "default": false },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_BROWSER_WEBSOCKET_FRAME_LIMIT, "default": DEFAULT_BROWSER_WEBSOCKET_FRAME_LIMIT },
                    "request_id": { "type": "string", "description": "Optional captured CDP WebSocket request ID." },
                    "direction": { "type": "string", "enum": ["sent", "received"] },
                    "include_text_payloads": { "type": "boolean", "default": false },
                    "max_payload_chars": { "type": "integer", "minimum": 1, "maximum": MAX_BROWSER_WEBSOCKET_PAYLOAD_CHARS, "default": DEFAULT_BROWSER_WEBSOCKET_PAYLOAD_CHARS }
                },
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |args, browser_context| {
                let clear = optional_bool(&args, "clear");
                let limit = optional_usize(&args, "limit")
                    .unwrap_or(DEFAULT_BROWSER_WEBSOCKET_FRAME_LIMIT)
                    .clamp(1, MAX_BROWSER_WEBSOCKET_FRAME_LIMIT);
                let request_id = optional_trimmed_string(&args, "request_id");
                let direction = optional_trimmed_string(&args, "direction");
                let include_text_payloads = optional_bool(&args, "include_text_payloads");
                let max_payload_chars = optional_usize(&args, "max_payload_chars")
                    .unwrap_or(DEFAULT_BROWSER_WEBSOCKET_PAYLOAD_CHARS)
                    .clamp(1, MAX_BROWSER_WEBSOCKET_PAYLOAD_CHARS);
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_websocket_frames_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        clear,
                        limit,
                        request_id,
                        direction,
                        include_text_payloads,
                        max_payload_chars,
                    )
                    .await
                })
            }),
        );
    }

    pub(super) fn register_browser_websocket_stop(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_websocket_stop",
            "Stop and remove the current session's bounded WebSocket frame observer. Captured data remains process-memory-only and is discarded when the observer is removed.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |_args, browser_context| {
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_websocket_stop_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                    )
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
            async_browser_text_tool_handler(move |args, browser_context| {
                let question = optional_trimmed_string(&args, "question");
                let full = optional_bool(&args, "full");
                let annotate = optional_bool(&args, "annotate");
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_inspect_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        browser_context.caller_model_runtime.clone(),
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
            async_browser_text_tool_handler(move |args, browser_context| {
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
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_research_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        browser_context.caller_model_runtime.clone(),
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
            async_browser_text_tool_handler(move |args, browser_context| {
                let question = required_trimmed_string(&args, "question")?;
                let annotate = optional_bool(&args, "annotate");
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_vision_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        browser_context.caller_model_runtime.clone(),
                        question,
                        annotate,
                    )
                    .await
                })
            }),
        );
    }
}
