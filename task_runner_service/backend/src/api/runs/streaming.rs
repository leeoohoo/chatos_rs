// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

const RUN_EVENT_POLL_BATCH_SIZE: usize = 200;

pub(in crate::api) async fn stream_run_events(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>>, ApiError>
{
    let run = state
        .run_service
        .get_run(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))?;
    ensure_run_access(&state, &run, &current_user).await?;
    let last_seen_cursor = match state.run_service.list_run_events(&id).await {
        Ok(events) => events
            .last()
            .map(|event| (event.created_at.clone(), event.id.clone())),
        Err(err) => {
            tracing::warn!(
                "failed to initialize run event stream cursor for {}: {}",
                id,
                err
            );
            None
        }
    };
    let stream_lease = state.runtime_stats.acquire_run_event_stream();
    let stream = stream::unfold(
        RunEventStreamState {
            run_id: id,
            run_service: state.run_service.clone(),
            receiver: state.run_service.subscribe_run_events(),
            pending_events: VecDeque::new(),
            last_event_created_at: last_seen_cursor.as_ref().map(|cursor| cursor.0.clone()),
            last_event_id: last_seen_cursor.map(|cursor| cursor.1),
            receiver_closed: false,
            path_redactor:
                crate::services::path_redaction::WorkspacePathRedactor::for_workspace_base(
                    state.config.default_workspace_dir.as_str(),
                ),
            _stream_lease: stream_lease,
        },
        |mut stream_state| async move {
            loop {
                if let Some(event) = stream_state.pending_events.pop_front() {
                    stream_state.update_cursor(&event);
                    return Some((
                        Ok(run_event_sse_event(&event, &stream_state.path_redactor)),
                        stream_state,
                    ));
                }

                if stream_state.receiver_closed {
                    tokio::time::sleep(RUN_EVENT_POLL_INTERVAL).await;
                } else {
                    tokio::select! {
                        received = stream_state.receiver.recv() => {
                            match received {
                                Ok(event) => {
                                    if event.run_id == stream_state.run_id
                                        && stream_state.event_is_after_cursor(&event)
                                    {
                                        stream_state.update_cursor(&event);
                                        return Some((
                                            Ok(run_event_sse_event(&event, &stream_state.path_redactor)),
                                            stream_state,
                                        ));
                                    }
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                    stream_state.receiver_closed = true;
                                }
                            }
                        }
                        _ = tokio::time::sleep(RUN_EVENT_POLL_INTERVAL) => {}
                    }
                }

                match stream_state
                    .run_service
                    .list_run_events_after(
                        &stream_state.run_id,
                        stream_state.last_event_created_at.as_deref(),
                        stream_state.last_event_id.as_deref(),
                        RUN_EVENT_POLL_BATCH_SIZE,
                    )
                    .await
                {
                    Ok(events) => {
                        stream_state.pending_events.extend(events);
                    }
                    Err(err) => {
                        tracing::warn!(
                            "failed to poll run events for {}: {}",
                            stream_state.run_id,
                            err
                        );
                    }
                }
            }
        },
    );
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

struct RunEventStreamState {
    run_id: String,
    run_service: crate::services::RunService,
    receiver: tokio::sync::broadcast::Receiver<TaskRunEventRecord>,
    pending_events: VecDeque<TaskRunEventRecord>,
    last_event_created_at: Option<String>,
    last_event_id: Option<String>,
    receiver_closed: bool,
    path_redactor: crate::services::path_redaction::WorkspacePathRedactor,
    _stream_lease: crate::state::ActiveRunEventStreamLease,
}

impl RunEventStreamState {
    fn event_is_after_cursor(&self, event: &TaskRunEventRecord) -> bool {
        match (
            self.last_event_created_at.as_deref(),
            self.last_event_id.as_deref(),
        ) {
            (Some(created_at), Some(id)) => {
                event.created_at.as_str() > created_at
                    || (event.created_at.as_str() == created_at && event.id.as_str() > id)
            }
            _ => true,
        }
    }

    fn update_cursor(&mut self, event: &TaskRunEventRecord) {
        self.last_event_created_at = Some(event.created_at.clone());
        self.last_event_id = Some(event.id.clone());
    }
}

fn run_event_sse_event(
    event: &TaskRunEventRecord,
    redactor: &crate::services::path_redaction::WorkspacePathRedactor,
) -> Event {
    let mut event = event.clone();
    if let Some(message) = event.message.as_mut() {
        *message = redactor.redact_text(message);
    }
    if let Some(payload) = event.payload.as_mut() {
        redactor.redact_value(payload);
    }
    Event::default()
        .event("run_event")
        .data(serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string()))
}
