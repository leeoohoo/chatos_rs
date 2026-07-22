// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

pub(super) const BROWSER_VISION_TRANSPORT: &str = "responses";
pub(super) const DEFAULT_CONTACT_VISION_MAX_OUTPUT_TOKENS: i64 = 700;

#[derive(Debug, Clone)]
pub(super) struct BrowserVisionPreparedContext {
    pub(super) session_model_cfg: Option<Value>,
    pub(super) selected_model_id: Option<String>,
    pub(super) user_id: Option<String>,
    pub(super) contact_agent_id: Option<String>,
    pub(super) contact_system_prompt: Option<String>,
    pub(super) warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct BrowserVisionCandidate {
    pub(super) mode: &'static str,
    pub(super) prompt_source: &'static str,
    pub(super) contact_agent_id: Option<String>,
    pub(super) instructions: Option<String>,
    pub(super) model: String,
    pub(super) provider: String,
    pub(super) thinking_level: Option<String>,
    pub(super) temperature: f64,
    pub(super) api_key: String,
    pub(super) base_url: String,
    pub(super) request_body_limit_bytes: Option<usize>,
    pub(super) max_transient_retries: Option<usize>,
}

#[derive(Debug, Clone)]
pub(super) struct BrowserVisionOutput {
    pub(super) analysis: String,
    pub(super) mode: String,
    pub(super) prompt_source: String,
    pub(super) contact_agent_id: Option<String>,
    pub(super) model: String,
    pub(super) provider: String,
    pub(super) transport: String,
    pub(super) fallback_used: bool,
    pub(super) attempts: Vec<Value>,
    pub(super) warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct BrowserVisionRunResult {
    pub(super) analysis: String,
    pub(super) transport: &'static str,
}
