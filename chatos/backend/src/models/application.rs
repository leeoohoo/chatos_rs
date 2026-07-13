// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Application {
    pub id: String,
    pub name: String,
    pub url: String,
    pub description: Option<String>,
    pub user_id: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}
