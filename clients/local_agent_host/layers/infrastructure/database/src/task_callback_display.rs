// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

pub(super) fn callback_content(
    _title: &str,
    objective: &str,
    event: &str,
    terminal_outcome: Option<&Value>,
    english: bool,
) -> String {
    let detail = user_visible_callback_detail(objective, event, terminal_outcome, english)
        .map(|(_, detail)| detail);
    if event == "task.completed" {
        return detail.unwrap_or_else(|| completion_receipt(english));
    }
    let headline = if english {
        match event {
            "task.run.started" => "I've started working on it.".to_string(),
            "task.failed" => "I couldn't complete this.".to_string(),
            "task.blocked" => "I can't continue yet.".to_string(),
            "task.cancelled" => "I've stopped working on it.".to_string(),
            _ => "I'm continuing to work on it.".to_string(),
        }
    } else {
        match event {
            "task.run.started" => "我已经开始处理了。".to_string(),
            "task.failed" => "我这次没有处理完成。".to_string(),
            "task.blocked" => "我暂时还无法继续处理。".to_string(),
            "task.cancelled" => "我已经停下来了。".to_string(),
            _ => "我还在继续处理。".to_string(),
        }
    };
    if matches!(event, "task.run.started" | "task.cancelled") {
        return headline;
    }
    let Some(detail) = detail else {
        return headline;
    };
    format!("{headline}\n\n{detail}")
}

pub(super) fn user_visible_callback_detail(
    objective: &str,
    event: &str,
    terminal_outcome: Option<&Value>,
    english: bool,
) -> Option<(&'static str, String)> {
    if event == "task.completed" {
        return terminal_outcome
            .and_then(visible_detail_with_source)
            .and_then(|(source, detail)| {
                let detail = sanitize_visible_detail(&detail, english)?;
                summarize_completion_detail(&detail, english).map(|summary| (source, summary))
            })
            .or_else(|| {
                summarize_completion_detail(objective, english)
                    .map(|summary| ("task_objective", summary))
            })
            .or_else(|| Some(("completion_receipt", completion_receipt(english))));
    }
    if matches!(event, "task.run.started" | "task.cancelled") {
        return None;
    }
    terminal_outcome
        .and_then(visible_detail_with_source)
        .and_then(|(source, detail)| {
            sanitize_visible_detail(&detail, english).map(|detail| (source, detail))
        })
}

fn completion_receipt(english: bool) -> String {
    if english {
        "I've finished working on it.".to_string()
    } else {
        "我已经处理完了。".to_string()
    }
}

fn visible_detail_with_source(value: &Value) -> Option<(&'static str, String)> {
    let object = value.as_object()?;
    for key in [
        "result_summary",
        "content",
        "answer",
        "text",
        "error",
        "message",
        "reason",
    ] {
        if let Some(detail) = object
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some((key, detail.to_string()));
        }
    }
    object
        .get("report")
        .and_then(|report| {
            report
                .as_str()
                .or_else(|| report.get("content").and_then(Value::as_str))
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|detail| ("report", detail.to_string()))
}

pub(super) fn sanitize_visible_detail(value: &str, english: bool) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    if upstream_connection_interrupted(&lower) {
        let retry_count = callback_retry_count(value);
        return Some(match (english, retry_count) {
            (true, Some(count)) => format!(
                "The model connection ended before processing started and did not recover after {count} automatic retries."
            ),
            (true, None) => "The model connection ended before processing started and did not recover after automatic retries.".to_string(),
            (false, Some(count)) => format!("模型连接在开始处理前中断，已自动重试 {count} 次仍未恢复。"),
            (false, None) => "模型连接在开始处理前中断，自动重试后仍未恢复。".to_string(),
        });
    }
    if transient_service_error(&lower) {
        return Some(if english {
            "The service is temporarily unavailable. Please try again later.".to_string()
        } else {
            "服务暂时不可用，请稍后重试。".to_string()
        });
    }
    if internal_platform_error(&lower) {
        return Some(if english {
            "The task could not start. Please try again later.".to_string()
        } else {
            "任务暂时无法启动，请稍后重试。".to_string()
        });
    }
    if [
        "authorization: bearer",
        "authorization",
        "bearer ",
        "x-api-key",
        "api key",
        "api_key",
        "api_key=",
        "password=",
        "database_url=",
        "postgres://",
        "access_token",
        "internal_trace",
        "trace=",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return Some(if english {
            "The request failed. Please try again later.".to_string()
        } else {
            "请求失败，请稍后重试。".to_string()
        });
    }
    let mut in_code_fence = false;
    let mut lines = Vec::new();
    for raw_line in value.lines() {
        let trimmed = raw_line.trim();
        if trimmed.starts_with("```") {
            in_code_fence = !in_code_fence;
            continue;
        }
        if in_code_fence || trimmed.is_empty() {
            continue;
        }
        let normalized = trimmed.to_ascii_lowercase();
        if callback_detail_line_is_internal(&normalized)
            || normalized.starts_with("[task outcome")
            || normalized.starts_with("[tool boundary")
            || normalized.starts_with("reasoning:")
            || normalized.starts_with("tool_calls:")
            || normalized.contains("invocation_id")
            || normalized.contains("claim_token")
        {
            continue;
        }
        if lines.last().is_some_and(|line: &String| line == trimmed) {
            continue;
        }
        let line = replace_internal_terms(&strip_internal_identifiers(trimmed), english);
        let line = normalize_detail_spacing(&line);
        if !line.is_empty() {
            lines.push(line);
        }
    }
    let detail = lines.join("\n");
    (!detail.is_empty()).then_some(detail)
}

