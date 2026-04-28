pub(super) const SECRET_MASK: &str = "******";

pub(in crate::services::ui_prompt_manager) fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub(super) fn trimmed(value: Option<&str>) -> String {
    value.map(|item| item.trim().to_string()).unwrap_or_default()
}
