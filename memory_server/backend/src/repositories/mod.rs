pub mod agents;
pub mod auth;
pub mod configs;
pub mod contacts;
pub mod jobs;
pub mod memories;
pub mod messages;
pub mod sessions;
pub mod skills;
pub mod summaries;

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}
