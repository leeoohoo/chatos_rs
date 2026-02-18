// Canonical event type constants for SSE/chat
pub struct Events;

impl Events {
    pub const START: &'static str = "start";
    pub const CHUNK: &'static str = "chunk";
    pub const THINKING: &'static str = "thinking";
    pub const TOOLS_START: &'static str = "tools_start";
    pub const TOOLS_STREAM: &'static str = "tools_stream";
    pub const TOOLS_END: &'static str = "tools_end";
    pub const CONTEXT_SUMMARIZED_START: &'static str = "context_summarized_start";
    pub const CONTEXT_SUMMARIZED_STREAM: &'static str = "context_summarized_stream";
    pub const CONTEXT_SUMMARIZED_END: &'static str = "context_summarized_end";
    pub const CONTEXT_SUMMARIZED: &'static str = "context_summarized";
    pub const COMPLETE: &'static str = "complete";
    pub const CANCELLED: &'static str = "cancelled";
    pub const ERROR: &'static str = "error";
    pub const TASK_CREATE_REVIEW_REQUIRED: &'static str = "task_create_review_required";
    pub const TASK_CREATE_REVIEW_RESOLVED: &'static str = "task_create_review_resolved";
    pub const HEARTBEAT: &'static str = "heartbeat";
}
