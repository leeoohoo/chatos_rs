// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::LocalAgentRunRecord;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Explicit, frozen constraints only; never infer a limit from natural-language criteria.
pub fn validate_task_final_response_limit(input: &Value) -> Result<(), String> {
    if let Some(limit) = input
        .get("final_response_max_chars")
        .filter(|v| !v.is_null())
    {
        if !limit.as_u64().is_some_and(|n| (1..=32_000).contains(&n)) {
            return Err("final_response_max_chars must be an integer in 1..=32000".into());
        }
    }
    Ok(())
}

/// This verifies coverage and deterministic constraints, not the truth of model evidence.
pub fn validate_task_acceptance_report(input: &Value, report: &Value) -> Result<(), String> {
    if input
        .get("acceptance_report_version")
        .and_then(Value::as_u64)
        != Some(1)
    {
        return Ok(()); // Already-created Tasks and their retries retain their old contract.
    }
    validate_task_final_response_limit(input)?;
    let result_status = report
        .get("result_status")
        .or_else(|| report.get("status"))
        .and_then(Value::as_str)
        .filter(|s| matches!(*s, "succeeded" | "failed" | "blocked"))
        .ok_or("Task acceptance report has an invalid result_status")?;
    validate_verification_report(input, report, result_status)?;
    let final_response = report
        .get("final_response")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or("final_response is required for this Task")?;
    let length = final_response.chars().count();
    let maximum = input
        .get("final_response_max_chars")
        .and_then(Value::as_u64)
        .unwrap_or(32_000);
    if length as u64 > maximum {
        return Err(format!("final_response has {length} characters; maximum is {maximum}. Count includes whitespace, punctuation and Markdown. Revise before reporting."));
    }
    let criteria = input
        .get("acceptance_criteria")
        .and_then(Value::as_array)
        .filter(|c| !c.is_empty())
        .ok_or("Task has no bound acceptance criteria")?;
    let checks = report
        .get("acceptance_checks")
        .and_then(Value::as_array)
        .ok_or("acceptance_checks is required for this Task")?;
    if checks.len() != criteria.len() {
        return Err(format!(
            "acceptance_checks must cover all {} criteria exactly once",
            criteria.len()
        ));
    }
    let mut seen = vec![false; criteria.len()];
    for check in checks {
        let index = check
            .get("criterion_index")
            .and_then(Value::as_u64)
            .filter(|i| *i < criteria.len() as u64)
            .ok_or("criterion_index is out of range (zero-based)")? as usize;
        if std::mem::replace(&mut seen[index], true) {
            return Err("duplicate criterion_index in acceptance_checks".into());
        }
        let _verdict = check
            .get("status")
            .and_then(Value::as_str)
            .filter(|s| matches!(*s, "passed" | "failed" | "not_verified" | "blocked"))
            .ok_or("acceptance check status must be passed, failed, not_verified, or blocked")?;
        check.get("evidence").and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty() && s.chars().count() <= 2_000)
            .ok_or("each acceptance check needs 1..=2000 characters of concrete evidence or an explanation of the gap")?;
        // `result_status` is the internal dependency gate, not execution state.
        // Per-criterion verdicts remain independent findings: a verification
        // Task may finish normally and prove that the inspected artifact does
        // not meet acceptance without turning the Run into an execution error.
    }
    Ok(())
}

