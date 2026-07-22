// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskRunnerCallbackLanguage {
    ZhCn,
    EnUs,
}

impl TaskRunnerCallbackLanguage {
    pub fn is_english(self) -> bool {
        matches!(self, Self::EnUs)
    }
}

pub fn detect_task_runner_callback_language(
    language_text: &str,
    fallback_locale: Option<&str>,
) -> TaskRunnerCallbackLanguage {
    if contains_cjk_text(language_text) {
        return TaskRunnerCallbackLanguage::ZhCn;
    }
    if contains_latin_text(language_text) {
        return TaskRunnerCallbackLanguage::EnUs;
    }
    if fallback_locale
        .map(str::trim)
        .is_some_and(|locale| locale.eq_ignore_ascii_case("en-US"))
    {
        TaskRunnerCallbackLanguage::EnUs
    } else {
        TaskRunnerCallbackLanguage::ZhCn
    }
}

pub fn sanitize_user_visible_callback_detail(
    value: &str,
    language: TaskRunnerCallbackLanguage,
) -> String {
    if callback_detail_is_transient_service_error(value) {
        return if language.is_english() {
            "The service is temporarily unavailable. Please try again later.".to_string()
        } else {
            "服务暂时不可用，请稍后重试。".to_string()
        };
    }
    if callback_detail_is_internal_platform_error(value) {
        return if language.is_english() {
            "The task could not start. Please try again later.".to_string()
        } else {
            "任务暂时无法启动，请稍后重试。".to_string()
        };
    }
    if callback_detail_contains_sensitive_runtime_text(value) {
        return if language.is_english() {
            "The request failed. Please try again later.".to_string()
        } else {
            "请求失败，请稍后重试。".to_string()
        };
    }
    let mut lines = Vec::new();
    let mut previous_was_blank = true;
    for raw_line in value.lines() {
        let line = raw_line.trim_end();
        let normalized = line.trim().to_ascii_lowercase();
        if callback_detail_line_is_internal(normalized.as_str()) {
            continue;
        }
        let line = strip_internal_identifier_tokens(line);
        let line = replace_internal_callback_terms(line.as_str(), language);
        let line = normalize_callback_detail_spacing(line.as_str());
        let is_blank = line.trim().is_empty();
        if is_blank && previous_was_blank {
            continue;
        }
        lines.push(line);
        previous_was_blank = is_blank;
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

pub fn task_runner_callback_completion_detail(
    language: TaskRunnerCallbackLanguage,
) -> &'static str {
    if language.is_english() {
        "The requested work is complete and passed its task-level checks."
    } else {
        "已完成当前任务并通过任务内验证。"
    }
}

pub fn task_runner_callback_detail_footer(language: TaskRunnerCallbackLanguage) -> &'static str {
    if language.is_english() {
        "More implementation details are available in the task details."
    } else {
        "更多实施细节可在任务详情中查看。"
    }
}

pub fn summarize_task_runner_callback_detail(
    value: &str,
    language: TaskRunnerCallbackLanguage,
) -> Option<String> {
    const MAX_LINES: usize = 3;
    const MAX_LINE_CHARS: usize = 180;
    const MAX_TOTAL_CHARS: usize = 420;

    let value_without_code_fences = remove_callback_code_fences(value);
    let sanitized =
        sanitize_user_visible_callback_detail(value_without_code_fences.as_str(), language);
    let mut lines = Vec::new();
    let mut saw_implementation = false;
    let mut saw_integration = false;
    let mut saw_validation = false;

    for raw_line in sanitized.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let was_bullet = is_markdown_bullet(trimmed);
        let line = strip_callback_markdown_prefix(trimmed);
        if line.is_empty()
            || callback_summary_line_is_generic(line.as_str())
            || callback_summary_line_is_details_hint(line.as_str())
        {
            continue;
        }

        let signals = callback_summary_technical_signals(line.as_str());
        saw_implementation |= signals.0;
        saw_integration |= signals.1;
        saw_validation |= signals.2;
        if callback_summary_line_is_technical_noise(line.as_str()) {
            continue;
        }
        if !callback_summary_line_is_meaningful(line.as_str()) {
            continue;
        }

        let line = truncate_callback_summary_text(line.as_str(), MAX_LINE_CHARS);
        let line = if was_bullet {
            format!("- {line}")
        } else {
            line
        };
        if lines.iter().any(|existing| existing == &line) {
            continue;
        }
        lines.push(line);
        if lines.len() >= MAX_LINES {
            break;
        }
    }

    if lines.is_empty() {
        let evidence_summary = callback_evidence_summary(
            language,
            saw_implementation,
            saw_integration,
            saw_validation,
        );
        if !evidence_summary.is_empty() {
            lines.push(evidence_summary);
        }
    } else if saw_validation
        && lines.len() < MAX_LINES
        && !lines
            .iter()
            .any(|line| callback_line_mentions_validation(line))
    {
        lines.push(if language.is_english() {
            "- Relevant checks and tests passed.".to_string()
        } else {
            "- 已通过相关检查与测试验证。".to_string()
        });
    }

    let summary = lines.join("\n");
    if summary.is_empty() {
        None
    } else {
        Some(truncate_callback_summary_text(
            summary.as_str(),
            MAX_TOTAL_CHARS,
        ))
    }
}

