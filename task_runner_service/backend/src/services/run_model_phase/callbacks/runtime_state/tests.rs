// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

fn review_checkpoint(trigger: TaskExecutionReviewTrigger) -> TaskExecutionReviewCheckpoint {
    TaskExecutionReviewCheckpoint {
        iteration: 24,
        trigger,
        read_only_iterations: 24,
        missing_read_failures: 2,
        checkpoints_since_action: 3,
        policy: TaskExecutionReviewPolicy::default(),
    }
}

#[test]
fn checkpoint_guidance_requires_one_actionable_review_decision() {
    let guidance = checkpoint_guidance_message(
        review_checkpoint(TaskExecutionReviewTrigger::ReadOnlyLoop),
        &["src/lib.rs".to_string()],
    );
    let content = guidance["content"].as_str().expect("guidance content");

    for expected in [
        "IMPLEMENT",
        "LOCATE",
        "VERIFY",
        "COMPLETE",
        "BLOCKED",
        "状态 + 证据依据 + 未满足的验收项",
        "目标文件/函数或唯一命令 + 具体动作 + 完成判据",
        "每个结论都必须有具体证据",
        "下一条可见输出只能是执行该指令的一次工具调用",
        "失败就引用新失败的具体原因给出纠正后的",
        "工具始终完整可用",
        "src/lib.rs",
        "不得从任务描述或自然语言摘要重新猜测路径",
        "不得仅为确认它们存在而重复全仓搜索",
    ] {
        assert!(content.contains(expected), "missing {expected}");
    }
    for forbidden in [
        "第 3 次",
        "当前计数",
        "连续多轮只读/观察",
        "你又在重复",
        "工具已临时关闭",
    ] {
        assert!(!content.contains(forbidden), "unexpected {forbidden}");
    }
    assert!(!content.contains("关闭观察类工具"));
}

#[test]
fn review_contract_persists_and_accepts_newer_checkpoint_evidence() {
    let active_review = parking_lot::Mutex::new(None);
    assert!(persistent_review_checkpoint(&active_review, None).is_none());

    let first = review_checkpoint(TaskExecutionReviewTrigger::ReadOnlyLoop);
    assert_eq!(
        persistent_review_checkpoint(&active_review, Some(first)),
        Some(first)
    );
    assert_eq!(
        persistent_review_checkpoint(&active_review, None),
        Some(first)
    );

    let newer = review_checkpoint(TaskExecutionReviewTrigger::StaleProjectWrite);
    assert_eq!(
        persistent_review_checkpoint(&active_review, Some(newer)),
        Some(newer)
    );
    assert_eq!(
        persistent_review_checkpoint(&active_review, None),
        Some(newer)
    );
}

#[test]
fn missing_path_review_directs_a_bounded_location_step() {
    let guidance = checkpoint_guidance_message(
        review_checkpoint(TaskExecutionReviewTrigger::MissingTargetedReads),
        &[],
    );
    let content = guidance["content"].as_str().expect("guidance content");

    assert!(content.contains("已有工具结果否定了当前路径假设"));
    assert!(content.contains("只执行一次限定目录、关键词和预期命中的定位动作"));
    assert!(content.contains("只允许执行一次限定目录、关键词和预期命中的 LOCATE 动作"));
    assert!(content.contains("不得继续扩大搜索范围"));
}

#[test]
fn stale_write_review_directs_an_exact_rebased_edit() {
    let guidance = checkpoint_guidance_message(
        review_checkpoint(TaskExecutionReviewTrigger::StaleProjectWrite),
        &["src/lib.rs".to_string()],
    );
    let content = guidance["content"].as_str().expect("guidance content");

    assert!(content.contains("最近一次代码写入未生效"));
    assert!(content.contains("最近一次成功读取的目标内容作为权威版本"));
    assert!(content.contains("直接生成基于该文本的精确编辑"));
    assert!(content.contains("写入成功后转入必要验证"));
}

