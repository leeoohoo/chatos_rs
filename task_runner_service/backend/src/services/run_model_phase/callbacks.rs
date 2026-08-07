// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

mod execution;
mod runtime_state;

type PendingRunStreamState = Arc<parking_lot::Mutex<PendingRunStreamEvent>>;

struct RuntimeExecutionState {
    runtime_options: AiRuntimeOptions,
    pending_stream_event: PendingRunStreamState,
    execution_outcome: Arc<parking_lot::Mutex<Option<chatos_ai_runtime::TaskExecutionOutcome>>>,
    supply_chain_evidence: Arc<parking_lot::Mutex<super::supply_chain::SupplyChainEvidenceState>>,
}
