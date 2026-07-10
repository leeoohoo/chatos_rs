// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;
use serde_json::{json, Value};

const DEFAULT_MCP_LIST_LIMIT: usize = 50;
const MAX_MCP_LIST_LIMIT: usize = 100;

#[derive(Debug, Clone, Copy)]
pub(super) struct McpListPageRequest {
    pub(super) limit: usize,
    pub(super) offset: usize,
}

#[derive(Debug, Serialize)]
struct McpListPageMeta {
    limit: usize,
    offset: usize,
    returned: usize,
    has_more: bool,
    next_offset: Option<usize>,
}

impl McpListPageRequest {
    pub(super) fn fetch_limit(&self) -> usize {
        self.limit.saturating_add(1)
    }
}

pub(super) fn mcp_list_page(limit: Option<usize>, offset: Option<usize>) -> McpListPageRequest {
    McpListPageRequest {
        limit: limit
            .unwrap_or(DEFAULT_MCP_LIST_LIMIT)
            .clamp(1, MAX_MCP_LIST_LIMIT),
        offset: offset.unwrap_or_default(),
    }
}

pub(super) fn paginated_list_payload<T: Serialize>(
    items: Vec<T>,
    page: McpListPageRequest,
    has_more: bool,
) -> Value {
    let returned = items.len();
    json!({
        "items": items,
        "page": McpListPageMeta {
            limit: page.limit,
            offset: page.offset,
            returned,
            has_more,
            next_offset: has_more.then_some(page.offset.saturating_add(page.limit)),
        }
    })
}
