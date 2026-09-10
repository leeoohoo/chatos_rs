// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod factory;
mod mongodb;
mod types;

pub use factory::{get_db, init_global};
pub use types::Database;
