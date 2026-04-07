pub mod action_requests;
pub mod auth;
pub mod contacts;
pub mod conversations;
pub mod messages;
pub mod runs;

pub fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}
