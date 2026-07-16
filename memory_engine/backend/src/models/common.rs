// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::Utc;

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

pub fn now_plus_seconds_rfc3339(seconds: i64) -> String {
    (Utc::now() + chrono::Duration::seconds(seconds.max(1))).to_rfc3339()
}

pub fn default_active() -> String {
    "active".to_string()
}
