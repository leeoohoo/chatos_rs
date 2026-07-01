// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::{
    DEFAULT_TOOL_RESULTS_MODEL_TOTAL_MAX_CHARS, DEFAULT_TOOL_RESULT_MODEL_MAX_CHARS,
};
use chrono::Utc;

mod external_mcp_config;
mod mcp;
mod memory;
mod model_config;
mod project;
mod remote_server;
mod run;
mod skill;
mod system;
mod task;
mod user;

pub use self::external_mcp_config::*;
pub use self::mcp::*;
pub use self::memory::*;
pub use self::model_config::*;
pub use self::project::*;
pub use self::remote_server::*;
pub use self::run::*;
pub use self::skill::*;
pub use self::system::*;
pub use self::task::*;
pub use self::user::*;

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn default_tool_result_model_max_chars() -> usize {
    DEFAULT_TOOL_RESULT_MODEL_MAX_CHARS
}

fn default_tool_results_model_total_max_chars() -> usize {
    DEFAULT_TOOL_RESULTS_MODEL_TOTAL_MAX_CHARS
}

fn default_true() -> bool {
    true
}
