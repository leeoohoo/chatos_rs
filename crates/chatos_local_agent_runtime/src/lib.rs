// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Durable Local Agent execution primitives. The reducer is deliberately pure:
//! it advances one claimed event and never calls a model, tool, queue, or
//! database by itself.

mod reducer;

pub use reducer::*;
