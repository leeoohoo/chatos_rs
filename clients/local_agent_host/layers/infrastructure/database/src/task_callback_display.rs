// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

#[cfg(test)]
pub(super) fn callback_content(
    _title: &str,
    objective: &str,
    event: &str,
    terminal_outcome: Option<&Value>,
    english: bool,
) -> String {
    callback_content_with_status(
        _title,
        objective,
        event,
        terminal_outcome,
        english,
        "failed",
    )
}

pub(super) fn callback_content_with_status(
    _title: &str,
    objective: &str,
    event: &str,
    terminal_outcome: Option<&Value>,
    english: bool,
    display_status: &str,
) -> String {
    let detail = user_visible_callback_detail(objective, event, terminal_outcome, english)
        .map(|(_, detail)| detail);
    if event == "task.completed" {
        return detail.unwrap_or_else(|| completion_receipt(english));
    }
    let headline = if english {
        match event {
            "task.run.started" => "I've started working on it.".to_string(),
            "task.failed" if display_status == "evidence_incomplete" => {
                "I completed part of this, but couldn't verify everything yet.".to_string()
            }
            "task.failed" if display_status == "acceptance_failed" => {
                "The result didn't meet all the requirements.".to_string()
            }
            "task.failed" => "I couldn't complete this.".to_string(),
            "task.blocked" => "I can't continue yet.".to_string(),
            "task.cancelled" => "I've stopped working on it.".to_string(),
            _ => "I'm continuing to work on it.".to_string(),
        }
    } else {
        match event {
            "task.run.started" => "我已经开始处理了。".to_string(),
            "task.failed" if display_status == "evidence_incomplete" => {
                "我完成了一部分，但还有内容未核验。".to_string()
            }
            "task.failed" if display_status == "acceptance_failed" => {
                "结果未达到全部要求。".to_string()
            }
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
            let detail = if matches!(event, "task.failed" | "task.blocked") {
                sanitize_failure_detail(&detail, english)
            } else {
                sanitize_visible_detail(&detail, english)
            };
            detail.map(|detail| (source, detail))
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

// Deliver only the model's user-facing output, never the whole outcome/checkpoint.
// Unlike a lifecycle preview this retains Markdown, code and the complete report.
pub(super) fn full_delivery_detail(value: &Value, english: bool) -> Option<String> {
    let object = value.as_object()?;
    let report = object.get("report").and_then(|value| {
        value
            .as_str()
            .or_else(|| value.get("content").and_then(Value::as_str))
    });
    let detail = ["content", "answer", "text"]
        .iter()
        .find_map(|key| {
            object
                .get(*key)
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
        })
        .or(report.filter(|s| !s.trim().is_empty()))
        .or_else(|| object.get("result_summary").and_then(Value::as_str))?;
    let lines = detail
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let lower = trimmed.to_ascii_lowercase();
            if callback_detail_line_is_internal(&lower)
                || lower.starts_with("[task outcome")
                || lower.starts_with("[tool boundary")
                || lower.starts_with("reasoning:")
                || lower.starts_with("tool_calls:")
                || lower.contains("invocation_id")
                || lower.contains("claim_token")
                || line_exposes_secret(trimmed)
            {
                return None;
            }
            let safe = if line
                .split_whitespace()
                .any(|token| looks_like_uuid(token.trim_matches(['`', ',', '.', ';', ':'])))
            {
                strip_internal_identifiers(line)
            } else {
                line.to_string()
            };
            Some(replace_internal_terms(&safe, english))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let detail = lines.trim();
    (!detail.is_empty()).then(|| detail.to_string())
}

pub(super) fn progress_callback_content_with_status(
    title: &str,
    event: &str,
    english: bool,
    display_status: &str,
) -> String {
    let title = sanitize_visible_detail(title, english)
        .map(|value| truncate_chars(&value.replace('\n', " "), 180))
        .unwrap_or_else(|| if english { "Task" } else { "任务" }.to_string());
    let state = match (english, event) {
        (true, "task.run.started") => "Started",
        (true, "task.completed") => "Completed",
        (true, "task.failed") if display_status == "evidence_incomplete" => {
            "Partially complete; not fully verified"
        }
        (true, "task.failed") if display_status == "acceptance_failed" => "Requirements not met",
        (true, "task.failed") if display_status == "execution_failed" => "Execution failed",
        (true, "task.failed") => "Not completed",
        (true, "task.blocked") => "Blocked — attention required",
        (true, "task.cancelled") => "Cancelled",
        (false, "task.run.started") => "已开始",
        (false, "task.completed") => "已完成",
        (false, "task.failed") if display_status == "evidence_incomplete" => "部分完成，尚未核验",
        (false, "task.failed") if display_status == "acceptance_failed" => "未通过验收",
        (false, "task.failed") if display_status == "execution_failed" => "执行失败",
        (false, "task.failed") => "未完成",
        (false, "task.blocked") => "阻塞，需要处理",
        (false, "task.cancelled") => "已取消",
        _ => "…",
    };
    if english {
        format!("{state}: {title}")
    } else {
        format!("{state}：{title}")
    }
}

pub(super) fn sanitize_visible_detail(value: &str, english: bool) -> Option<String> {
    sanitize_visible_text(value, english)
}

// Error classification is intentionally separate from ordinary result sanitization.
// Successful reports routinely discuss HTTP 5xx responses, timeouts, unavailable
// dependencies, or platform failures that were diagnosed and fixed. Treating those
// words as the Run's own failure discards the completed report and replaces it with a
// misleading retry message. The authoritative Task event decides whether this
// classifier may run; the report text itself never changes a successful event.
fn sanitize_failure_detail(value: &str, english: bool) -> Option<String> {
    if value == "Execution was interrupted and the result of the current step could not be confirmed. Review the execution record before retrying to avoid repeating an operation." {
        return Some(if english {
            value.to_string()
        } else {
            "任务执行意外中断，无法确认当前步骤的结果。请先检查执行记录，再决定是否重试，避免重复操作。".to_string()
        });
    }
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
    sanitize_visible_text(value, english)
}

fn sanitize_visible_text(value: &str, english: bool) -> Option<String> {
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
        // Model output often explains authentication code using identifiers such as
        // `access_token` or placeholders such as `Authorization: Bearer ...`. Those are
        // safe documentation, not credentials. Drop only lines that contain an assigned,
        // non-placeholder secret so one example cannot replace an otherwise successful
        // task callback with a misleading request-failed message.
        if line_exposes_secret(trimmed) {
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
        "internal_trace",
        "trace=",
    ]
    .iter()
    .any(|key| line.contains(key))
}

fn line_exposes_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if lower.contains("postgres://") {
        return true;
    }
    if bearer_secret_value(value, &lower) {
        return true;
    }
    if assigned_secret_value(value, &lower, "authorization", true) {
        return true;
    }
    [
        "x-api-key",
        "api_key",
        "api key",
        "password",
        "database_url",
        "access_token",
    ]
    .iter()
    .any(|key| assigned_secret_value(value, &lower, key, false))
}

fn assigned_secret_value(value: &str, lower: &str, key: &str, bearer_value: bool) -> bool {
    let mut offset = 0;
    while let Some(relative_index) = lower[offset..].find(key) {
        let key_end = offset + relative_index + key.len();
        let suffix = value[key_end..].trim_start_matches(|character: char| {
            character.is_whitespace() || matches!(character, '`' | '"' | '\'' | ']' | ')')
        });
        let Some(delimiter) = suffix.chars().next() else {
            return false;
        };
        if matches!(delimiter, ':' | '=') {
            let mut candidate = suffix[delimiter.len_utf8()..].trim_start();
            if bearer_value {
                let candidate_lower = candidate.to_ascii_lowercase();
                if let Some(remainder) = candidate_lower.strip_prefix("bearer") {
                    let bearer_bytes = candidate.len() - remainder.len();
                    candidate = candidate[bearer_bytes..].trim_start();
                }
            }
            if !secret_value_is_placeholder(candidate) {
                return true;
            }
        }
        offset = key_end;
    }
    false
}

fn bearer_secret_value(value: &str, lower: &str) -> bool {
    let mut offset = 0;
    while let Some(relative_index) = lower[offset..].find("bearer ") {
        let value_start = offset + relative_index + "bearer ".len();
        if !secret_value_is_placeholder(&value[value_start..]) {
            return true;
        }
        offset = value_start;
    }
    false
}

fn secret_value_is_placeholder(value: &str) -> bool {
    let token = value
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_matches(['`', '"', '\'', ',', ';']);
    if token.is_empty()
        || token
            .chars()
            .all(|character| matches!(character, '.' | '…' | '*'))
    {
        return true;
    }
    let normalized = token
        .trim_matches(['<', '>', '{', '}', '[', ']', '$'])
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "token"
            | "access_token"
            | "api_key"
            | "apikey"
            | "password"
            | "secret"
            | "redacted"
            | "masked"
            | "placeholder"
    )
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
    const MAX_MANAGED_ARTIFACT_LINKS: usize = 8;
    const MAX_LINE_CHARS: usize = 180;
    const MAX_TOTAL_CHARS: usize = 420;
    let mut preview_lines = Vec::new();
    let mut managed_artifact_links = Vec::new();
    for raw_line in value.lines() {
        let line = raw_line
            .trim()
            .trim_start_matches(['#', '-', '*', '+', ' '])
            .trim_matches(['*', '_', '`'])
            .trim();
        if line.is_empty() || callback_summary_heading(line) || callback_summary_noise(line) {
            continue;
        }
        if let Some(link) = managed_artifact_markdown_link(line) {
            let link = link.to_string();
            if managed_artifact_links.len() < MAX_MANAGED_ARTIFACT_LINKS
                && !managed_artifact_links.contains(&link)
            {
                managed_artifact_links.push(link);
            }
            continue;
        }
        if preview_lines.len() < MAX_LINES {
            let line = truncate_chars(line, MAX_LINE_CHARS);
            if !preview_lines.contains(&line) {
                preview_lines.push(line);
            }
        }
    }
    if preview_lines.is_empty() && managed_artifact_links.is_empty() {
        return Some(completion_receipt(english));
    }
    if !managed_artifact_links.is_empty() {
        // Delivery links can appear after several title/metadata lines. Scan the
        // complete final response and put every bounded managed link first so a
        // lifecycle preview cannot turn a successful file delivery into an
        // unreachable filename-only receipt.
        managed_artifact_links.extend(preview_lines);
        return Some(managed_artifact_links.join("\n"));
    }
    Some(truncate_chars(&preview_lines.join("\n"), MAX_TOTAL_CHARS))
}

fn managed_artifact_markdown_link(value: &str) -> Option<&str> {
    let destination_marker = "](chatos-plugin-artifacts/";
    let destination_start = value.find(destination_marker)?;
    let label_start = value[..destination_start].rfind('[')?;
    let destination_body_start = destination_start + destination_marker.len();
    let destination_end = destination_body_start + value[destination_body_start..].find(')')? + 1;
    Some(&value[label_start..destination_end])
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
