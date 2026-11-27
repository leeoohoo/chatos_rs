use super::*;

#[async_trait]
impl LocalToolExecutor for LocalTaskProcessToolExecutor {
    async fn execute_tool(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
    ) -> Result<LocalAgentToolOutcome, String> {
        let run = self.validate_run(&invocation.run_id).await?;
        let output = match invocation.tool_name.as_str() {
            TASK_PROCESS_RECORD_TOOL => {
                let args: RecordProcessArgs = serde_json::from_value(invocation.arguments.clone())
                    .map_err(|error| format!("invalid Task process input: {error}"))?;
                let operation = match args.operation {
                    ProcessOperation::Append => "append",
                    ProcessOperation::Replace => "replace",
                    ProcessOperation::Clear => "clear",
                };
                let heading = normalized(args.heading, 240, "heading")?;
                let content = normalized(args.content, PROCESS_ENTRY_MAX_CHARS, "content")?;
                let acceptance_checks =
                    validate_process_acceptance_checks(&run.input, args.acceptance_checks)?;
                if operation != "clear" && content.is_none() && acceptance_checks.is_empty() {
                    return Err(
                        "Task process content or acceptance_checks is required unless operation is clear"
                            .to_string()
                    );
                }
                json!({
                    "recorded": true,
                    "operation": operation,
                    "heading": heading,
                    "content_chars": content.as_deref().map(str::chars).map(Iterator::count).unwrap_or(0),
                    "acceptance_checks": acceptance_checks
                })
            }
            TASK_PROCESS_ALLOCATE_SERVICE_TOOL => {
                let args: AllocateServiceArgs =
                    serde_json::from_value(invocation.arguments.clone()).map_err(|error| {
                        format!("invalid Task service allocation input: {error}")
                    })?;
                let service_name = args.service_name.trim();
                if service_name.is_empty()
                    || service_name.len() > 64
                    || !service_name.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                    })
                {
                    return Err(
                        "service_name must be 1..=64 ASCII letters, digits, '.', '_' or '-'"
                            .to_string(),
                    );
                }
                if !matches!(args.scheme.as_str(), "http" | "https") {
                    return Err("scheme must be http or https".to_string());
                }
                let mut services = self
                    .services
                    .lock()
                    .map_err(|_| "Task service allocation registry is unavailable".to_string())?;
                match args.operation {
                    ServiceOperation::Allocate => {
                        let allocation = services.allocate(
                            &invocation.run_id,
                            service_name,
                            args.preferred_port,
                            &args.scheme,
                        )?;
                        json!({
                            "allocated": true,
                            "run_id": invocation.run_id,
                            "service_name": service_name,
                            "host": "127.0.0.1",
                            "port": allocation.port,
                            "service_url": format!("{}://127.0.0.1:{}", allocation.scheme, allocation.port),
                            "preferred_port": args.preferred_port,
                            "preferred_port_honored": args.preferred_port == Some(allocation.port),
                            "environment": {
                                "CHATOS_SERVICE_HOST": "127.0.0.1",
                                "CHATOS_SERVICE_PORT": allocation.port.to_string(),
                                "CHATOS_SERVICE_URL": format!("{}://127.0.0.1:{}", allocation.scheme, allocation.port)
                            }
                        })
                    }
                    ServiceOperation::Release => {
                        let released = services.release(&invocation.run_id, service_name);
                        json!({
                            "released": released.is_some(),
                            "run_id": invocation.run_id,
                            "service_name": service_name,
                            "port": released.map(|allocation| allocation.port)
                        })
                    }
                }
            }
            TASK_OUTCOME_REPORT_TOOL => {
                let args: ReportOutcomeArgs =
                    match serde_json::from_value(invocation.arguments.clone()) {
                        Ok(args) => args,
                        Err(error) => {
                            return Ok(rejected_outcome_report(
                                format!("invalid Task outcome input: {error}"),
                                &run.input,
                            ))
                        }
                    };
                // A successful Plugin call is neither necessary (an authorized fallback
                // may satisfy research) nor sufficient (opening a session is not delivery).
                // Validate the frozen acceptance contract instead of inventing a global
                // Plugin artifact requirement. This checks report structure, not evidence truth.
                if let Err(error) =
                    validate_task_acceptance_report(&run.input, &invocation.arguments)
                {
                    return Ok(rejected_outcome_report(error, &run.input));
                }
                let reason = args.reason.trim();
                if reason.is_empty() || reason.chars().count() > OUTCOME_REASON_MAX_CHARS {
                    return Ok(rejected_outcome_report(
                        format!(
                            "Task outcome reason must be 1..={OUTCOME_REASON_MAX_CHARS} characters"
                        ),
                        &run.input,
                    ));
                }
                if let Err(error) =
                    validate_blocker(&run.input, args.result_status, args.blocker.as_ref())
                {
                    return Ok(rejected_outcome_report(error, &run.input));
                }
                if let Some(blocker) = args.blocker.as_ref() {
                    match blocker.classification {
                        BlockerClassification::HandledLocally => {
                            return Ok(continue_outcome_report(
                                blocker,
                                "The issue is already handled locally. Continue the Task and report a terminal outcome only after completing the remaining acceptance work.",
                            ));
                        }
                        BlockerClassification::RetryWithChangedMethod => {
                            if self.recovery_method_already_tried(&run, blocker).await? {
                                return Ok(rejected_outcome_report(
                                    "This failure_fingerprint and next_method were already tried without resolving the Task. Do not repeat them; choose a new safe method or report replan_required."
                                        .into(),
                                    &run.input,
                                ));
                            }
                            return Ok(continue_outcome_report(
                                blocker,
                                "Continue this same Task with next_method. Do not repeat previous_method, and record the changed approach plus new evidence before reporting again.",
                            ));
                        }
                        BlockerClassification::UnknownSideEffectsNeedsReview => {
                            return Ok(rejected_outcome_report(
                                "Only the Host may place a Run in needs_review after observing an external side effect with an unknown result. Inspect the durable tool result; do not claim this state or replay the operation."
                                    .into(),
                                &run.input,
                            ));
                        }
                        BlockerClassification::ReplanRequired
                        | BlockerClassification::UserActionRequired => {}
                    }
                }
                let result_status = match args.result_status {
                    ReportedResultStatus::Succeeded => "succeeded",
                    ReportedResultStatus::Failed => "failed",
                    ReportedResultStatus::Blocked => "blocked",
                };
                let checks = args.acceptance_checks.map(|checks| checks.into_iter().map(|check| json!({
                    "criterion_index": check.criterion_index, "status": check.status, "evidence": check.evidence
                })).collect::<Vec<_>>());
                json!({
                    "reported": true,
                    "result_status": result_status,
                    // Compatibility alias for already-installed clients. This is an
                    // internal result, never the Host-owned execution status.
                    "status": result_status,
                    "reason": reason,
                    "acceptance_checks": checks,
                    "final_response_chars": args.final_response.as_ref().map(|text| text.chars().count()),
                    "final_response": args.final_response,
                    "verification_report": args.verification_report,
                    "blocker": args.blocker,
                    "acceptance_report_version": run.input.get("acceptance_report_version")
                })
            }
            tool_name => return Err(format!("unsupported Task process tool: {tool_name}")),
        };
        Ok(LocalAgentToolOutcome::Succeeded { output })
    }
}

