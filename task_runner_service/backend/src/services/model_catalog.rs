// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::ModelConfigRecord;
use chatos_ai_runtime::model_config::{
    default_base_url_for_provider, normalize_provider, normalize_thinking_level,
};
use chatos_plugin_management_sdk::{normalize_agent_prompt_vendor, AgentPromptVendor};
use std::str::FromStr;

use super::normalized_optional;

mod normalization;

pub(in crate::services) use self::normalization::normalize_model_config_record;
#[cfg(test)]
pub(in crate::services) use self::normalization::{
    normalize_model_prompt_vendor_input, normalize_model_provider_input,
    normalize_model_thinking_level_input,
};
