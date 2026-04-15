mod callbacks;
mod events;
#[cfg(test)]
mod tests;
mod text;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::services::v2::ai_client::AiClientCallbacks as V2AiClientCallbacks;
use crate::services::v3::ai_client::AiClientCallbacks as V3AiClientCallbacks;

pub use self::callbacks::{build_v2_callbacks, build_v3_callbacks};
pub use self::events::{
    handle_chat_result, send_error_event, send_start_event, send_tools_unavailable_event,
};

pub struct StreamCallbacksV2 {
    pub callbacks: V2AiClientCallbacks,
    pub chunk_sent: Arc<AtomicBool>,
    pub streamed_content: Arc<Mutex<String>>,
}

pub struct StreamCallbacksV3 {
    pub callbacks: V3AiClientCallbacks,
    pub chunk_sent: Arc<AtomicBool>,
    pub streamed_content: Arc<Mutex<String>>,
}