fn validate_blocker(
    input: &Value,
    result_status: ReportedResultStatus,
    blocker: Option<&BlockerReport>,
) -> Result<(), String> {
    if matches!(result_status, ReportedResultStatus::Blocked)
        && input.get("requirement_plan").is_some()
        && blocker.is_none()
    {
        return Err(
            "Plan-bound blocked outcomes require a structured blocker classification".into(),
        );
    }
    let Some(blocker) = blocker else {
        return Ok(());
    };
    let evidence = blocker.evidence.trim();
    if evidence.is_empty() || evidence.chars().count() > 2_000 {
        return Err("blocker evidence must contain 1..=2000 characters".into());
    }
    let bounded = |name: &str, value: Option<&String>| -> Result<(), String> {
        if value.is_some_and(|value| {
            let value = value.trim();
            value.is_empty() || value.chars().count() > 1_000
        }) {
            return Err(format!("blocker {name} must contain 1..=1000 characters"));
        }
        Ok(())
    };
    bounded("failure_fingerprint", blocker.failure_fingerprint.as_ref())?;
    bounded("previous_method", blocker.previous_method.as_ref())?;
    bounded("next_method", blocker.next_method.as_ref())?;
    bounded("user_question", blocker.user_question.as_ref())?;
    match blocker.classification {
        BlockerClassification::HandledLocally => {
            if matches!(result_status, ReportedResultStatus::Blocked) {
                return Err("handled_locally cannot be reported as blocked".into());
            }
        }
        BlockerClassification::RetryWithChangedMethod => {
            blocker
                .failure_fingerprint
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or("retry_with_changed_method requires failure_fingerprint")?;
            let previous = blocker
                .previous_method
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or("retry_with_changed_method requires previous_method")?;
            let next = blocker
                .next_method
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or("retry_with_changed_method requires next_method")?;
            if previous == next {
                return Err(
                    "retry_with_changed_method requires a materially different next_method".into(),
                );
            }
        }
        BlockerClassification::ReplanRequired => {
            if !matches!(result_status, ReportedResultStatus::Blocked) {
                return Err("replan_required must use result_status=blocked".into());
            }
            if blocker.affected_task_ids.is_empty() {
                return Err("replan_required requires affected_task_ids".into());
            }
            for task_id in &blocker.affected_task_ids {
                let task_id = task_id.trim();
                if task_id.is_empty() || task_id.chars().count() > 256 {
                    return Err("affected_task_ids must contain 1..=256 character IDs".into());
                }
            }
        }
        BlockerClassification::UserActionRequired => {
            let planning_uses_requirement_survey = input.get("task_kind").and_then(Value::as_str)
                == Some("planning")
                && input
                    .pointer("/tool_options/enabled_builtin_kinds")
                    .and_then(Value::as_array)
                    .is_some_and(|kinds| {
                        kinds
                            .iter()
                            .any(|kind| kind.as_str() == Some("RequirementSurveyWrite"))
                    });
            if planning_uses_requirement_survey {
                return Err(
                    "A planning Task with Requirement Survey cannot stop as user_action_required. Ask genuine product questions through requirement_survey_create; classify unavailable or inconsistent Task capabilities as replan_required."
                        .into(),
                );
            }
            if !matches!(result_status, ReportedResultStatus::Blocked) {
                return Err("user_action_required must use result_status=blocked".into());
            }
            if blocker
                .user_question
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
            {
                return Err("user_action_required requires one concrete user_question".into());
            }
        }
        BlockerClassification::UnknownSideEffectsNeedsReview => {}
    }
    Ok(())
}

