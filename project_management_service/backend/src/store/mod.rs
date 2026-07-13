// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod common;
mod mongo;

pub use mongo::MongoStore;
pub type AppStore = MongoStore;
