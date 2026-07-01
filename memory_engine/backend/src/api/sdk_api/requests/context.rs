// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SdkComposeContextRequest {
    pub tenant_id: String,
    pub subject_id: Option<String>,
    pub related_subject_ids: Option<Vec<String>>,
    pub thread_id: String,
    pub policy: Option<crate::models::ComposeContextPolicy>,
}
