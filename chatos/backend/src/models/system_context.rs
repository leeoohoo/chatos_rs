// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemContext {
    pub id: String,
    pub name: String,
    pub content: Option<String>,
    pub user_id: String,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}
