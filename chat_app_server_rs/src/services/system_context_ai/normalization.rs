use serde_json::{json, Value};

pub(super) fn normalize_optional_text(value: Option<String>, max_len: usize) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| {
            if value.chars().count() > max_len {
                value.chars().take(max_len).collect()
            } else {
                value
            }
        })
}

pub(super) fn normalize_string_list(
    values: Option<Vec<String>>,
    max_items: usize,
    max_item_len: usize,
) -> Vec<String> {
    values
        .unwrap_or_default()
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| {
            if value.chars().count() > max_item_len {
                value.chars().take(max_item_len).collect()
            } else {
                value
            }
        })
        .take(max_items)
        .collect()
}

pub(super) fn normalize_score(value: Option<&Value>) -> Option<i64> {
    let raw = value?;
    let score = if let Some(v) = raw.as_i64() {
        v
    } else if let Some(v) = raw.as_u64() {
        v as i64
    } else if let Some(v) = raw.as_f64() {
        v.round() as i64
    } else {
        return None;
    };

    Some(score.clamp(0, 100))
}

pub(super) fn normalize_warning_list(value: Option<&Value>, max_items: usize) -> Vec<String> {
    let mut out = Vec::new();

    let values: Vec<String> = match value {
        Some(Value::String(value)) => vec![value.to_string()],
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|item| item.as_str().map(|text| text.to_string()))
            .collect(),
        _ => Vec::new(),
    };

    for item in values {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        if out.iter().any(|existing| existing == item) {
            continue;
        }
        out.push(item.to_string());
        if out.len() >= max_items {
            break;
        }
    }

    out
}

pub(super) fn normalize_quality_report(value: Option<&Value>) -> Option<Value> {
    let obj = value.and_then(|value| value.as_object())?;

    let mut out = serde_json::Map::new();
    if let Some(v) = normalize_score(obj.get("clarity")) {
        out.insert("clarity".to_string(), json!(v));
    }
    if let Some(v) = normalize_score(obj.get("constraint_completeness")) {
        out.insert("constraint_completeness".to_string(), json!(v));
    }
    if let Some(v) = normalize_score(obj.get("conflict_risk")) {
        out.insert("conflict_risk".to_string(), json!(v));
    }
    if let Some(v) = normalize_score(obj.get("verbosity")) {
        out.insert("verbosity".to_string(), json!(v));
    }

    let overall = normalize_score(obj.get("overall")).or_else(|| {
        let clarity = out.get("clarity").and_then(|value| value.as_i64())?;
        let completeness = out
            .get("constraint_completeness")
            .and_then(|value| value.as_i64())?;
        let conflict = out.get("conflict_risk").and_then(|value| value.as_i64())?;
        let verbosity = out.get("verbosity").and_then(|value| value.as_i64())?;
        Some(((clarity + completeness + (100 - conflict) + (100 - verbosity)) / 4).clamp(0, 100))
    });

    if let Some(v) = overall {
        out.insert("overall".to_string(), json!(v));
    }

    let warnings = normalize_warning_list(obj.get("warnings"), 8);
    if !warnings.is_empty() {
        out.insert("warnings".to_string(), json!(warnings));
    }

    if out.is_empty() {
        None
    } else {
        Some(Value::Object(out))
    }
}

fn contains_any(text: &str, words: &[&str]) -> bool {
    words.iter().any(|word| text.contains(word))
}

pub(super) fn local_quality_report(content: &str) -> Value {
    let trimmed = content.trim();
    let char_count = trimmed.chars().count() as i64;
    let line_count = trimmed
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .count() as i64;

    let lower = trimmed.to_lowercase();

    let mut warnings = Vec::new();

    let structure_bonus = if trimmed.contains("###")
        || trimmed.contains("## ")
        || trimmed.contains("1.")
        || trimmed.contains("- ")
    {
        20
    } else {
        0
    };

    let length_bonus = if (120..=1600).contains(&char_count) {
        10
    } else {
        0
    };

    let line_bonus = if line_count >= 4 { 10 } else { 0 };

    let clarity = (55 + structure_bonus + length_bonus + line_bonus).clamp(0, 100);

    let mut constraint_hits = 0;
    for keyword in [
        "必须",
        "禁止",
        "不得",
        "输出",
        "格式",
        "边界",
        "拒绝",
        "must",
        "must not",
        "do not",
        "format",
        "constraints",
    ] {
        if trimmed.contains(keyword) || lower.contains(keyword) {
            constraint_hits += 1;
        }
    }

    let mut completeness = 40 + (constraint_hits * 8).min(40);
    if trimmed.contains("示例") || lower.contains("example") {
        completeness += 8;
    }
    if trimmed.contains("拒绝") || lower.contains("refuse") || lower.contains("fallback") {
        completeness += 8;
    }
    completeness = completeness.clamp(0, 100);

    let prefers_short = contains_any(trimmed, &["简短", "简洁", "精简"])
        || contains_any(lower.as_str(), &["concise", "brief", "short"]);
    let prefers_detail = contains_any(trimmed, &["详细", "全面", "完整", "详尽"])
        || contains_any(lower.as_str(), &["detailed", "comprehensive", "thorough"]);

    let mut conflict_risk = 20;
    if prefers_short && prefers_detail {
        conflict_risk += 35;
        warnings.push("同时出现“简短/简洁”和“详细/全面”要求，可能存在冲突。".to_string());
    }
    if constraint_hits <= 2 {
        conflict_risk += 10;
    }
    conflict_risk = conflict_risk.clamp(0, 100);

    let verbosity = if char_count < 120 {
        25
    } else if char_count <= 900 {
        45
    } else if char_count <= 1800 {
        65
    } else {
        85
    };

    if char_count < 120 {
        warnings.push("内容偏短，建议补充角色边界、输出格式和异常处理策略。".to_string());
    }
    if completeness < 65 {
        warnings.push("约束条款偏少，建议补充“必须/禁止/输出格式/拒答规则”。".to_string());
    }
    if verbosity > 75 {
        warnings.push("文本较长，建议压缩重复描述，避免执行成本过高。".to_string());
    }

    warnings.truncate(8);

    let overall =
        ((clarity + completeness + (100 - conflict_risk) + (100 - verbosity)) / 4).clamp(0, 100);

    json!({
        "clarity": clarity,
        "constraint_completeness": completeness,
        "conflict_risk": conflict_risk,
        "verbosity": verbosity,
        "overall": overall,
        "warnings": warnings,
    })
}