fn remove_callback_code_fences(value: &str) -> String {
    let mut in_code_fence = false;
    value
        .lines()
        .filter_map(|line| {
            if line.trim_start().starts_with("```") {
                in_code_fence = !in_code_fence;
                return None;
            }
            (!in_code_fence).then_some(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_markdown_bullet(value: &str) -> bool {
    value.starts_with("- ") || value.starts_with("* ") || value.starts_with("+ ")
}

fn strip_callback_markdown_prefix(value: &str) -> String {
    let mut current = value.trim();
    while current.starts_with('#') {
        current = current[1..].trim_start();
    }
    for prefix in ["- ", "* ", "+ "] {
        if let Some(rest) = current.strip_prefix(prefix) {
            current = rest.trim_start();
            break;
        }
    }
    if let Some((prefix, rest)) = current.split_once(". ") {
        if !prefix.is_empty() && prefix.chars().all(|ch| ch.is_ascii_digit()) {
            current = rest.trim_start();
        }
    }
    current
        .trim_matches(|ch: char| ch == '*' || ch == '_' || ch == '`')
        .trim()
        .to_string()
}

fn callback_summary_line_is_generic(value: &str) -> bool {
    let normalized = value
        .trim_matches(|ch: char| ch.is_whitespace() || matches!(ch, ':' | '：' | '.' | '。'))
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "结果摘要"
            | "摘要"
            | "已完成"
            | "完成情况"
            | "完成结果"
            | "验证"
            | "验证结果"
            | "result summary"
            | "summary"
            | "completed"
            | "completion result"
            | "validation"
            | "validation result"
    )
}

fn callback_summary_line_is_details_hint(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("任务详情")
        || lower.contains("完整技术报告")
        || lower.contains("task details")
        || lower.contains("full technical report")
}

fn callback_summary_technical_signals(value: &str) -> (bool, bool, bool) {
    let lower = value.to_ascii_lowercase();
    let implementation = [
        "修改 ",
        "新增 ",
        "删除 ",
        "implemented ",
        "updated ",
        "changed ",
        "src/",
        "crates/",
        ".rs",
        ".ts",
        ".tsx",
        ".js",
        ".jsx",
    ]
    .iter()
    .any(|candidate| lower.contains(candidate));
    let integration = ["/api/", "endpoint", "接口", "联调"]
        .iter()
        .any(|candidate| lower.contains(candidate));
    let validation = [
        " test",
        "test ",
        "测试",
        "验证",
        "type-check",
        "typecheck",
        "cargo check",
        "cargo test",
        "npm test",
        "pnpm test",
        "build passed",
        "构建通过",
    ]
    .iter()
    .any(|candidate| lower.contains(candidate));
    (implementation, integration, validation)
}

fn callback_summary_line_is_technical_noise(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "src/",
        "crates/",
        "tests/",
        "/api/",
        "npm test",
        "pnpm test",
        "cargo test",
        "cargo check",
        "type-check",
        "typecheck",
        "git diff",
        ".rs",
        ".ts",
        ".tsx",
        ".js",
        ".jsx",
    ]
    .iter()
    .any(|candidate| lower.contains(candidate))
        || value.split_whitespace().any(|token| {
            let token = token.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_');
            token.len() >= 6
                && token.contains('_')
                && token
                    .chars()
                    .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
        })
}

fn callback_summary_line_is_meaningful(value: &str) -> bool {
    let compact = value
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect::<Vec<_>>();
    if compact.len() < 4 {
        return false;
    }
    compact
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        >= 3
}

fn callback_evidence_summary(
    language: TaskRunnerCallbackLanguage,
    implementation: bool,
    integration: bool,
    validation: bool,
) -> String {
    if !implementation && !integration && !validation {
        return String::new();
    }
    if language.is_english() {
        let mut parts = Vec::new();
        if implementation {
            parts.push("the requested implementation");
        }
        if integration {
            parts.push("the relevant integration work");
        }
        let completed = if parts.is_empty() {
            "The requested work is complete".to_string()
        } else {
            format!("Completed {}", parts.join(" and "))
        };
        if validation {
            format!("{completed}, and the relevant checks passed.")
        } else {
            format!("{completed}.")
        }
    } else {
        let mut parts = Vec::new();
        if implementation {
            parts.push("相关功能实现");
        }
        if integration {
            parts.push("接口联调");
        }
        let completed = if parts.is_empty() {
            "已完成当前任务".to_string()
        } else {
            format!("已完成{}", parts.join("与"))
        };
        if validation {
            format!("{completed}，并通过相关检查与测试验证。")
        } else {
            format!("{completed}。")
        }
    }
}

fn callback_line_mentions_validation(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    ["验证", "测试", "check", "test", "validation"]
        .iter()
        .any(|candidate| lower.contains(candidate))
}

fn truncate_callback_summary_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let chars = value.chars().take(max_chars).collect::<Vec<_>>();
    let preferred_cut = chars
        .iter()
        .enumerate()
        .rev()
        .find(|(index, ch)| {
            *index >= max_chars / 2
                && matches!(**ch, '。' | '！' | '？' | '.' | '!' | '?' | ';' | '；')
        })
        .map(|(index, _)| index + 1)
        .unwrap_or(max_chars);
    let mut truncated = chars.into_iter().take(preferred_cut).collect::<String>();
    truncated = truncated.trim_end().to_string();
    truncated.push('…');
    truncated
}

fn callback_detail_is_internal_platform_error(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "task_runner_run_phase failed",
        "task_runner_plan_phase failed",
        "resolve published prompt",
        "agent_prompt_",
        "plugin management request",
        "sandbox_environment_start_failed",
        "sandbox environment start request",
        "worker claim expired",
        "internal api secret",
        "internal api token",
    ]
    .iter()
    .any(|candidate| lower.contains(candidate))
}

