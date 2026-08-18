// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use super::actions::{
    browser_back_with_context, browser_click_with_context, browser_download_with_context,
    browser_get_images_with_context, browser_navigate_with_context, browser_press_with_context,
    browser_scroll_with_context, browser_set_viewport_with_context, browser_snapshot_with_context,
    browser_tab_close_with_context, browser_tab_new_with_context, browser_tab_switch_with_context,
    browser_tabs_with_context, browser_type_with_context, browser_upload_with_context,
    MAX_BROWSER_UPLOAD_FILES,
};
use super::context::{optional_bool, optional_trimmed_string, required_trimmed_string};
use super::{async_browser_text_tool_handler, BoundContext, BrowserToolsService};

impl BrowserToolsService {
    pub(super) fn register_browser_tabs(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_tabs",
            "List open browser tabs using stable session-scoped tab IDs. Returned web URLs have credentials removed and query values redacted; non-web URLs are omitted.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |_args, browser_context| {
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_tabs_with_context(ctx, browser_context.conversation_id.as_deref()).await
                })
            }),
        );
    }

    pub(super) fn register_browser_tab_new(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_tab_new",
            "Open a new browser tab and make it active. The optional URL follows the same navigation policy as browser_navigate. Returns refreshed stable tab IDs and page state.",
            json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string" }
                },
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |args, browser_context| {
                let url = optional_trimmed_string(&args, "url");
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_tab_new_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        url,
                    )
                    .await
                })
            }),
        );
    }

    pub(super) fn register_browser_tab_switch(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_tab_switch",
            "Switch the active browser page using a stable tab_id returned by browser_tabs. Returns the refreshed tab list and current page snapshot.",
            json!({
                "type": "object",
                "properties": {
                    "tab_id": { "type": "string", "pattern": "^t[0-9]+$" }
                },
                "required": ["tab_id"],
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |args, browser_context| {
                let tab_id = required_trimmed_string(&args, "tab_id")?;
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_tab_switch_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        tab_id,
                    )
                    .await
                })
            }),
        );
    }

    pub(super) fn register_browser_tab_close(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_tab_close",
            "Close a specific browser tab using a stable tab_id returned by browser_tabs. The last remaining page tab cannot be closed.",
            json!({
                "type": "object",
                "properties": {
                    "tab_id": { "type": "string", "pattern": "^t[0-9]+$" }
                },
                "required": ["tab_id"],
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |args, browser_context| {
                let tab_id = required_trimmed_string(&args, "tab_id")?;
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_tab_close_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        tab_id,
                    )
                    .await
                })
            }),
        );
    }

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
            async_browser_text_tool_handler(move |args, browser_context| {
                let url = required_trimmed_string(&args, "url")?;
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_navigate_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        url,
                    )
                    .await
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
            async_browser_text_tool_handler(move |args, browser_context| {
                let full = optional_bool(&args, "full");
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_snapshot_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        full,
                    )
                    .await
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
            async_browser_text_tool_handler(move |args, browser_context| {
                let reference = required_trimmed_string(&args, "ref")?;
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_click_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        reference,
                    )
                    .await
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
            async_browser_text_tool_handler(move |args, browser_context| {
                let reference = required_trimmed_string(&args, "ref")?;
                let text = required_trimmed_string(&args, "text")?;
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_type_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        reference,
                        text,
                    )
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
            async_browser_text_tool_handler(move |args, browser_context| {
                let direction = required_trimmed_string(&args, "direction")?;
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_scroll_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        direction,
                    )
                    .await
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
            async_browser_text_tool_handler(move |_args, browser_context| {
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_back_with_context(ctx, browser_context.conversation_id.as_deref()).await
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
            async_browser_text_tool_handler(move |args, browser_context| {
                let key = required_trimmed_string(&args, "key")?;
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_press_with_context(ctx, browser_context.conversation_id.as_deref(), key)
                        .await
                })
            }),
        );
    }

    pub(super) fn register_browser_set_viewport(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_set_viewport",
            "Set the current managed browser viewport to bounded CSS-pixel dimensions and verify the resulting window.innerWidth/innerHeight. Use this for real responsive-layout testing instead of window.resizeTo or high-risk arbitrary CDP commands.",
            json!({
                "type": "object",
                "properties": {
                    "width": { "type": "integer", "minimum": 240, "maximum": 3840 },
                    "height": { "type": "integer", "minimum": 240, "maximum": 2160 }
                },
                "required": ["width", "height"],
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |args, browser_context| {
                let width = required_viewport_dimension(&args, "width", 3840)?;
                let height = required_viewport_dimension(&args, "height", 2160)?;
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_set_viewport_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        width,
                        height,
                    )
                    .await
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
            async_browser_text_tool_handler(move |_args, browser_context| {
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_get_images_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                    )
                    .await
                })
            }),
        );
    }

    pub(super) fn register_browser_upload(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_upload",
            "Upload one or more existing workspace-relative regular files into a file input ref. Paths cannot escape the workspace; symlinks and oversized files are rejected.",
            json!({
                "type": "object",
                "properties": {
                    "ref": { "type": "string" },
                    "paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 1,
                        "maxItems": MAX_BROWSER_UPLOAD_FILES
                    }
                },
                "required": ["ref", "paths"],
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |args, browser_context| {
                let reference = required_trimmed_string(&args, "ref")?;
                let paths = args
                    .get("paths")
                    .and_then(|value| value.as_array())
                    .ok_or_else(|| "paths is required".to_string())?
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .map(ToOwned::to_owned)
                            .ok_or_else(|| "paths must contain only strings".to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_upload_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        reference,
                        paths,
                    )
                    .await
                })
            }),
        );
    }

    pub(super) fn register_browser_download(&mut self, bound: BoundContext) {
        self.register_tool(
            "browser_download",
            "Click a download element ref and save the resulting file to a new workspace-relative path. The parent directory must exist, existing targets are never overwritten, and downloads over 100 MiB are removed.",
            json!({
                "type": "object",
                "properties": {
                    "ref": { "type": "string" },
                    "path": { "type": "string" }
                },
                "required": ["ref", "path"],
                "additionalProperties": false
            }),
            async_browser_text_tool_handler(move |args, browser_context| {
                let reference = required_trimmed_string(&args, "ref")?;
                let path = required_trimmed_string(&args, "path")?;
                let ctx = bound.for_tool_call(&browser_context);
                Ok(async move {
                    browser_download_with_context(
                        ctx,
                        browser_context.conversation_id.as_deref(),
                        reference,
                        path,
                    )
                    .await
                })
            }),
        );
    }
}

fn required_viewport_dimension(
    args: &serde_json::Value,
    field: &str,
    maximum: u32,
) -> Result<u32, String> {
    let value = args
        .get(field)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| format!("{field} must be an integer"))?;
    if !(240..=maximum).contains(&value) {
        return Err(format!("{field} must be between 240 and {maximum}"));
    }
    Ok(value)
}