fn continue_outcome_report(blocker: &BlockerReport, guidance: &str) -> LocalAgentToolOutcome {
    LocalAgentToolOutcome::Succeeded {
        output: json!({
            "phase": "task_blocker_recovery",
            "reported": false,
            "retryable": true,
            "classification": blocker.classification.as_str(),
            "failure_fingerprint": blocker.failure_fingerprint,
            "previous_method": blocker.previous_method,
            "next_method": blocker.next_method,
            "guidance": guidance,
        }),
    }
}

fn rejected_outcome_report(error: String, input: &Value) -> LocalAgentToolOutcome {
    // No result has been accepted and no external action has been dispatched.
    // Return a known tool failure so the model can repair its report. Err remains
    // reserved for execution/storage failures whose effect cannot be confirmed.
    LocalAgentToolOutcome::Failed {
        error,
        detail: json!({
            "phase": "task_outcome_validation",
            "reported": false,
            "retryable": true,
            "task_acceptance_contract": {
                "acceptance_report_version": input.get("acceptance_report_version"),
                "acceptance_criteria": input.get("acceptance_criteria"),
                "expected_outputs": input.get("expected_outputs"),
                "final_response_max_chars": input.get("final_response_max_chars"),
                "source_user_message": input.get("source_user_message"),
                "requirements": input.get("requirements"),
                "requirement_refs": input.get("requirement_refs"),
                "unassigned_requirement_refs": input.get("unassigned_requirement_refs")
            },
            "guidance": "Correct the report against the returned Host-owned task_acceptance_contract, covering every criterion exactly once, and call task_run_process_report_outcome again. Preserve actual failed or unverified findings; do not rerun remote commands merely to fix report structure."
        }),
    }
}

fn normalized(
    value: Option<String>,
    maximum_chars: usize,
    field: &str,
) -> Result<Option<String>, String> {
    let value = value.map(|value| value.trim().to_string());
    let value = value.filter(|value| !value.is_empty());
    if value
        .as_deref()
        .is_some_and(|value| value.chars().count() > maximum_chars)
    {
        return Err(format!(
            "Task process {field} cannot exceed {maximum_chars} characters"
        ));
    }
    Ok(value)
}

fn validate_process_acceptance_checks(
    input: &Value,
    checks: Vec<ProcessAcceptanceCheck>,
) -> Result<Vec<Value>, String> {
    if checks.is_empty() {
        return Ok(Vec::new());
    }
    let criteria_count = input
        .get("acceptance_criteria")
        .and_then(Value::as_array)
        .map(Vec::len)
        .ok_or_else(|| "Task has no bound acceptance criteria".to_string())?;
    let mut seen = BTreeSet::new();
    let mut normalized_checks = Vec::with_capacity(checks.len());
    for check in checks {
        let index = usize::try_from(check.criterion_index)
            .map_err(|_| "criterion_index is out of range (zero-based)".to_string())?;
        if index >= criteria_count {
            return Err("criterion_index is out of range (zero-based)".to_string());
        }
        if !seen.insert(index) {
            return Err("acceptance_checks contains a duplicate criterion_index".to_string());
        }
        if !matches!(
            check.status.as_str(),
            "pending" | "in_progress" | "passed" | "failed" | "not_verified" | "blocked"
        ) {
            return Err(
                "process acceptance status must be pending, in_progress, passed, failed, not_verified, or blocked"
                    .to_string(),
            );
        }
        let evidence = normalized(check.evidence, 2_000, "acceptance evidence")?;
        if matches!(
            check.status.as_str(),
            "passed" | "failed" | "not_verified" | "blocked"
        ) && evidence.is_none()
        {
            return Err(format!(
                "acceptance check {index} requires concrete evidence for status {}",
                check.status
            ));
        }
        normalized_checks.push(json!({
            "criterion_index": index,
            "status": check.status,
            "evidence": evidence,
        }));
    }
    Ok(normalized_checks)
}
