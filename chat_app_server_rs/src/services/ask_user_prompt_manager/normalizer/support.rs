pub(super) const SECRET_MASK: &str = "******";

#[cfg(test)]
pub(in crate::services::ask_user_prompt_manager) fn trimmed(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_default()
}

pub(in crate::services::ask_user_prompt_manager) fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
