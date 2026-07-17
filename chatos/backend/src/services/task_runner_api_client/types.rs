// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct UserServiceTaskRunnerExchange {
    pub base_url: String,
    pub access_token: String,
    pub task_runner_agent_account_id: String,
    pub contact_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct UserServiceTaskRunnerTokenResponse {
    pub(super) access_token: String,
    #[serde(default = "default_task_runner_token_expires_in")]
    pub(super) expires_in: i64,
}

fn default_task_runner_token_expires_in() -> i64 {
    3600
}

#[derive(Debug, Clone)]
pub struct ExchangedTaskRunnerToken {
    pub access_token: String,
    pub expires_in: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TaskRunnerTaskRecord {
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CancelTaskRunnerTaskRequest {
    pub reason: String,
    pub replacement_task_ids: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct SubmitTaskRunnerPromptRequest {
    pub values: Option<Value>,
    pub selection: Option<Value>,
    pub reason: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct CancelTaskRunnerPromptRequest {
    pub reason: Option<String>,
}