fn callback_detail_is_transient_service_error(value: &str) -> bool {
    static TRANSIENT_STATUS_RE: OnceLock<Result<regex::Regex, regex::Error>> = OnceLock::new();
    let lower = value.to_ascii_lowercase();
    TRANSIENT_STATUS_RE
        .get_or_init(|| regex::Regex::new(r"status\s+5\d\d\b"))
        .as_ref()
        .is_ok_and(|regex| regex.is_match(value))
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
        .any(|candidate| lower.contains(candidate))
}

fn callback_detail_contains_sensitive_runtime_text(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "api_key",
        "api key",
        "authorization",
        "bearer ",
        "access_token",
        "internal_trace",
    ]
    .iter()
    .any(|candidate| lower.contains(candidate))
        || value.contains("trace=")
}

fn contains_cjk_text(value: &str) -> bool {
    value.chars().any(|ch| {
        matches!(
            ch as u32,
            0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
        )
    })
}

fn contains_latin_text(value: &str) -> bool {
    value.chars().filter(|ch| ch.is_ascii_alphabetic()).count() >= 3
}

fn callback_detail_line_is_internal(normalized_line: &str) -> bool {
    const INTERNAL_KEYS: &[&str] = &[
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
    ];
    INTERNAL_KEYS
        .iter()
        .any(|key| normalized_line.contains(key))
}

fn strip_internal_identifier_tokens(value: &str) -> String {
    static IDENTIFIER_RE: OnceLock<Result<regex::Regex, regex::Error>> = OnceLock::new();
    let regex = IDENTIFIER_RE.get_or_init(|| {
        regex::Regex::new(r"`?[0-9a-fA-F]{6,8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{10,12}`?")
    });
    match regex {
        Ok(regex) => regex.replace_all(value, "").into_owned(),
        Err(_) => value.to_string(),
    }
}

