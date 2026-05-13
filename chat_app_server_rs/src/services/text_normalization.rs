use std::collections::HashSet;

pub fn normalize_optional_text_ref(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
}

pub fn normalize_optional_text_owned(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

pub fn normalize_required_text_owned(
    value: Option<String>,
    field: &str,
) -> Result<String, String> {
    normalize_optional_text_owned(value).ok_or_else(|| format!("{field} is required"))
}

pub fn normalize_string_vec(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for item in values {
        let trimmed = item.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.clone()) {
            out.push(trimmed);
        }
    }
    out
}

pub fn resolve_visible_user_ids(scope_user_id: &str) -> Vec<String> {
    let normalized = scope_user_id.trim();
    if normalized.is_empty() {
        Vec::new()
    } else {
        vec![normalized.to_string()]
    }
}
