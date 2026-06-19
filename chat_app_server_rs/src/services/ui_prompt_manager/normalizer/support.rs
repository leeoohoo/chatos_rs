pub(super) const SECRET_MASK: &str = "******";

pub(in crate::services::ui_prompt_manager) fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
