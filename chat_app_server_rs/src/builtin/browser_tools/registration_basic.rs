use serde_json::json;

use crate::core::tool_registry::async_text_tool_handler_with_optional_string;

use super::actions::{
    browser_back_with_context, browser_click_with_context, browser_get_images_with_context,
    browser_navigate_with_context, browser_press_with_context, browser_scroll_with_context,
    browser_snapshot_with_context, browser_type_with_context,
};
use super::context::{optional_bool, required_trimmed_string};
use super::{BoundContext, BrowserToolsService};

impl BrowserToolsService {
    pub(super) fn register_browser_navigate(&mut self, bound: BoundContext) {
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
            async_text_tool_handler_with_optional_string(move |args, conversation_id| {
                let url = required_trimmed_string(&args, "url")?;
                let ctx = bound.clone();
                Ok(async move {
                    browser_navigate_with_context(ctx, conversation_id.as_deref(), url).await
                })
            }),
        );
    }

    pub(super) fn register_browser_snapshot(&mut self, bound: BoundContext) {
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
            async_text_tool_handler_with_optional_string(move |args, conversation_id| {
                let full = optional_bool(&args, "full");
                let ctx = bound.clone();
                Ok(async move {
                    browser_snapshot_with_context(ctx, conversation_id.as_deref(), full).await
                })
            }),
        );
    }

    pub(super) fn register_browser_click(&mut self, bound: BoundContext) {
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
            async_text_tool_handler_with_optional_string(move |args, conversation_id| {
                let reference = required_trimmed_string(&args, "ref")?;
                let ctx = bound.clone();
                Ok(async move {
                    browser_click_with_context(ctx, conversation_id.as_deref(), reference).await
                })
            }),
        );
    }

    pub(super) fn register_browser_type(&mut self, bound: BoundContext) {
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
            async_text_tool_handler_with_optional_string(move |args, conversation_id| {
                let reference = required_trimmed_string(&args, "ref")?;
                let text = required_trimmed_string(&args, "text")?;
                let ctx = bound.clone();
                Ok(async move {
                    browser_type_with_context(ctx, conversation_id.as_deref(), reference, text)
                        .await
                })
            }),
        );
    }

    pub(super) fn register_browser_scroll(&mut self, bound: BoundContext) {
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
            async_text_tool_handler_with_optional_string(move |args, conversation_id| {
                let direction = required_trimmed_string(&args, "direction")?;
                let ctx = bound.clone();
                Ok(async move {
                    browser_scroll_with_context(ctx, conversation_id.as_deref(), direction).await
                })
            }),
        );
    }

    pub(super) fn register_browser_back(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_back",
            "Navigate browser history back. Prefer browser_inspect afterwards if you need the refreshed page state before acting.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            async_text_tool_handler_with_optional_string(move |_args, conversation_id| {
                let ctx = bound.clone();
                Ok(async move {
                    browser_back_with_context(ctx, conversation_id.as_deref()).await
                })
            }),
        );
    }

    pub(super) fn register_browser_press(&mut self, bound: BoundContext) {
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
            async_text_tool_handler_with_optional_string(move |args, conversation_id| {
                let key = required_trimmed_string(&args, "key")?;
                let ctx = bound.clone();
                Ok(async move {
                    browser_press_with_context(ctx, conversation_id.as_deref(), key).await
                })
            }),
        );
    }

    pub(super) fn register_browser_get_images(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_get_images",
            "List visible images from the active browser page. Use when image assets matter more than generic page refs.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            async_text_tool_handler_with_optional_string(move |_args, conversation_id| {
                let ctx = bound.clone();
                Ok(async move {
                    browser_get_images_with_context(ctx, conversation_id.as_deref()).await
                })
            }),
        );
    }
}
