use serde_json::{json, Value};

use crate::services::ai_prompt_tool::parse_json_loose;

use super::normalization::{
    local_quality_report, normalize_quality_report, normalize_score, normalize_warning_list,
};

pub(super) fn parse_prompt_candidates(raw: &str, max_count: usize) -> Vec<Value> {
    let mut out = Vec::new();
    let mut items: Vec<Value> = Vec::new();

    if let Some(parsed) = parse_json_loose(raw) {
        if let Some(arr) = parsed.get("candidates").and_then(|value| value.as_array()) {
            items = arr.to_vec();
        } else if let Some(arr) = parsed.as_array() {
            items = arr.to_vec();
        } else if parsed.get("content").is_some() {
            items.push(parsed);
        }
    }

    for (index, item) in items.iter().enumerate() {
        if out.len() >= max_count {
            break;
        }

        let content = item
            .get("content")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if content.is_empty() {
            continue;
        }

        let title = item
            .get("title")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("候选-{}", index + 1));

        let mut candidate = json!({
            "title": title,
            "content": content,
        });

        if let Some(score) = normalize_score(item.get("score")) {
            candidate["score"] = json!(score);
        }

        let highlights = normalize_warning_list(item.get("highlights"), 5);
        if !highlights.is_empty() {
            candidate["highlights"] = json!(highlights);
        }

        if let Some(report) = normalize_quality_report(item.get("report")) {
            candidate["report"] = report;
        }

        out.push(candidate);
    }

    if out.is_empty() {
        let fallback = raw.trim();
        if !fallback.is_empty() {
            out.push(json!({
                "title": "候选-1",
                "content": fallback,
            }));
        }
    }

    out.truncate(max_count);
    out
}

pub(super) fn parse_optimize_response(raw: &str, original_content: &str) -> Value {
    let parsed = parse_json_loose(raw);

    let mut optimized_content = String::new();
    let mut score_before = None;
    let mut score_after = None;
    let mut report_after = None;
    let mut warnings = Vec::new();

    if let Some(value) = parsed.as_ref() {
        if let Some(obj) = value.as_object() {
            optimized_content = obj
                .get("optimized_content")
                .or_else(|| obj.get("content"))
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim()
                .to_string();

            score_before = normalize_score(obj.get("score_before"));
            score_after = normalize_score(obj.get("score_after"));
            report_after = normalize_quality_report(obj.get("report_after"))
                .or_else(|| normalize_quality_report(obj.get("report")));

            warnings = normalize_warning_list(obj.get("warnings"), 8);
        }

        if optimized_content.is_empty() {
            if let Some(first) = value
                .get("candidates")
                .and_then(|value| value.as_array())
                .and_then(|arr| arr.first())
            {
                optimized_content = first
                    .get("content")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
            }
        }
    }

    if optimized_content.is_empty() {
        optimized_content = raw.trim().to_string();
    }

    let before_report = local_quality_report(original_content);
    let fallback_after = local_quality_report(optimized_content.as_str());

    let report_after = report_after.unwrap_or(fallback_after);

    if warnings.is_empty() {
        warnings = normalize_warning_list(report_after.get("warnings"), 8);
    }

    if score_before.is_none() {
        score_before = normalize_score(before_report.get("overall"));
    }
    if score_after.is_none() {
        score_after = normalize_score(report_after.get("overall"));
    }

    json!({
        "optimized_content": optimized_content,
        "score_before": score_before,
        "score_after": score_after,
        "report_after": report_after,
        "warnings": warnings,
    })
}

pub(super) fn parse_evaluate_response(raw: &str, content: &str) -> Value {
    let parsed = parse_json_loose(raw);

    let report = if let Some(value) = parsed.as_ref() {
        normalize_quality_report(value.get("report"))
            .or_else(|| normalize_quality_report(Some(value)))
            .unwrap_or_else(|| local_quality_report(content))
    } else {
        local_quality_report(content)
    };

    json!({"report": report})
}