#[test]
fn task_outcome_review_accepts_strict_succeeded_contract() {
    let outcome = parse_task_execution_outcome(
        r#"{"status":"succeeded","summary":"implemented and verified","blocking_reason":null,"unmet_acceptance_criteria":[],"verification_evidence":["cargo test passed"]}"#,
    )
    .expect("valid outcome");

    assert_eq!(outcome.status, TaskExecutionOutcomeStatus::Succeeded);
    assert_eq!(outcome.verification_evidence, ["cargo test passed"]);
}

#[test]
fn task_outcome_review_rejects_success_without_evidence() {
    let error = parse_task_execution_outcome(
        r#"{"status":"succeeded","summary":"claimed completion","blocking_reason":null,"unmet_acceptance_criteria":[],"verification_evidence":[]}"#,
    )
    .expect_err("missing evidence must fail closed");

    assert!(error.contains("verification evidence"));
}

#[test]
fn task_outcome_review_requires_one_to_one_runtime_backed_acceptance_evidence() {
    let outcome = parse_task_execution_outcome_with_evidence(
        r#"{"status":"succeeded","summary":"implemented and verified","blocking_reason":null,"unmet_acceptance_criteria":[],"verification_evidence":["dashboard render test passed"],"acceptance_evidence":[{"criterion":"dashboard renders","evidence":["dashboard render test passed"],"referenced_paths":["src/Dashboard.tsx"],"commands":["npm test"]}],"referenced_paths":["src/Dashboard.tsx"],"referenced_endpoints":[]}"#,
        &["dashboard renders".to_string()],
        &["src/Dashboard.tsx".to_string()],
        &["npm test".to_string()],
        &[],
        true,
    )
    .expect("runtime-backed acceptance evidence");
    assert_eq!(outcome.acceptance_evidence.len(), 1);

    let error = parse_task_execution_outcome_with_evidence(
        r#"{"status":"succeeded","summary":"claimed","blocking_reason":null,"unmet_acceptance_criteria":[],"verification_evidence":["dashboard render test passed"],"acceptance_evidence":[{"criterion":"dashboard renders","evidence":["dashboard render test passed"],"referenced_paths":[],"commands":["npm test"]}],"referenced_paths":[],"referenced_endpoints":[]}"#,
        &["dashboard renders".to_string()],
        &[],
        &["npm run build".to_string()],
        &[],
        true,
    )
    .expect_err("unrecorded command must fail closed");
    assert!(error.contains("without successful runtime evidence"));
}

#[test]
fn task_outcome_review_accepts_recorded_browser_evidence() {
    let outcome = parse_task_execution_outcome_with_evidence(
        r#"{"status":"succeeded","summary":"verified responsive layout","blocking_reason":null,"unmet_acceptance_criteria":[],"verification_evidence":["320px browser snapshot rendered without overflow"],"acceptance_evidence":[{"criterion":"works at 320px","evidence":["320px browser snapshot rendered without overflow"],"referenced_paths":[],"commands":[],"tool_names":["browser_tools_browser_snapshot"]}],"referenced_paths":[],"referenced_endpoints":[]}"#,
        &["works at 320px".to_string()],
        &[],
        &[],
        &["browser_tools_browser_snapshot".to_string()],
        true,
    )
    .expect("recorded browser evidence");

    assert_eq!(outcome.acceptance_evidence.len(), 1);
}

#[test]
fn task_outcome_review_rejects_markdown_wrapped_json() {
    let error = parse_task_execution_outcome("```json\n{\"status\":\"blocked\"}\n```")
        .expect_err("review response must be strict JSON");

    assert!(error.contains("invalid task execution outcome JSON"));
}

#[test]
fn task_outcome_review_uses_strict_provider_json_schema() {
    let format = task_execution_outcome_output_format();

    assert_eq!(format.name, "task_execution_outcome");
    assert!(format.strict);
    assert_eq!(
        format.schema.pointer("/additionalProperties"),
        Some(&Value::Bool(false))
    );
    assert_eq!(
        format.schema.pointer("/properties/status/enum"),
        Some(&json!(["succeeded", "blocked"]))
    );
    assert!(format
        .schema
        .pointer("/required")
        .and_then(Value::as_array)
        .is_some_and(|required| required.iter().any(|value| value == "acceptance_evidence")));
}

