use super::*;

mod execution;
mod runtime_state;

type PendingRunStreamState = Arc<parking_lot::Mutex<PendingRunStreamEvent>>;

struct RuntimeExecutionState {
    runtime_options: AiRuntimeOptions,
    pending_stream_event: PendingRunStreamState,
    stop_cancel_poll: Arc<AtomicBool>,
    cancel_poll_handle: tokio::task::JoinHandle<()>,
}
