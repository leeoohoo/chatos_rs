// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::infrastructure::{LocalHookInvocation, LocalHookInvocationResult, LocalHookRunner};
use async_trait::async_trait;
use chatos_agent_profiles::{
    LocalHookContextProvider, LocalToolHookGate, MAIN_CHAT_PROFILE_KEY, TASK_EXECUTION_PROFILE_KEY,
};
use chatos_ai_runtime::message_item;
use chatos_local_agent_ports::{
    LocalHookEventRecord, LocalHookEventSpec, LocalHookInvocationCompletion,
    LocalHookInvocationRecord, LocalHookInvocationSpec, LocalHookOutputSpill,
};
use chatos_local_agent_protocol::{
    LocalAgentRunClaim, LocalAgentRunRecord, LocalAgentStepOutcome, LocalAgentToolCall,
    LocalAgentToolInvocationRecord, LocalAgentToolOutcome, LocalPluginHookBundleRecord,
    LocalPluginHookDefinition, ResolveAgentTeamHookContextCommand, StartConversationTurnCommand,
};
use chatos_local_agent_runtime::LocalAgentRuntime;
use futures::future::join_all;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, Weak,
    },
    time::{Duration, Instant},
};
use tokio::sync::{Mutex as AsyncMutex, Semaphore};
use uuid::Uuid;

const MAXIMUM_EVENT_CONTEXT_BYTES: usize = 512 * 1_024;
const MAXIMUM_INLINE_HOOK_TOOL_RESPONSE_BYTES: usize = 256 * 1_024;
const MINIMUM_INLINE_BINARY_REDACTION_BYTES: usize = 16 * 1_024;
const MAXIMUM_STDERR_SUMMARY_BYTES: usize = 4_096;
const MAXIMUM_PARENT_DEPTH: usize = 64;
const OUTPUT_SPILL_THRESHOLD_BYTES: usize = 16 * 1_024;
const MAXIMUM_STOP_BLOCKS: u64 = 3;
const MAXIMUM_CONCURRENT_HOOKS: usize = 8;
const MAXIMUM_MATCHING_HOOKS_PER_EVENT: usize = 256;

/// Executes the trusted Codex-compatible lifecycle Hooks which must run before
/// a model request. The durable runtime owns replay; this coordinator owns
/// event adaptation, matching, isolated execution, and deterministic merging.
pub struct LocalHookCoordinator {
    runtime: Arc<LocalAgentRuntime>,
    runner: Arc<dyn LocalHookRunner>,
    workspace_root: Option<PathBuf>,
    process_session_sequence: u64,
    active_sessions: Mutex<HashMap<String, ActiveHookSession>>,
    session_generations: Mutex<HashMap<String, u64>>,
    connected_clients: AtomicUsize,
    inflight_events: Mutex<HashMap<String, Weak<AsyncMutex<()>>>>,
    hook_execution_permits: Arc<Semaphore>,
}

pub(crate) enum LocalHookPromptDecision {
    Allow,
    Block { reason: String },
}

impl LocalHookCoordinator {
    pub fn new<R>(
        runtime: Arc<LocalAgentRuntime>,
        runner: R,
        workspace_root: Option<PathBuf>,
    ) -> Result<Self, String>
    where
        R: LocalHookRunner + 'static,
    {
        if workspace_root
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        {
            return Err("Hook workspace root must be absolute".to_string());
        }
        Ok(Self {
            runtime,
            runner: Arc::new(runner),
            workspace_root,
            process_session_sequence: stable_sequence(&["process", &Uuid::new_v4().to_string()]),
            active_sessions: Mutex::new(HashMap::new()),
            session_generations: Mutex::new(HashMap::new()),
            connected_clients: AtomicUsize::new(0),
            inflight_events: Mutex::new(HashMap::new()),
            hook_execution_permits: Arc::new(Semaphore::new(MAXIMUM_CONCURRENT_HOOKS)),
        })
    }

    pub(crate) fn client_connected(&self) {
        self.connected_clients.fetch_add(1, Ordering::AcqRel);
    }

    pub(crate) fn client_disconnected(&self) {
        let _ =
            self.connected_clients
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                    Some(current.saturating_sub(1))
                });
    }
}

include!("hook_coordinator/gates.rs");
include!("hook_coordinator/session_lifecycle.rs");
include!("hook_coordinator/event_builder.rs");
include!("hook_coordinator/execution.rs");
include!("hook_coordinator/event_types.rs");
include!("hook_coordinator/tool_protocol.rs");
include!("hook_coordinator/outcome.rs");

#[cfg(test)]
#[path = "hook_coordinator/tests.rs"]
mod tests;
