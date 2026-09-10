// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::{TaskBuiltinMcpPromptMode, TaskMcpInitMode};
use chatos_mcp_runtime::BuiltinMcpPromptLocale;
use chatos_plugin_management_sdk::TaskPluginConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod config;
mod record;
mod requests;

pub use self::config::*;
pub use self::record::*;
pub use self::requests::*;

pub const TASK_PROFILE_DEFAULT: &str = "default";

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
    Err(format!("unknown task_profile: {value}"))
}

#[cfg(test)]
mod task_profile_tests {
    use super::*;

    #[test]
    fn obsolete_planning_profiles_are_rejected() {
        for profile in ["chatos_plan", " CHATOS_PLAN ", "unknown"] {
            assert!(normalize_task_profile(Some(profile)).is_err());
        }
        assert_eq!(normalize_task_profile(None).unwrap(), TASK_PROFILE_DEFAULT);
        assert_eq!(
            normalize_task_profile(Some(" default ")).unwrap(),
            TASK_PROFILE_DEFAULT
        );
    }
}
