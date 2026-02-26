use super::super::*;
pub(crate) use crate::core::async_bridge::block_on_result;
use crate::core::mcp_tools::ToolStreamChunkCallback;

static JOBS: Lazy<Mutex<HashMap<String, JobRecord>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static JOB_EVENTS: Lazy<Mutex<HashMap<String, Vec<JobEvent>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static JOB_CANCEL_FLAGS: Lazy<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static JOB_STREAM_CHUNK_SINKS: Lazy<Mutex<HashMap<String, ToolStreamChunkCallback>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static ROUTER_TRACE_LOG_PATH: Lazy<Option<PathBuf>> = Lazy::new(|| {
    settings::ensure_state_files()
        .ok()
        .map(|paths| paths.root.join(ROUTER_TRACE_LOG_FILE))
});
static ROUTER_TRACE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

pub(crate) fn trace_log_path_string() -> Option<String> {
    ROUTER_TRACE_LOG_PATH
        .as_ref()
        .map(|path| path.to_string_lossy().to_string())
}

pub(crate) fn trace_router_node(
    node: &str,
    stage: &str,
    job_id: Option<&str>,
    session_id: Option<&str>,
    run_id: Option<&str>,
    payload: Option<Value>,
) {
    let Some(path) = ROUTER_TRACE_LOG_PATH.as_ref() else {
        return;
    };

    let payload_json = payload.map(|value| {
        let raw = serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string());
        truncate_for_event(raw.as_str(), ROUTER_TRACE_PAYLOAD_MAX_CHARS)
    });

    let line = serde_json::to_string(&json!({
        "id": generate_id("node"),
        "ts": Utc::now().to_rfc3339(),
        "node": node,
        "stage": stage,
        "job_id": job_id,
        "session_id": session_id,
        "run_id": run_id,
        "payload_json": payload_json,
    }))
    .unwrap_or_default();

    if line.is_empty() {
        return;
    }

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Ok(_guard) = ROUTER_TRACE_LOCK.lock() {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = file.write_all(line.as_bytes());
            let _ = file.write_all(b"\n");
        }
    }
}

fn emit_progress_stream_chunk(
    job_id: &str,
    event_type: &str,
    payload: Option<Value>,
    session_id: &str,
    run_id: &str,
    created_at: &str,
) {
    let sink = JOB_STREAM_CHUNK_SINKS
        .lock()
        .ok()
        .and_then(|sinks| sinks.get(job_id).cloned());

    let Some(callback) = sink else {
        return;
    };

    let envelope = json!({
        "kind": "sub_agent_progress",
        "event": event_type,
        "job_id": job_id,
        "session_id": session_id,
        "run_id": run_id,
        "payload": payload.unwrap_or(Value::Null),
        "created_at": created_at,
    });

    if let Ok(raw) = serde_json::to_string(&envelope) {
        callback(format!("{}\n", raw));
    }
}

pub(crate) fn set_job_stream_sink(job_id: &str, sink: ToolStreamChunkCallback) {
    if let Ok(mut sinks) = JOB_STREAM_CHUNK_SINKS.lock() {
        sinks.insert(job_id.to_string(), sink);
    }
}

pub(crate) fn remove_job_stream_sink(job_id: &str) {
    if let Ok(mut sinks) = JOB_STREAM_CHUNK_SINKS.lock() {
        sinks.remove(job_id);
    }
}

pub(crate) fn emit_job_raw_stream_chunk(job_id: &str, raw_chunk: &str) {
    let sink = JOB_STREAM_CHUNK_SINKS
        .lock()
        .ok()
        .and_then(|sinks| sinks.get(job_id).cloned());

    let Some(callback) = sink else {
        return;
    };

    let payload = raw_chunk.trim();
    if payload.is_empty() {
        return;
    }

    callback(payload.to_string());
}

pub(crate) fn emit_job_progress_update(
    job_id: &str,
    event_type: &str,
    payload: Option<Value>,
    session_id: &str,
    run_id: &str,
) {
    let now = Utc::now().to_rfc3339();
    emit_progress_stream_chunk(
        job_id,
        event_type,
        payload,
        session_id,
        run_id,
        now.as_str(),
    );
}

