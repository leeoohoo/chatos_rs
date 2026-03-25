pub mod agents;
pub mod auth;
pub mod configs;
pub mod contacts;
pub mod jobs;
pub mod memories;
pub mod messages;
pub mod project_agent_links;
pub mod projects;
mod session_support;
pub mod sessions;
pub mod skills;
pub mod summaries;
mod summaries_support;
pub mod turn_runtime_snapshots;

pub fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

pub fn default_active_status() -> String {
    "active".to_string()
}

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}
