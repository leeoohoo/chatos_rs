// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;
use serde_json::{json, Value};

pub(crate) const APPROVAL_DECISION_TOOL: &str = "approval_decision";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApprovalToolDecision {
    pub(crate) decision: String,
    pub(crate) reason: String,
}

#[derive(Debug, Deserialize)]
struct ApprovalDecisionToolArgs {
    decision: String,
    reason: String,
    #[serde(default)]
    remember_allow: bool,
}

pub(crate) fn approval_decision_tool_result(
    args: Value,
) -> Result<(ApprovalToolDecision, Value), String> {
    let parsed = serde_json::from_value::<ApprovalDecisionToolArgs>(args)
        .map_err(|error| format!("approval_decision 参数无效: {error}"))?;
    let decision = parsed.decision.trim().to_ascii_lowercase();
    if !matches!(decision.as_str(), "approve" | "deny" | "ask_user") {
        return Err("approval_decision.decision must be approve, deny, or ask_user".to_string());
    }
    let reason = parsed.reason.trim().to_string();
    if reason.is_empty() {
        return Err("approval_decision.reason is required".to_string());
    }
    let remember_allow = decision == "approve" && parsed.remember_allow;
    let result = json!({
        "content": [{
            "type": "text",
            "text": json!({
                "decision": decision,
                "reason": reason,
                "remember_allow": remember_allow,
            }).to_string(),
        }],
        "_structured_result": {
            "decision": decision,
            "reason": reason,
            "remember_allow": remember_allow,
        },
    });
    Ok((ApprovalToolDecision { decision, reason }, result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_normalizes_decision_payload() {
        let (decision, result) = approval_decision_tool_result(json!({
            "decision": " APPROVE ",
            "reason": " matches project scripts ",
            "remember_allow": true,
        }))
        .expect("valid decision");

        assert_eq!(decision.decision, "approve");
        assert_eq!(decision.reason, "matches project scripts");
        assert_eq!(result["_structured_result"]["remember_allow"], true);
        assert_eq!(result["_structured_result"]["decision"], "approve");
    }

    #[test]
    fn rejects_missing_reason_and_unknown_decision() {
        assert!(approval_decision_tool_result(json!({
            "decision": "approve",
            "reason": " "
        }))
        .is_err());
        assert!(approval_decision_tool_result(json!({
            "decision": "later",
            "reason": "unsupported"
        }))
        .is_err());
    }
}
