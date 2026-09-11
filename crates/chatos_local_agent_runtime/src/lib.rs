// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Durable Local Agent execution primitives. The reducer is deliberately pure:
//! it advances one claimed event and never calls a model, tool, queue, or
//! database by itself.

mod context;
mod digest;
mod durable;
mod memory_sync;
mod model_gateway;
mod model_gateway_client;
mod model_input_token_guard;
mod model_step;
mod pagination;
mod recovery;
mod reducer;
mod run_control;
mod scheduler;
mod tool_runtime;
mod ui_events;

pub use context::*;
pub use durable::*;
pub use memory_sync::*;
pub use model_gateway::*;
pub use model_gateway_client::*;
pub use model_input_token_guard::*;
pub use model_step::*;
pub use recovery::*;
pub use reducer::*;
pub use run_control::*;
pub use scheduler::*;
pub use tool_runtime::*;
