// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse<T> {
    pub items: Vec<T>,
}

pub fn default_active() -> String {
    "active".to_string()
}

pub fn default_pending() -> String {
    "pending".to_string()
}

pub fn default_idle() -> String {
    "idle".to_string()
}
