// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

const RUN_EVENT_RESYNC_BATCH_SIZE: usize = 200;

pub(in crate::api) async fn stream_run_events(
    Path(id): Path<String>,
    Query(query): Query<RunEventStreamQuery>,
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
    // Subscribe before reading the cursor so an event cannot fall between the DB read and MQ feed.
    let receiver = state.run_service.subscribe_run_events();
    let resync_receiver = state.run_event_resync_sender.subscribe();
    let (last_seen_cursor, resync_required) =
        match (query.after_created_at, query.after_id, query.from_start) {
            (Some(created_at), Some(event_id), false) => (Some((created_at, event_id)), true),
            (None, None, true) => (None, true),
            (None, None, false) => (
                state
                    .run_service
                    .latest_run_event_cursor(&id)
                    .await
                    .map_err(ApiError::internal)?,
                false,
            ),
            _ => {
                return Err(ApiError::bad_request(
                    "Run event stream cursor is invalid".to_string(),
                ))
            }
        };
    let stream_lease = state.runtime_stats.acquire_run_event_stream();
    let stream = stream::unfold(
        RunEventStreamState {
            run_id: id,
            run_service: state.run_service.clone(),
            receiver,
            resync_receiver,
            pending_events: VecDeque::new(),
            last_event_created_at: last_seen_cursor.as_ref().map(|cursor| cursor.0.clone()),
            last_event_id: last_seen_cursor.map(|cursor| cursor.1),
            resync_required,
            _stream_lease: stream_lease,
        },
        |mut stream_state| async move {
            loop {
                if let Some(event) = stream_state.pending_events.pop_front() {
                    stream_state.update_cursor(&event);
                    return Some((Ok(run_event_sse_event(&event)), stream_state));
                }

                if stream_state.resync_required {
                    match stream_state
                        .run_service
                        .list_run_events_after(
                            &stream_state.run_id,
                            stream_state.last_event_created_at.as_deref(),
                            stream_state.last_event_id.as_deref(),
                            RUN_EVENT_RESYNC_BATCH_SIZE,
                        )
                        .await
                    {
                        Ok(events) => {
                            stream_state.resync_required =
                                events.len() == RUN_EVENT_RESYNC_BATCH_SIZE;
                            stream_state.pending_events.extend(events);
                            continue;
                        }
                        Err(err) => {
                            tracing::warn!(
                                run_id = stream_state.run_id.as_str(),
                                error = err.as_str(),
                                "failed to resynchronize Run event stream"
                            );
                            return None;
                        }
                    }
                }

                tokio::select! {
                    received = stream_state.receiver.recv() => {
                        match received {
                            Ok(event) => {
                                if event.run_id == stream_state.run_id
                                    && stream_state.event_is_after_cursor(&event)
                                {
                                    stream_state.update_cursor(&event);
                                    return Some((
                                        Ok(run_event_sse_event(&event)),
                                        stream_state,
                                    ));
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                stream_state.resync_required = true;
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                        }
                    }
                    resync = stream_state.resync_receiver.recv() => {
                        match resync {
                            Ok(()) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                stream_state.resync_required = true;
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                        }
                    }
                }
            }
        },
    );
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct RunEventStreamQuery {
    after_created_at: Option<String>,
    after_id: Option<String>,
    #[serde(default)]
    from_start: bool,
}

struct RunEventStreamState {
    run_id: String,
    run_service: crate::services::RunService,
    receiver: tokio::sync::broadcast::Receiver<TaskRunEventRecord>,
    resync_receiver: tokio::sync::broadcast::Receiver<()>,
    pending_events: VecDeque<TaskRunEventRecord>,
    last_event_created_at: Option<String>,
    last_event_id: Option<String>,
    resync_required: bool,
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

fn run_event_sse_event(event: &TaskRunEventRecord) -> Event {
    Event::default()
        .event("run_event")
        .data(run_event_sse_payload(event))
}

fn run_event_sse_payload(event: &TaskRunEventRecord) -> String {
    serde_json::json!({
        "id": event.id,
        "run_id": event.run_id,
        "event_type": event.event_type,
        "created_at": event.created_at,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_notification_excludes_large_event_content() {
        let event = TaskRunEventRecord {
            id: "event-1".to_string(),
            run_id: "run-1".to_string(),
            event_type: "tool.output".to_string(),
            message: Some("large message".repeat(1_000)),
            payload: Some(serde_json::json!({ "content": "large payload".repeat(1_000) })),
            created_at: "2026-08-05T12:00:00Z".to_string(),
        };

        let payload: serde_json::Value =
            serde_json::from_str(run_event_sse_payload(&event).as_str())
                .expect("decode SSE notification");

        assert_eq!(payload["id"], "event-1");
        assert_eq!(payload["run_id"], "run-1");
        assert!(payload.get("message").is_none());
        assert!(payload.get("payload").is_none());
    }
}
