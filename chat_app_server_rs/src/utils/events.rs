// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// Canonical event type constants for SSE/chat
pub struct Events;

impl Events {
    pub const START: &'static str = "start";
    pub const CHUNK: &'static str = "chunk";
    pub const THINKING: &'static str = "thinking";
    pub const TOOLS_START: &'static str = "tools_start";
    pub const TOOLS_STREAM: &'static str = "tools_stream";
    pub const TOOLS_END: &'static str = "tools_end";
    pub const TOOLS_UNAVAILABLE: &'static str = "tools_unavailable";
    pub const CONTEXT_SUMMARIZED_START: &'static str = "context_summarized_start";
    pub const CONTEXT_SUMMARIZED_STREAM: &'static str = "context_summarized_stream";
    pub const CONTEXT_SUMMARIZED_END: &'static str = "context_summarized_end";
    pub const TURN_PHASE: &'static str = "turn_phase";
    pub const COMPLETE: &'static str = "complete";
    pub const CANCELLED: &'static str = "cancelled";
    pub const ERROR: &'static str = "error";
    pub const TASK_CREATE_REVIEW_REQUIRED: &'static str = "task_create_review_required";
    pub const TASK_BOARD_UPDATED: &'static str = "task_board_updated";
    pub const ASK_USER_PROMPT_REQUIRED: &'static str = "ask_user_prompt_required";
    pub const ASK_USER_PROMPT_RESOLVED: &'static str = "ask_user_prompt_resolved";
    pub const RUNTIME_GUIDANCE_APPLIED: &'static str = "runtime_guidance_applied";
}
