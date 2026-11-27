// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{LocalAgentRuntime, LocalAgentRuntimeError};
use chatos_local_agent_protocol::{
    LocalAgentRunRecord, LocalDiscoveredToolSkillGate, LocalSkillBundleSnapshot,
    LocalToolSkillActivationPolicy, LOCAL_AGENT_DECIDE_TASK_ROUTE_TOOL_NAME,
    LOCAL_AGENT_SKILL_ACTIVATE_TOOL_NAME, LOCAL_SKILL_BUNDLE_PROFILE_KEY,
    LOCAL_TASK_SKILL_BUNDLE_PROFILE_KEY,
};
use std::collections::BTreeSet;

fn complete_disclosed_tool(tool: &serde_json::Value) -> bool {
    let valid_digest = tool
        .get("schema_sha256")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| {
            value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        });
    valid_digest
        && tool
            .get("invocation_mode")
            .and_then(serde_json::Value::as_str)
            == Some("capability_invoke")
        && (tool.get("input_schema").is_some()
            || (tool
                .get("schema_delivery")
                .and_then(serde_json::Value::as_str)
                == Some("paged")
                && tool
                    .get("schema_chunk")
                    .and_then(serde_json::Value::as_str)
                    .is_some()
                && tool
                    .get("next_schema_offset")
                    .is_some_and(serde_json::Value::is_null)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalToolSkillGateDenial {
    pub error: String,
    pub tool_name: String,
    pub binding_id: String,
    pub required_skill_refs: Vec<String>,
    pub bundle_revision: String,
    pub next_action: Option<serde_json::Value>,
}

include!("skill_runtime/activation.rs");
include!("skill_runtime/disclosures.rs");
include!("skill_runtime/authorization.rs");

#[cfg(test)]
mod tests;
