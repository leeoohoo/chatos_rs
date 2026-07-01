// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::{TaskBuiltinMcpPromptMode, TaskMcpInitMode};
use chatos_mcp_runtime::{configurable_builtin_kinds, BuiltinMcpPromptLocale};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::CreateRemoteServerRequest;

mod config;
mod record;
mod requests;

pub use self::config::*;
pub use self::record::*;
pub use self::requests::*;

pub const TASK_PROFILE_DEFAULT: &str = "default";
pub const TASK_PROFILE_CHATOS_PLAN: &str = "chatos_plan";

pub fn default_task_profile() -> String {
    TASK_PROFILE_DEFAULT.to_string()
}

pub fn normalize_task_profile(value: Option<&str>) -> Result<String, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(default_task_profile());
    };
    if value.eq_ignore_ascii_case(TASK_PROFILE_DEFAULT) {
        return Ok(TASK_PROFILE_DEFAULT.to_string());
    }
    if value.eq_ignore_ascii_case(TASK_PROFILE_CHATOS_PLAN) {
        return Ok(TASK_PROFILE_CHATOS_PLAN.to_string());
    }
    Err(format!("unknown task_profile: {value}"))
}
