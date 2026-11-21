// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::LocalToolExecutor;
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    validate_task_acceptance_report, LocalAgentRunRecord, LocalAgentToolInvocationRecord,
    LocalAgentToolOutcome,
};
use chatos_local_agent_runtime::LocalAgentRuntime;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    net::{Ipv4Addr, SocketAddrV4, TcpListener},
    sync::{Arc, Mutex},
};

pub const TASK_PROCESS_RECORD_TOOL: &str = "task_run_process_record_process";
pub const TASK_PROCESS_ALLOCATE_SERVICE_TOOL: &str = "task_run_process_allocate_service";
pub const TASK_OUTCOME_REPORT_TOOL: &str = "task_run_process_report_outcome";
pub const TASK_PROCESS_TOOL_NAMES: [&str; 3] = [
    TASK_PROCESS_ALLOCATE_SERVICE_TOOL,
    TASK_PROCESS_RECORD_TOOL,
    TASK_OUTCOME_REPORT_TOOL,
];

const PROCESS_ENTRY_MAX_CHARS: usize = 16_000;
const OUTCOME_REASON_MAX_CHARS: usize = 2_000;

mod model_tools;
mod service_registry;

pub use model_tools::{task_process_model_tools, task_process_prompt_item};
pub use service_registry::LocalTaskProcessToolExecutor;
#[cfg(test)]
use service_registry::RunServicePortRegistry;

#[cfg(test)]
mod tests;