fn replace_internal_callback_terms(value: &str, language: TaskRunnerCallbackLanguage) -> String {
    let replacements = if language.is_english() {
        [
            ("technical_overview", "technical overview"),
            ("implementation_plan", "implementation plan"),
            ("implementation plan", "implementation plan"),
            ("get_project_dependency_graph()", "project dependency check"),
            ("ready=true", "dependency graph is ready"),
            ("pending", "pending preparation"),
            ("contains", "contains relationship"),
            ("blocks", "prerequisite dependency"),
            ("requirement", "requirement"),
            ("`draft`", "draft"),
            ("draft", "draft"),
            ("`reviewing`", "in review"),
            ("reviewing", "in review"),
            ("`approved`", "approved"),
            ("approved", "approved"),
        ]
    } else {
        [
            ("technical_overview", "技术概览"),
            ("implementation_plan", "实施计划"),
            ("implementation plan", "实施计划"),
            ("get_project_dependency_graph()", "项目依赖关系检查"),
            ("ready=true", "依赖关系已就绪"),
            ("pending", "待准备"),
            ("contains", "包含关系"),
            ("blocks", "前置依赖"),
            ("requirement", "需求"),
            ("`draft`", "草稿"),
            ("draft", "草稿"),
            ("`reviewing`", "评审中"),
            ("reviewing", "评审中"),
            ("`approved`", "已批准"),
            ("approved", "已批准"),
        ]
    };
    replacements
        .into_iter()
        .fold(value.to_string(), |current, (from, to)| {
            current.replace(from, to)
        })
}

fn normalize_callback_detail_spacing(value: &str) -> String {
    let mut normalized = value.replace('`', "").replace("-  ", "- ");
    while normalized.contains("  ") {
        normalized = normalized.replace("  ", " ");
    }
    for (from, to) in [
        ("该 需求", "该需求"),
        ("需求 从", "需求从"),
        ("在 需求", "在需求"),
        ("需求 《", "需求《"),
        ("拉取 需求 下", "拉取需求下"),
        ("调整 需求 状态", "调整需求状态"),
        ("确认 包含关系", "确认包含关系"),
    ] {
        normalized = normalized.replace(from, to);
    }
    normalized.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_http_status_is_replaced_with_user_visible_message() {
        assert_eq!(
            sanitize_user_visible_callback_detail(
                "upstream returned status 503 while dispatching",
                TaskRunnerCallbackLanguage::EnUs,
            ),
            "The service is temporarily unavailable. Please try again later."
        );
    }

    #[test]
    fn internal_identifiers_are_removed_from_callback_detail() {
        assert_eq!(
            sanitize_user_visible_callback_detail(
                "任务 `123e4567-e89b-12d3-a456-426614174000` 已完成",
                TaskRunnerCallbackLanguage::ZhCn,
            ),
            "任务 已完成"
        );
    }

    #[test]
    fn callback_summary_keeps_user_facing_results_and_drops_technical_noise() {
        let summary = summarize_task_runner_callback_detail(
            "## 已完成\n\n- 已梳理项目用途、核心流程和主要模块。\n- 修改 src/service.js\n- 执行 npm test\n```text\ninternal log\n```",
            TaskRunnerCallbackLanguage::ZhCn,
        )
        .expect("summary");

        assert!(summary.contains("已梳理项目用途、核心流程和主要模块"));
        assert!(summary.contains("已通过相关检查与测试验证"));
        assert!(!summary.contains("src/service.js"));
        assert!(!summary.contains("npm test"));
        assert!(!summary.contains("internal log"));
    }

    #[test]
    fn callback_summary_converts_only_technical_evidence_to_a_readable_result() {
        let summary = summarize_task_runner_callback_detail(
            "- 修改 src/service.js\n- 调用 /api/dashboard\n- 执行 npm test",
            TaskRunnerCallbackLanguage::ZhCn,
        )
        .expect("summary");

        assert_eq!(
            summary,
            "已完成相关功能实现与接口联调，并通过相关检查与测试验证。"
        );
    }
}
