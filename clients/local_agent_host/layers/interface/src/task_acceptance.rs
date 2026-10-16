// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

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
    let status = report
        .get("status")
        .and_then(Value::as_str)
        .filter(|s| matches!(*s, "succeeded" | "failed" | "blocked"))
        .ok_or("Task acceptance report has an invalid status")?;
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
        let verdict = check
            .get("status")
            .and_then(Value::as_str)
            .filter(|s| matches!(*s, "passed" | "failed" | "not_verified"))
            .ok_or("acceptance check status must be passed, failed, or not_verified")?;
        check.get("evidence").and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty() && s.chars().count() <= 2_000)
            .ok_or("each acceptance check needs 1..=2000 characters of concrete evidence or an explanation of the gap")?;
        if status == "succeeded" && verdict != "passed" {
            return Err("succeeded requires every Task criterion to be passed; repair the gap or report failed/blocked".into());
        }
    }
    Ok(())
}

pub fn completed_task_acceptance_report(continuation: Option<&Value>) -> Option<&Value> {
    continuation?
        .get("invocations")?
        .as_array()?
        .iter()
        .find_map(|invocation| {
            (invocation.get("tool_name")?.as_str()? == "task_run_process_report_outcome"
                && invocation.get("status")?.as_str()? == "succeeded"
                && invocation.get("result")?.get("reported")?.as_bool()?)
            .then(|| invocation.get("result"))
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
        assert!(validate_task_acceptance_report(&input, &report).is_err());
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
}