fn validate_verification_report(
    input: &Value,
    report: &Value,
    result_status: &str,
) -> Result<(), String> {
    let Some(contract) = input
        .get("verification_contract")
        .filter(|value| !value.is_null())
    else {
        if report
            .get("verification_report")
            .is_some_and(|value| !value.is_null())
        {
            return Err(
                "verification_report is allowed only for a Task with verification_contract"
                    .to_string(),
            );
        }
        return Ok(());
    };
    let verification = report
        .get("verification_report")
        .filter(|value| !value.is_null())
        .ok_or("verification_report is required for this Task")?;
    let subject = verification
        .get("subject_identity")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("verification_report.subject_identity is required")?;
    if contract
        .get("subject_identity")
        .and_then(Value::as_str)
        .map(str::trim)
        != Some(subject)
    {
        return Err(
            "verification_report subject_identity does not match the frozen verification subject"
                .to_string(),
        );
    }
    let full_rerun = verification
        .get("full_rerun")
        .and_then(Value::as_bool)
        .ok_or("verification_report.full_rerun is required")?;
    if contract
        .get("require_full_rerun")
        .and_then(Value::as_bool)
        .ok_or("verification_contract.require_full_rerun is required")?
        && !full_rerun
    {
        return Err("verification_report.full_rerun must be true for this Task".to_string());
    }

    let contract_checks = contract
        .get("checks")
        .and_then(Value::as_array)
        .filter(|checks| !checks.is_empty())
        .ok_or("verification_contract.checks is required")?;
    let mut dependencies_by_check = HashMap::with_capacity(contract_checks.len());
    for check in contract_checks {
        let check_id = check
            .get("check_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or("verification_contract contains an invalid check_id")?;
        let dependencies = check
            .get("depends_on")
            .and_then(Value::as_array)
            .ok_or("verification_contract check is missing depends_on")?
            .iter()
            .map(|dependency| {
                dependency
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .ok_or("verification_contract contains an invalid dependency")
            })
            .collect::<Result<HashSet<_>, _>>()?;
        if dependencies_by_check
            .insert(check_id.to_string(), dependencies)
            .is_some()
        {
            return Err(format!(
                "verification_contract check_id is duplicated: {check_id}"
            ));
        }
    }

    let checks = verification
        .get("checks")
        .and_then(Value::as_array)
        .ok_or("verification_report.checks is required")?;
    if checks.len() != dependencies_by_check.len() {
        return Err(format!(
            "verification_report must report all {} checks exactly once",
            dependencies_by_check.len()
        ));
    }
    let mut reported = HashMap::with_capacity(checks.len());
    for check in checks {
        let check_id = check
            .get("check_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| dependencies_by_check.contains_key(*value))
            .ok_or("verification_report contains an unknown check_id")?;
        let status = check
            .get("status")
            .and_then(Value::as_str)
            .filter(|value| matches!(*value, "passed" | "failed" | "skipped_due_to"))
            .ok_or("verification check status must be passed, failed, or skipped_due_to")?;
        if reported
            .insert(check_id.to_string(), (status, check))
            .is_some()
        {
            return Err(format!(
                "verification_report check_id is duplicated: {check_id}"
            ));
        }
        if status != "skipped_due_to" {
            check
                .get("evidence")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty() && value.chars().count() <= 2_000)
                .ok_or("a passed or failed verification check needs concrete evidence")?;
        }
    }
    for (check_id, (status, check)) in &reported {
        if *status == "skipped_due_to" {
            let skipped_due_to = check
                .get("skipped_due_to")
                .and_then(Value::as_array)
                .filter(|dependencies| !dependencies.is_empty())
                .ok_or("a skipped verification check must name at least one dependency")?;
            let declared = &dependencies_by_check[check_id];
            let mut seen = HashSet::new();
            for dependency in skipped_due_to {
                let dependency = dependency
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty() && declared.contains(*value))
                    .ok_or_else(|| {
                        format!(
                            "verification check {check_id} may skip only for a declared direct dependency"
                        )
                    })?;
                if !seen.insert(dependency) {
                    return Err(format!(
                        "verification check {check_id} repeats skipped dependency {dependency}"
                    ));
                }
                if !reported
                    .get(dependency)
                    .is_some_and(|(dependency_status, _)| {
                        matches!(*dependency_status, "failed" | "skipped_due_to")
                    })
                {
                    return Err(format!(
                        "verification check {check_id} cannot skip because dependency {dependency} did not fail or skip"
                    ));
                }
            }
        }
        if result_status == "succeeded" && *status != "passed" {
            return Err("a succeeded Task requires every verification check to pass".to_string());
        }
    }
    Ok(())
}

