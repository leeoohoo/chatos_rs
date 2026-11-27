use super::super::{
    task_tool_definitions::TASK_BUILTIN_KIND_VALUES,
    task_tools::{TaskPluginHint, TaskScheduleArgs},
};
use chrono::DateTime;
use serde_json::{json, Value};
use std::collections::HashSet;

pub(in crate::application) fn task_runtime_settings(
    thinking_level: Option<&str>,
) -> Result<Value, String> {
    let Some(level) = thinking_level
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(Value::Null);
    };
    let normalized = match level.to_ascii_lowercase().as_str() {
        "off" | "disabled" | "none" => "none",
        "auto" => "auto",
        "minimal" => "minimal",
        "low" => "low",
        "medium" => "medium",
        "high" => "high",
        "xhigh" => "xhigh",
        "max" => "max",
        _ => return Err("invalid thinking_level".to_string()),
    };
    Ok(json!({
        "selected_thinking_level": normalized,
        "reasoning_enabled": normalized != "none"
    }))
}

pub(in crate::application) fn task_prompt(
    objective: &str,
    description: &str,
    context_summary: &str,
    expected_outputs: &[String],
    acceptance_criteria: &[String],
    input_payload: &Value,
) -> Result<String, String> {
    let objective = objective.trim();
    if objective.is_empty() {
        return Err("task objective cannot be empty".to_string());
    }
    let english = !contains_cjk(objective) && !contains_cjk(description);
    let mut prompt = if english {
        format!("Task Objective:\n{objective}")
    } else {
        format!("任务目标：\n{objective}")
    };
    let description = description.trim();
    if !description.is_empty() {
        prompt.push_str(if english {
            "\n\nTask Description:\n"
        } else {
            "\n\n任务说明：\n"
        });
        prompt.push_str(description);
    }
    let context_summary = context_summary.trim();
    if context_summary.is_empty() {
        return Err("task context_summary cannot be empty".to_string());
    }
    prompt.push_str(if english {
        "\n\nCurated Context (minimum self-contained source context):\n"
    } else {
        "\n\n精选上下文（最小且自包含的来源上下文）：\n"
    });
    prompt.push_str(context_summary);
    append_task_contract_list(
        &mut prompt,
        if english {
            "Expected Outputs"
        } else {
            "预期输出"
        },
        expected_outputs,
        "expected_outputs",
    )?;
    append_task_contract_list(
        &mut prompt,
        if english {
            "Acceptance Criteria"
        } else {
            "验收标准"
        },
        acceptance_criteria,
        "acceptance_criteria",
    )?;
    if !input_payload.is_null() {
        prompt.push_str(if english {
            "\n\nInput Data:\n"
        } else {
            "\n\n输入数据：\n"
        });
        prompt.push_str(
            &serde_json::to_string(input_payload)
                .map_err(|error| format!("task input is not serializable: {error}"))?,
        );
    }
    prompt.push_str(if english {
        "\n\n[Project Inspection Evidence Contract]\nWhen the objective asks to inspect, review, evaluate, understand, or summarize a real project, do not finish from README or architecture documents alone. Inspect and cross-check the build or package manifests, source entry points, representative core implementations, and tests or CI configuration. Report the concrete files and checks used as evidence and state any validation gap. When terminal or build tools are available and the user expects current behavior or correctness, run a proportionate validation. Finish once the evidence is sufficient; do not perform unrelated work.\n\n[Output Language Policy]\nUse the language requested by the user or used in this task for all user-visible prose. Preserve code identifiers, commands, paths, APIs, and product names."
    } else {
        "\n\n[项目检查最低证据契约]\n当目标要求检查、审查、评估、了解或总结真实项目时，不得只依据 README 或架构文档结束。至少读取并交叉核对构建或包管理清单、源码入口、具有代表性的核心实现，以及测试或 CI 配置；结果必须列出实际作为证据的文件和检查，并说明尚未验证的部分。若本轮提供终端或构建工具，且用户关注当前行为或正确性，应执行与目标相称的验证。证据充分后及时结束，不得扩展无关工作。\n\n[输出语言规则]\n所有用户可见文本使用用户要求或当前任务所使用的语言；代码标识符、命令、路径、API 和产品名保持原样。"
    });
    Ok(prompt)
}

fn append_task_contract_list(
    prompt: &mut String,
    heading: &str,
    values: &[String],
    field: &str,
) -> Result<(), String> {
    if values.is_empty() {
        return Err(format!("{field} must contain at least one item"));
    }
    prompt.push_str("\n\n");
    prompt.push_str(heading);
    prompt.push_str(":\n");
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            return Err(format!("{field} cannot contain an empty item"));
        }
        prompt.push_str("- ");
        prompt.push_str(value);
        prompt.push('\n');
    }
    Ok(())
}