#[test]
fn task_outcome_protocol_repair_is_bounded_and_actionable() {
    let raw = "x".repeat(TASK_OUTCOME_RAW_RESPONSE_MAX_CHARS + 10);
    let bounded = bounded_task_outcome_raw_response(raw.as_str());
    assert!(bounded.ends_with("...[truncated]"));
    assert!(bounded.chars().count() < raw.chars().count());

    let repair = task_execution_outcome_repair_message("expected value").to_string();
    assert!(repair.contains("expected value"));
    assert!(repair.contains("schema-constrained JSON object"));
    assert!(repair.contains("Do not change the underlying task conclusion"));
}

#[test]
fn task_outcome_review_distinguishes_execution_and_planning_evidence() {
    let execution = task_execution_outcome_review_message(true, &[]).to_string();
    let planning = task_execution_outcome_review_message(false, &[]).to_string();

    assert!(execution.contains("actual tool results"));
    assert!(execution.contains("changed project files"));
    assert!(execution.contains("referenced_paths"));
    assert!(execution.contains("referenced_endpoints"));
    assert!(execution.contains("workspace-relative paths only"));
    assert!(planning.contains("non-execution planning task"));
    assert!(planning.contains("do not require file changes"));
}

#[test]
fn browser_session_event_is_extracted_from_nested_tool_result() {
    let payload = json!({
        "name": "browser_tools_browser_navigate",
        "result": {
            "success": true,
            "browser_session": {
                "id": "h_session_123",
                "mode": "managed",
                "status": "active",
                "event": "updated"
            }
        }
    });

    let session = browser_session_event_payload(&payload).expect("browser session");
    assert_eq!(session["id"], "h_session_123");
    assert_eq!(session["status"], "active");
}

#[test]
fn sanitize_runtime_event_payload_redacts_ask_user_tool_results() {
    let payload = json!({
        "name": "ask_user_prompt_mixed_form",
        "success": true,
        "content": serde_json::to_string(&json!({
            "status": "submitted",
            "values": {
                "public_port_policy": "direct_open_defaults",
                "admin_password": "super-secret"
            },
            "selection": "proceed"
        })).expect("content"),
        "result": {
            "status": "submitted",
            "values": {
                "token": "secret-token"
            },
            "selection": "proceed"
        }
    });

    let sanitized = sanitize_runtime_event_payload(payload);
    let content = sanitized["content"].as_str().expect("content");

    assert!(!content.contains("super-secret"));
    assert!(content.contains(EVENT_SECRET_VALUE_MASK));
    assert_eq!(
        sanitized["result"]["values"]["token"],
        EVENT_SECRET_VALUE_MASK
    );
    assert_eq!(sanitized["result"]["selection"], "proceed");
}

#[test]
fn sanitize_runtime_event_payload_redacts_ask_user_output_in_model_input() {
    let payload = json!({
        "input": [
            {
                "type": "function_call_output",
                "call_id": "call_1",
                "output": serde_json::to_string(&json!({
                    "status": "submitted",
                    "values": {
                        "admin_password": "super-secret"
                    },
                    "selection": "proceed"
                })).expect("output")
            }
        ]
    });

    let sanitized = sanitize_runtime_event_payload(payload);
    let output = sanitized["input"][0]["output"].as_str().expect("output");

    assert!(!output.contains("super-secret"));
    assert!(output.contains(EVENT_SECRET_VALUE_MASK));
}

#[test]
fn sanitize_runtime_event_payload_keeps_unrelated_status_values_objects() {
    let payload = json!({
        "status": "ok",
        "values": {
            "debug": "keep-me"
        }
    });

    let sanitized = sanitize_runtime_event_payload(payload);

    assert_eq!(sanitized["values"]["debug"], "keep-me");
}