/// Returns the frozen acceptance criteria that still prevent successful completion.
/// Structural validation remains separate so callers never treat a malformed report as
/// an ordinary incomplete result.
pub fn incomplete_task_acceptance_checks(
    input: &Value,
    report: &Value,
) -> Result<Vec<Value>, String> {
    validate_task_acceptance_report(input, report)?;
    if input
        .get("acceptance_report_version")
        .and_then(Value::as_u64)
        != Some(1)
    {
        return Ok(Vec::new());
    }
    let criteria = input
        .get("acceptance_criteria")
        .and_then(Value::as_array)
        .ok_or("Task has no bound acceptance criteria")?;
    let checks = report
        .get("acceptance_checks")
        .and_then(Value::as_array)
        .ok_or("acceptance_checks is required for this Task")?;
    let mut incomplete = Vec::new();
    for check in checks {
        if check.get("status").and_then(Value::as_str) == Some("passed") {
            continue;
        }
        let index = check
            .get("criterion_index")
            .and_then(Value::as_u64)
            .ok_or("criterion_index is out of range (zero-based)")? as usize;
        incomplete.push(serde_json::json!({
            "criterion_index": index,
            "criterion": criteria.get(index).and_then(Value::as_str).unwrap_or_default(),
            "status": check.get("status"),
            "evidence": check.get("evidence"),
        }));
    }
    let result_status = report
        .get("result_status")
        .or_else(|| report.get("status"))
        .and_then(Value::as_str);
    if incomplete.is_empty() && result_status != Some("succeeded") {
        incomplete.push(serde_json::json!({
            "criterion_index": Value::Null,
            "criterion": "Task result must be reported as succeeded",
            "status": result_status,
            "evidence": report.get("reason"),
        }));
    }
    Ok(incomplete)
}

pub fn task_acceptance_report_is_complete(input: &Value, report: &Value) -> Result<bool, String> {
    Ok(incomplete_task_acceptance_checks(input, report)?.is_empty())
}

/// A Task may stop for external intervention only after every criterion that the
/// model can still act on has passed. One blocked criterion does not waive the
/// remaining acceptance contract.
pub fn task_acceptance_report_is_blocked(input: &Value, report: &Value) -> Result<bool, String> {
    validate_task_acceptance_report(input, report)?;
    if input
        .get("acceptance_report_version")
        .and_then(Value::as_u64)
        != Some(1)
    {
        return Ok(false);
    }
    if report
        .get("result_status")
        .or_else(|| report.get("status"))
        .and_then(Value::as_str)
        != Some("blocked")
    {
        return Ok(false);
    }
    let checks = report
        .get("acceptance_checks")
        .and_then(Value::as_array)
        .ok_or("acceptance_checks is required for this Task")?;
    Ok(checks
        .iter()
        .any(|check| check.get("status").and_then(Value::as_str) == Some("blocked"))
        && checks.iter().all(|check| {
            matches!(
                check.get("status").and_then(Value::as_str),
                Some("passed" | "blocked")
            )
        }))
}

pub fn task_acceptance_report_is_terminal(input: &Value, report: &Value) -> Result<bool, String> {
    Ok(task_acceptance_report_is_complete(input, report)?
        || task_acceptance_report_is_blocked(input, report)?)
}

pub fn completed_task_acceptance_report(continuation: Option<&Value>) -> Option<&Value> {
    continuation?
        .get("invocations")?
        .as_array()?
        .iter()
        .rev()
        .find_map(|invocation| {
            (invocation.get("tool_name")?.as_str()? == "task_run_process_report_outcome"
                && invocation.get("status")?.as_str()? == "succeeded"
                && invocation.get("result")?.get("reported")?.as_bool()?)
            .then(|| invocation.get("result"))
            .flatten()
        })
}