fn upstream_connection_interrupted(lower: &str) -> bool {
    [
        "connection closed before message completed",
        "disconnect/reset before headers",
        "upstream connect error",
        "connection reset by peer",
        "peer closed connection",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn callback_retry_count(value: &str) -> Option<usize> {
    let lower = value.to_ascii_lowercase();
    for marker in ["已重试", "retried"] {
        let Some(after) = lower.split(marker).nth(1) else {
            continue;
        };
        if let Some(count) = after
            .split(|character: char| !character.is_ascii_digit())
            .find(|part| !part.is_empty())
            .and_then(|part| part.parse().ok())
        {
            return Some(count);
        }
    }
    None
}

fn transient_service_error(lower: &str) -> bool {
    let has_server_status = lower.split_whitespace().any(|part| {
        part.len() == 3 && part.starts_with('5') && part.chars().all(|ch| ch.is_ascii_digit())
    });
    has_server_status
        || [
            "failed to fetch",
            "error sending request for url",
            "connection refused",
            "connection reset",
            "network is unreachable",
            "service unavailable",
            "bad gateway",
            "gateway timeout",
            "timed out",
            "timeout",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
}

fn internal_platform_error(lower: &str) -> bool {
    [
        "task_runner_run_phase failed",
        "resolve published prompt",
        "agent_prompt_",
        "plugin management request",
        "worker claim expired",
        "internal api secret",
        "internal api token",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn callback_detail_line_is_internal(line: &str) -> bool {
    [
        "requirement_id",
        "document_id",
        "work_item_id",
        "task_id",
        "run_id",
        "source_run_id",
        "source_turn_id",
        "source_user_message_id",
        "conversation_turn_id",
        "project_id",
        "parent_task_id",
        "tool_call_id",
        "model_config_id",
    ]
    .iter()
    .any(|key| line.contains(key))
}

fn strip_internal_identifiers(value: &str) -> String {
    value
        .split_whitespace()
        .filter(|token| !looks_like_uuid(token.trim_matches(['`', ',', '.', ';', ':'])))
        .collect::<Vec<_>>()
        .join(" ")
}

fn looks_like_uuid(value: &str) -> bool {
    let parts = value.split('-').collect::<Vec<_>>();
    matches!(parts.as_slice(), [a, b, c, d, e]
        if (6..=8).contains(&a.len())
            && b.len() == 4 && c.len() == 4 && d.len() == 4
            && (10..=12).contains(&e.len())
            && parts.iter().all(|part| part.chars().all(|ch| ch.is_ascii_hexdigit())))
}

fn replace_internal_terms(value: &str, english: bool) -> String {
    let replacements = if english {
        [
            ("technical_overview", "technical overview"),
            ("implementation_plan", "implementation plan"),
            ("get_project_dependency_graph()", "project dependency check"),
            ("ready=true", "dependency graph is ready"),
        ]
    } else {
        [
            ("technical_overview", "技术概览"),
            ("implementation_plan", "实施计划"),
            ("get_project_dependency_graph()", "项目依赖关系检查"),
            ("ready=true", "依赖关系已就绪"),
        ]
    };
    replacements
        .into_iter()
        .fold(value.to_string(), |current, (from, to)| {
            current.replace(from, to)
        })
}

fn normalize_detail_spacing(value: &str) -> String {
    let mut normalized = value.replace('`', "").replace("-  ", "- ");
    while normalized.contains("  ") {
        normalized = normalized.replace("  ", " ");
    }
    normalized.trim().to_string()
}

fn summarize_completion_detail(value: &str, english: bool) -> Option<String> {
    const MAX_LINES: usize = 3;
    const MAX_LINE_CHARS: usize = 180;
    const MAX_TOTAL_CHARS: usize = 420;
    let mut lines = Vec::new();
    for raw_line in value.lines() {
        let line = raw_line
            .trim()
            .trim_start_matches(['#', '-', '*', '+', ' '])
            .trim_matches(['*', '_', '`'])
            .trim();
        if line.is_empty() || callback_summary_heading(line) || callback_summary_noise(line) {
            continue;
        }
        let line = truncate_chars(line, MAX_LINE_CHARS);
        if !lines.contains(&line) {
            lines.push(line);
        }
        if lines.len() == MAX_LINES {
            break;
        }
    }
    if lines.is_empty() {
        return Some(completion_receipt(english));
    }
    Some(truncate_chars(&lines.join("\n"), MAX_TOTAL_CHARS))
}

fn callback_summary_heading(value: &str) -> bool {
    matches!(
        value
            .trim_matches([':', '：', '.', '。'])
            .to_ascii_lowercase()
            .as_str(),
        "结果摘要"
            | "摘要"
            | "已完成"
            | "完成情况"
            | "验证结果"
            | "result summary"
            | "summary"
            | "completed"
            | "validation result"
    )
}

fn callback_summary_noise(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "任务详情",
        "完整技术报告",
        "task details",
        "full technical report",
        "cargo test",
        "cargo check",
        "npm test",
        "pnpm test",
        "git diff",
        "target/",
        "node_modules/",
        "src/",
        "crates/",
        ".rs",
        ".ts",
        ".tsx",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

pub(super) fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let prefix = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

pub(super) fn contains_cjk(value: &Value) -> bool {
    match value {
        Value::String(value) => value.chars().any(|character| {
            ('\u{3400}'..='\u{4dbf}').contains(&character)
                || ('\u{4e00}'..='\u{9fff}').contains(&character)
        }),
        Value::Array(values) => values.iter().any(contains_cjk),
        Value::Object(values) => values.values().any(contains_cjk),
        _ => false,
    }
}