pub(crate) fn create_job(
    task: &str,
    agent_id: Option<String>,
    command_id: Option<String>,
    payload: Option<Value>,
    session_id: &str,
    run_id: &str,
) -> JobRecord {
    let now = Utc::now().to_rfc3339();
    let record = JobRecord {
        id: generate_id("job"),
        status: "queued".to_string(),
        task: task.to_string(),
        agent_id,
        command_id,
        payload_json: payload.map(|value| value.to_string()),
        result_json: None,
        error: None,
        created_at: now.clone(),
        updated_at: now,
        session_id: session_id.to_string(),
        run_id: run_id.to_string(),
    };

    if let Ok(mut jobs) = JOBS.lock() {
        jobs.insert(record.id.clone(), record.clone());
    }

    trace_router_node(
        "job",
        "create",
        Some(record.id.as_str()),
        Some(record.session_id.as_str()),
        Some(record.run_id.as_str()),
        Some(json!({
            "status": record.status.clone(),
            "task": truncate_for_event(record.task.as_str(), 2_000),
            "agent_id": record.agent_id.clone(),
            "command_id": record.command_id.clone(),
        })),
    );

    record
}

pub(crate) fn update_job_status(
    job_id: &str,
    status: &str,
    result_json: Option<String>,
    error: Option<String>,
) -> Option<JobRecord> {
    let mut jobs = JOBS.lock().ok()?;
    let job = jobs.get_mut(job_id)?;
    job.status = status.to_string();
    job.result_json = result_json;
    job.error = error;
    job.updated_at = Utc::now().to_rfc3339();
    let snapshot = job.clone();
    trace_router_node(
        "job",
        "status_update",
        Some(snapshot.id.as_str()),
        Some(snapshot.session_id.as_str()),
        Some(snapshot.run_id.as_str()),
        Some(json!({
            "status": snapshot.status.clone(),
            "error": snapshot.error.clone(),
            "has_result": snapshot.result_json.as_ref().map(|value| !value.trim().is_empty()).unwrap_or(false),
        })),
    );
    Some(snapshot)
}

pub(crate) fn append_job_event(
    job_id: &str,
    event_type: &str,
    payload: Option<Value>,
    session_id: &str,
    run_id: &str,
) {
    let payload_for_trace = payload.clone();
    let payload_for_stream = payload.clone();
    let event = JobEvent {
        id: generate_id("event"),
        job_id: job_id.to_string(),
        r#type: event_type.to_string(),
        payload_json: payload.map(|value| value.to_string()),
        created_at: Utc::now().to_rfc3339(),
        session_id: session_id.to_string(),
        run_id: run_id.to_string(),
    };
    let created_at = event.created_at.clone();

    if let Ok(mut events) = JOB_EVENTS.lock() {
        events
            .entry(job_id.to_string())
            .or_insert_with(Vec::new)
            .push(event);
    }

    trace_router_node(
        "job_event",
        event_type,
        Some(job_id),
        Some(session_id),
        Some(run_id),
        payload_for_trace,
    );

    emit_progress_stream_chunk(
        job_id,
        event_type,
        payload_for_stream,
        session_id,
        run_id,
        created_at.as_str(),
    );
}

pub(crate) fn list_job_events(job_id: &str) -> Vec<JobEvent> {
    JOB_EVENTS
        .lock()
        .ok()
        .and_then(|events| events.get(job_id).cloned())
        .unwrap_or_default()
}

pub(crate) fn set_cancel_flag(job_id: &str, flag: Arc<AtomicBool>) {
    if let Ok(mut flags) = JOB_CANCEL_FLAGS.lock() {
        flags.insert(job_id.to_string(), flag);
    }
}

pub(crate) fn remove_cancel_flag(job_id: &str) {
    if let Ok(mut flags) = JOB_CANCEL_FLAGS.lock() {
        flags.remove(job_id);
    }
}

pub(crate) fn get_cancel_flag(job_id: &str) -> Option<Arc<AtomicBool>> {
    JOB_CANCEL_FLAGS
        .lock()
        .ok()
        .and_then(|flags| flags.get(job_id).cloned())
}
