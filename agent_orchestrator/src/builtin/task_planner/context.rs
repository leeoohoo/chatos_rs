use crate::core::mcp_tools::ToolStreamChunkCallback;

pub(crate) struct ToolContext<'a> {
    pub(crate) session_id: &'a str,
    pub(crate) conversation_turn_id: &'a str,
    pub(crate) on_stream_chunk: Option<ToolStreamChunkCallback>,
}
