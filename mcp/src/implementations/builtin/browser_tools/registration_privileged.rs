// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use super::actions::{
    browser_cdp_command_with_context, browser_route_add_with_context,
    browser_route_clear_with_context, browser_route_list_with_context,
    browser_route_remove_with_context, MAX_BROWSER_ROUTE_PATTERN_CHARS,
};
use super::context::required_trimmed_string;
use super::{async_browser_text_tool_handler, BoundContext, BrowserToolsService};

impl BrowserToolsService {
    pub(super) fn register_route_tools(&mut self, bound: BoundContext) {
        let route_add_context = bound.clone();
        self.register_tool(
            "browser_route_add",
            "Add one explicitly approved, session-scoped network interception rule for the current managed browser. Rules either abort matching HTTP(S) requests or return a fixed bounded JSON body, expire after 30 minutes, and never inject headers, credentials, scripts, or arbitrary CDP commands.",
            json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "maxLength": MAX_BROWSER_ROUTE_PATTERN_CHARS,
                        "description": "HTTP(S) URL glob without credentials, query, fragment, whitespace, or advanced glob syntax; for example https://example.com/api/**."
                    },
                    "action": { "type": "string", "enum": ["abort", "mock_json"] },
                    "body": {
                        "description": "Required only for mock_json. Any JSON value is accepted up to the bounded serialized size."
                    }
                },
                "required": ["pattern", "action"],
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |args, browser_context| {
                let ctx = route_add_context.clone();
                Ok(async move {
                    browser_route_add_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        args,
                    )
                    .await
                })
            }),
        );

        let route_list_context = bound.clone();
        self.register_tool(
            "browser_route_list",
            "List the active ChatOS-owned interception rules for the current managed browser session. Mock response bodies are never returned; only their size and SHA-256 are shown.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |_args, browser_context| {
                let ctx = route_list_context.clone();
                Ok(async move {
                    browser_route_list_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                    )
                    .await
                })
            }),
        );

        let route_remove_context = bound.clone();
        self.register_tool(
            "browser_route_remove",
            "Remove one ChatOS-owned interception rule by its opaque route_id. The underlying URL pattern is resolved from session memory rather than accepted from the caller.",
            json!({
                "type": "object",
                "properties": {
                    "route_id": { "type": "string", "pattern": "^r_[0-9a-f]{32}$" }
                },
                "required": ["route_id"],
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |args, browser_context| {
                let route_id = required_trimmed_string(&args, "route_id")?;
                let ctx = route_remove_context.clone();
                Ok(async move {
                    browser_route_remove_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        route_id,
                    )
                    .await
                })
            }),
        );

        self.register_tool(
            "browser_route_clear",
            "Remove all ChatOS-owned interception rules from the current managed browser session. This is a recovery action and does not require an additional approval.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |_args, browser_context| {
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_route_clear_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                    )
                    .await
                })
            }),
        );
    }

    pub(super) fn register_full_cdp_tool(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_cdp_command",
            "HIGH RISK developer-mode tool: execute one explicitly approved Chrome DevTools Protocol command against the current managed browser or active page. The loopback debugger endpoint is never exposed. Every exact method and params object requires local user approval; responses are bounded but otherwise may contain sensitive browser internals.",
            json!({
                "type": "object",
                "properties": {
                    "target": { "type": "string", "enum": ["page", "browser"], "default": "page" },
                    "method": { "type": "string", "pattern": "^[A-Za-z][A-Za-z0-9]*\\.[A-Za-z][A-Za-z0-9]*$", "maxLength": 160 },
                    "params": { "type": "object", "default": {} },
                    "timeout_seconds": { "type": "integer", "minimum": 1, "maximum": 15, "default": 5 }
                },
                "required": ["method"],
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |args, browser_context| {
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_cdp_command_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        args,
                    )
                    .await
                })
            }),
        );
    }
}
