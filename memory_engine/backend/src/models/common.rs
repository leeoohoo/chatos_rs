use chrono::Utc;

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

pub fn default_active() -> String {
    "active".to_string()
}

pub fn default_pending() -> String {
    "pending".to_string()
}
