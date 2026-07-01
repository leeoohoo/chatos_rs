// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

mod execution;
mod runtime_state;

type PendingRunStreamState = Arc<parking_lot::Mutex<PendingRunStreamEvent>>;

struct RuntimeExecutionState {
    runtime_options: AiRuntimeOptions,
    pending_stream_event: PendingRunStreamState,
    task_completed_abort: Arc<AtomicBool>,
    stop_cancel_poll: Arc<AtomicBool>,
    cancel_poll_handle: tokio::task::JoinHandle<()>,
}
