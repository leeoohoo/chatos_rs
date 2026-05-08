use std::sync::OnceLock;

use tokio::sync::broadcast;

use crate::models::JobRun;

#[derive(Debug, Clone)]
pub struct JobRunRealtimeEvent {
    pub action: &'static str,
    pub job_run: JobRun,
}

static JOB_RUN_REALTIME_BUS: OnceLock<broadcast::Sender<JobRunRealtimeEvent>> = OnceLock::new();

fn job_run_realtime_bus() -> &'static broadcast::Sender<JobRunRealtimeEvent> {
    JOB_RUN_REALTIME_BUS.get_or_init(|| {
        let (sender, _) = broadcast::channel(512);
        sender
    })
}

pub fn init_job_run_realtime_bus() {
    let _ = job_run_realtime_bus();
}

pub fn subscribe_job_run_events() -> broadcast::Receiver<JobRunRealtimeEvent> {
    job_run_realtime_bus().subscribe()
}
