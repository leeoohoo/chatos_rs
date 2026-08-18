// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(in crate::services) mod execution;
pub(in crate::services) mod runtime_state;

type PendingRunStreamState = Arc<parking_lot::Mutex<PendingRunStreamEvent>>;

struct RuntimeExecutionState {
    pub(super) runtime_options: AiRuntimeOptions,
    pub(super) pending_stream_event: PendingRunStreamState,
    lifecycle_state: Arc<parking_lot::Mutex<TaskRunnerLifecycleState>>,
    progress: Arc<chatos_ai_runtime::TaskExecutionProgressState>,
    supply_chain_evidence: Arc<parking_lot::Mutex<super::supply_chain::SupplyChainEvidenceState>>,
}
