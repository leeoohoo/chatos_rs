// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use serde_json::{json, Value};

use super::test_support::{
    before_request_set_task_done_on_nth_request, build_test_client,
    build_test_client_with_max_iterations, chunk_callbacks, demo_echo_tool, empty_callbacks,
    ensure_memory_session, run_process_with_tools, setup_sqlite_task_board, start_mock_provider,
    unique_session_id, MockProviderStep, RunProcessWithToolsArgs,
};
use crate::services::agent_runtime::ai_client::AiClientCallbacks;
use crate::services::task_manager::TaskDraft;
use crate::services::user_settings::AiClientSettings;

mod context;
mod follow_up;
mod recovery_http;
mod recovery_retry;
mod recovery_stream_tools;
mod recovery_tools;
mod transport;