/// Returns the currently accepted Task report, including the report retained
/// across a trusted SubagentStop continuation. The retained copy is considered
/// only at that exact Host-authored checkpoint boundary and is validated again
/// by every terminal consumer.
pub fn task_acceptance_report_for_run(run: &LocalAgentRunRecord) -> Option<&Value> {
    completed_task_acceptance_report(run.continuation_input.as_ref()).or_else(|| {
        (run.checkpoint.pointer("/continuation/reason")?.as_str()?
            == "trusted_plugin_hook_stop_block")
            .then(|| run.checkpoint.pointer("/response/task_acceptance"))
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn coverage_verdicts_and_unicode_length_are_enforced_without_parsing_prose() {
        let input = json!({"acceptance_report_version":1,"acceptance_criteria":["最多400字"],"final_response_max_chars":4});
        let mut report = json!({"status":"succeeded","final_response":"中文\n!","acceptance_checks":[{"criterion_index":0,"status":"passed","evidence":"Checked the final text"}]});
        assert!(validate_task_acceptance_report(&input, &report).is_ok());
        let mut emoji_input = input.clone();
        emoji_input["final_response_max_chars"] = json!(1);
        let mut emoji_report = report.clone();
        emoji_report["final_response"] = json!("🙂");
        assert!(validate_task_acceptance_report(&emoji_input, &emoji_report).is_ok());
        emoji_report["final_response"] = json!("e\u{301}");
        assert!(validate_task_acceptance_report(&emoji_input, &emoji_report).is_err());
        report["final_response"] = json!("中文\n!!");
        assert!(validate_task_acceptance_report(&input, &report)
            .unwrap_err()
            .contains("5 characters"));
        report["final_response"] = json!("短文");
        report["acceptance_checks"][0]["status"] = json!("not_verified");
        assert!(validate_task_acceptance_report(&input, &report).is_ok());
        report["status"] = json!("blocked");
        assert!(validate_task_acceptance_report(&input, &report).is_ok());
        report["acceptance_checks"] = json!([]);
        assert!(validate_task_acceptance_report(&input, &report).is_err());
        assert!(
            validate_task_acceptance_report(&json!({}), &json!({"status":"succeeded"})).is_ok()
        );
    }

    #[test]
    fn duplicate_checks_and_empty_evidence_do_not_count_as_acceptance() {
        let input = json!({"acceptance_report_version":1,"acceptance_criteria":["A","B"]});
        let mut report = json!({"status":"succeeded","final_response":"Review found problems in the reviewed artifact.","acceptance_checks":[{"criterion_index":0,"status":"passed","evidence":"Compared A"},{"criterion_index":0,"status":"passed","evidence":"Compared B"}]});
        assert!(validate_task_acceptance_report(&input, &report).is_err());
        report["acceptance_checks"][1]["criterion_index"] = json!(1);
        assert!(validate_task_acceptance_report(&input, &report).is_ok()); // Review Task can succeed.
        report["acceptance_checks"][1]["evidence"] = json!(" ");
        assert!(validate_task_acceptance_report(&input, &report).is_err());
        for limit in [json!(0), json!(-1), json!(1.5), json!(32001), json!("400")] {
            assert!(
                validate_task_final_response_limit(&json!({"final_response_max_chars":limit}))
                    .is_err()
            );
        }
    }

    #[test]
    fn successful_completion_requires_every_frozen_check_to_pass() {
        let input = json!({
            "acceptance_report_version": 1,
            "acceptance_criteria": ["Saved", "Verified"]
        });
        let mut report = json!({
            "result_status": "succeeded",
            "reason": "Work inspected",
            "final_response": "Done",
            "acceptance_checks": [
                {"criterion_index":0,"status":"passed","evidence":"Saved row exists"},
                {"criterion_index":1,"status":"not_verified","evidence":"UI was not opened"}
            ]
        });
        let incomplete = incomplete_task_acceptance_checks(&input, &report).unwrap();
        assert_eq!(incomplete.len(), 1);
        assert_eq!(incomplete[0]["criterion"], "Verified");
        assert!(!task_acceptance_report_is_complete(&input, &report).unwrap());
        report["acceptance_checks"][1]["status"] = json!("passed");
        assert!(task_acceptance_report_is_complete(&input, &report).unwrap());
        report["result_status"] = json!("blocked");
        assert!(!task_acceptance_report_is_complete(&input, &report).unwrap());
        assert!(!task_acceptance_report_is_blocked(&input, &report).unwrap());
        report["acceptance_checks"][1]["status"] = json!("blocked");
        assert!(task_acceptance_report_is_blocked(&input, &report).unwrap());
        assert!(task_acceptance_report_is_terminal(&input, &report).unwrap());
        for actionable_status in ["failed", "not_verified"] {
            report["acceptance_checks"][0]["status"] = json!(actionable_status);
            assert!(
                !task_acceptance_report_is_blocked(&input, &report).unwrap(),
                "a blocked criterion must not waive a {actionable_status} criterion"
            );
            assert!(!task_acceptance_report_is_terminal(&input, &report).unwrap());
        }
    }

    #[test]
    fn verification_report_matches_the_frozen_subject_and_rerun_policy() {
        let input = json!({
            "acceptance_report_version": 1,
            "acceptance_criteria": ["[R1] Complete checks"],
            "verification_contract": {
                "subject_identity":"artifact:abc123",
                "workflow_entry":"./verify",
                "checks":[{"check_id":"custom-check","depends_on":[]}],
                "require_full_rerun":true
            }
        });
        let mut report = json!({
            "result_status":"succeeded",
            "reason":"verification passed",
            "final_response":"Ready",
            "acceptance_checks":[{"criterion_index":0,"status":"passed","evidence":"All checks passed"}],
            "verification_report":{
                "subject_identity":"artifact:abc123",
                "full_rerun":true,
                "checks":[{"check_id":"custom-check","status":"passed","evidence":"checked"}]
            }
        });
        assert!(validate_task_acceptance_report(&input, &report).is_ok());
        report["verification_report"]["subject_identity"] = json!("different");
        assert!(validate_task_acceptance_report(&input, &report)
            .unwrap_err()
            .contains("frozen verification subject"));
        report["verification_report"]["subject_identity"] = json!("artifact:abc123");
        report["verification_report"]["full_rerun"] = json!(false);
        assert!(validate_task_acceptance_report(&input, &report)
            .unwrap_err()
            .contains("full_rerun"));

        let mut without_contract = input;
        without_contract
            .as_object_mut()
            .unwrap()
            .remove("verification_contract");
        assert!(validate_task_acceptance_report(&without_contract, &report)
            .unwrap_err()
            .contains("only for a Task with verification_contract"));
    }

    #[test]
    fn verification_report_allows_only_real_dependency_skips_and_complete_coverage() {
        let input = json!({
            "acceptance_report_version": 1,
            "acceptance_criteria": ["[R1] Complete checks"],
            "verification_contract": {
                "subject_identity":"document:v7",
                "workflow_entry":"project-skill:review",
                "checks":[
                    {"check_id":"lint","depends_on":[]},
                    {"check_id":"integration","depends_on":["lint"]},
                    {"check_id":"docs","depends_on":[]}
                ],
                "require_full_rerun":false
            }
        });
        let mut report = json!({
            "result_status":"failed",
            "reason":"lint failed",
            "final_response":"Not ready",
            "acceptance_checks":[{"criterion_index":0,"status":"failed","evidence":"Lint failed; integration depends on it"}],
            "verification_report":{
                "subject_identity":"document:v7",
                "full_rerun":false,
                "checks":[
                    {"check_id":"lint","status":"failed","evidence":"lint error"},
                    {"check_id":"integration","status":"skipped_due_to","skipped_due_to":["lint"]},
                    {"check_id":"docs","status":"passed","evidence":"links checked"}
                ]
            }
        });
        assert!(validate_task_acceptance_report(&input, &report).is_ok());
        report["result_status"] = json!("succeeded");
        assert!(validate_task_acceptance_report(&input, &report)
            .unwrap_err()
            .contains("every verification check"));
        report["result_status"] = json!("failed");
        report["verification_report"]["checks"][2] = json!({
            "check_id":"docs","status":"skipped_due_to","skipped_due_to":["lint"]
        });
        assert!(validate_task_acceptance_report(&input, &report)
            .unwrap_err()
            .contains("declared direct dependency"));

        report["verification_report"]["checks"][2] =
            json!({"check_id":"docs","status":"passed","evidence":"links checked"});
        report["verification_report"]["checks"]
            .as_array_mut()
            .unwrap()
            .pop();
        assert!(validate_task_acceptance_report(&input, &report)
            .unwrap_err()
            .contains("all 3 checks"));
    }
}
