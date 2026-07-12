// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod callbacks;
mod events;
#[cfg(test)]
mod tests;
mod text;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::services::ai_client_common::AiClientCallbacks as AgentAiClientCallbacks;

pub use self::callbacks::build_chat_stream_callbacks;
pub use self::events::{
    enrich_chat_result_with_persisted_messages, handle_chat_result, send_error_event,
    send_start_event, send_tools_unavailable_event, ChatEventSink, ChatRealtimeStreamContext,
};

pub struct ChatStreamCallbacks {
    pub callbacks: AgentAiClientCallbacks,
    pub chunk_sent: Arc<AtomicBool>,
    pub streamed_content: Arc<Mutex<String>>,
}
