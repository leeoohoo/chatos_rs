// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::memory_compat::{
    MemoryCompatComposeContextMeta, MemoryCompatComposeContextResponse,
};
use crate::models::session::Session;
use crate::services::chatos_memory_engine::{self, ComposedChatHistoryContext};

pub fn compat_include_raw_messages(include_raw_messages: Option<bool>) -> bool {
    include_raw_messages.unwrap_or(true)
}

pub async fn compose_context(
    session: &Session,
    include_raw_messages: bool,
) -> Result<ComposedChatHistoryContext, String> {
    chatos_memory_engine::compose_chatos_context(session, include_raw_messages).await
}

pub async fn compose_context_compat_response(
    session: &Session,
    include_raw_messages: bool,
) -> Result<MemoryCompatComposeContextResponse, String> {
    let payload = compose_context(session, include_raw_messages).await?;
    Ok(MemoryCompatComposeContextResponse {
        session_id: session.id.clone(),
        merged_summary: payload.merged_summary,
        summary_count: payload.summary_count,
        messages: payload.messages,
        meta: MemoryCompatComposeContextMeta {
            used_levels: Vec::new(),
            filtered_rollup_count: 0,
            kept_raw_level0_count: 0,
        },
    })
}
