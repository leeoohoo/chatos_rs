// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod control;
mod events;
mod execution;
mod model;
mod request;
mod tools;

pub(crate) use control::{LocalRuntimeGuidance, LocalTurnControlRegistry};
pub(in crate::local_runtime) use events::LocalChatEventStream;
pub(crate) use execution::{
    execute_chat_turn, LocalChatExecutionError, LocalChatExecutionErrorKind,
};
pub(in crate::local_runtime) use model::build_local_memory_context_input_items;
pub(crate) use request::LocalChatSendRequest;
pub(crate) use tools::{prepare_local_chat_tools, LocalChatRecordWriter};

#[cfg(test)]
pub(in crate::local_runtime) mod tests;
