use super::super::super::*;

const STREAM_BUFFER_MAX_CHARS: usize = 24_000;

pub(super) struct AiStreamCallbacks {
    pub chunk_buffer: Arc<Mutex<String>>,
    pub thinking_buffer: Arc<Mutex<String>>,
    pub on_chunk: Arc<dyn Fn(String) + Send + Sync>,
    pub on_thinking: Arc<dyn Fn(String) + Send + Sync>,
    pub on_tools_start: Arc<dyn Fn(Value) + Send + Sync>,
    pub on_tools_stream: Arc<dyn Fn(Value) + Send + Sync>,
    pub on_tools_end: Arc<dyn Fn(Value) + Send + Sync>,
}

pub(super) fn create_ai_stream_callbacks(
    job_id: &str,
    session_id: &str,
    run_id: &str,
) -> AiStreamCallbacks {
    let chunk_buffer = Arc::new(Mutex::new(String::new()));
    let thinking_buffer = Arc::new(Mutex::new(String::new()));

    let on_chunk = {
        let chunk_buffer = chunk_buffer.clone();
        let job_id = job_id.to_string();
        let session_id = session_id.to_string();
        let run_id = run_id.to_string();
        Arc::new(move |chunk: String| {
            if chunk.trim().is_empty() {
                return;
            }
            append_to_capped_buffer(&chunk_buffer, chunk.as_str(), STREAM_BUFFER_MAX_CHARS);
            emit_job_progress_update(
                job_id.as_str(),
                "ai_content_stream",
                Some(json!({ "chunk": chunk })),
                session_id.as_str(),
                run_id.as_str(),
            );
        }) as Arc<dyn Fn(String) + Send + Sync>
    };

    let on_thinking = {
        let thinking_buffer = thinking_buffer.clone();
        let job_id = job_id.to_string();
        let session_id = session_id.to_string();
        let run_id = run_id.to_string();
        Arc::new(move |chunk: String| {
            if chunk.trim().is_empty() {
                return;
            }
            append_to_capped_buffer(&thinking_buffer, chunk.as_str(), STREAM_BUFFER_MAX_CHARS);
            emit_job_progress_update(
                job_id.as_str(),
                "ai_reasoning_stream",
                Some(json!({ "chunk": chunk })),
                session_id.as_str(),
                run_id.as_str(),
            );
        }) as Arc<dyn Fn(String) + Send + Sync>
    };

    let on_tools_start = {
        let job_id = job_id.to_string();
        let session_id = session_id.to_string();
        let run_id = run_id.to_string();
        Arc::new(move |tool_calls: Value| {
            append_job_event(
                job_id.as_str(),
                "ai_tools_start",
                Some(json!({
                    "tool_calls": summarize_tool_calls_for_event(&tool_calls),
                })),
                session_id.as_str(),
                run_id.as_str(),
            );
        }) as Arc<dyn Fn(Value) + Send + Sync>
    };

    let on_tools_stream = {
        let job_id = job_id.to_string();
        let session_id = session_id.to_string();
        let run_id = run_id.to_string();
        Arc::new(move |result: Value| {
            append_job_event(
                job_id.as_str(),
                "ai_tools_stream",
                Some(summarize_single_tool_result_for_event(&result)),
                session_id.as_str(),
                run_id.as_str(),
            );

            if let Some(raw_chunk) = extract_task_review_event_stream_chunk(&result) {
                // Re-emit task review events as raw tool chunks so the top-level chat UI can open the review panel.
                emit_job_raw_stream_chunk(job_id.as_str(), raw_chunk.as_str());
            }
        }) as Arc<dyn Fn(Value) + Send + Sync>
    };

    let on_tools_end = {
        let job_id = job_id.to_string();
        let session_id = session_id.to_string();
        let run_id = run_id.to_string();
        Arc::new(move |result: Value| {
            append_job_event(
                job_id.as_str(),
                "ai_tools_end",
                Some(summarize_tool_results_for_event(&result)),
                session_id.as_str(),
                run_id.as_str(),
            );
        }) as Arc<dyn Fn(Value) + Send + Sync>
    };

    AiStreamCallbacks {
        chunk_buffer,
        thinking_buffer,
        on_chunk,
        on_thinking,
        on_tools_start,
        on_tools_stream,
        on_tools_end,
    }
}

fn append_to_capped_buffer(buffer: &Arc<Mutex<String>>, chunk: &str, max_chars: usize) {
    if let Ok(mut guard) = buffer.lock() {
        guard.push_str(chunk);
        if guard.chars().count() > max_chars {
            let trimmed = guard
                .chars()
                .rev()
                .take(max_chars)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>();
            *guard = trimmed;
        }
    }
}

fn extract_task_review_event_stream_chunk(result: &Value) -> Option<String> {
    let raw_content = result
        .get("content")
        .and_then(|value| value.as_str())?
        .trim();
    if raw_content.is_empty() {
        return None;
    }

    let parsed: Value = serde_json::from_str(raw_content).ok()?;
    let event_name = parsed.get("event").and_then(|value| value.as_str())?;

    if matches!(
        event_name,
        crate::utils::events::Events::TASK_CREATE_REVIEW_REQUIRED
            | crate::utils::events::Events::TASK_CREATE_REVIEW_RESOLVED
    ) {
        Some(raw_content.to_string())
    } else {
        None
    }
}