pub(in crate::application) fn validate_nonempty_text(
    value: &str,
    field: &str,
) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} cannot be empty"));
    }
    Ok(())
}

pub(in crate::application) fn validated_builtin_kinds(
    requires_execution: bool,
    requested: Vec<String>,
) -> Result<Vec<String>, String> {
    let allowed = TASK_BUILTIN_KIND_VALUES.into_iter().collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut kinds = Vec::with_capacity(requested.len() + 2);
    for kind in requested {
        if !allowed.contains(kind.as_str()) {
            return Err(format!("unsupported enabled_builtin_kind: {kind}"));
        }
        if !seen.insert(kind.clone()) {
            return Err(format!("duplicated enabled_builtin_kind: {kind}"));
        }
        kinds.push(kind);
    }
    if !requires_execution
        && kinds.iter().any(|kind| {
            matches!(
                kind.as_str(),
                "CodeMaintainerWrite" | "TerminalController" | "RequirementSurveyWrite"
            )
        })
    {
        return Err(
            "execution-only builtin capabilities require requires_execution=true".to_string(),
        );
    }
    complete_kind_dependency(
        &mut kinds,
        &mut seen,
        "CodeMaintainerWrite",
        "CodeMaintainerRead",
    );
    complete_kind_dependency(
        &mut kinds,
        &mut seen,
        "RequirementSurveyWrite",
        "RequirementSurveyRead",
    );
    if seen.contains("RequirementSurveyWrite") {
        // The durable survey owns user input for this Task; a generic form must not bypass it.
        kinds.retain(|kind| kind != "AskUser");
    }
    Ok(kinds)
}

fn complete_kind_dependency(
    kinds: &mut Vec<String>,
    seen: &mut HashSet<String>,
    selected: &str,
    dependency: &str,
) {
    if seen.contains(selected) && seen.insert(dependency.to_string()) {
        kinds.push(dependency.to_string());
    }
}

pub(in crate::application) fn validate_external_mcp_ids(
    config_ids: &[String],
) -> Result<Vec<String>, String> {
    let mut normalized = Vec::with_capacity(config_ids.len());
    let mut seen = HashSet::with_capacity(config_ids.len());
    for raw in config_ids {
        let id = raw.trim();
        if id.is_empty() || id.len() > 256 {
            return Err("external MCP ids must contain 1..=256 characters".to_string());
        }
        if !seen.insert(id.to_string()) {
            return Err(format!("duplicated external MCP id: {id}"));
        }
        normalized.push(id.to_string());
    }
    Ok(normalized)
}

pub(in crate::application) fn validate_plugin_hints(
    hints: &[TaskPluginHint],
) -> Result<(), String> {
    let mut seen = HashSet::new();
    for hint in hints {
        let key = hint.plugin_key.trim();
        let component = hint.component_key.trim();
        if key.is_empty() || component.is_empty() || !seen.insert((key, component)) {
            return Err(
                "plugin_hints must contain unique non-empty plugin_key/component_key pairs"
                    .to_string(),
            );
        }
        if hint.reason.chars().count() > 1_000 {
            return Err("plugin hint reason cannot exceed 1000 characters".to_string());
        }
    }
    Ok(())
}

fn contains_cjk(value: &str) -> bool {
    value.chars().any(|character| {
        ('\u{3400}'..='\u{4dbf}').contains(&character)
            || ('\u{4e00}'..='\u{9fff}').contains(&character)
    })
}

pub(in crate::application) fn contact_async_schedule(
    schedule: Option<TaskScheduleArgs>,
) -> Result<Value, String> {
    let schedule = schedule.unwrap_or_default();
    if let Some(mode) = schedule.mode.as_deref() {
        if !matches!(mode, "manual" | "once" | "interval" | "contact_async") {
            return Err(format!("unsupported Task schedule mode: {mode}"));
        }
    }
    if schedule
        .interval_seconds
        .is_some_and(|seconds| seconds <= 0)
    {
        return Err("Task schedule.interval_seconds must be greater than zero".to_string());
    }
    let run_at = schedule
        .run_at
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let run_at_unix_ms = run_at
        .as_deref()
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|value| value.timestamp_millis())
                .map_err(|_| "Task schedule.run_at must be an RFC 3339 timestamp".to_string())
        })
        .transpose()?;
    Ok(json!({
        "mode": "contact_async",
        "run_at": run_at,
        "run_at_unix_ms": run_at_unix_ms,
        "interval_seconds": Value::Null,
    }))
}
