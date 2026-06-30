use super::*;

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
    let seen_event_ids = match state.run_service.list_run_events(&id).await {
        Ok(events) => events
            .into_iter()
            .map(|event| event.id)
            .collect::<HashSet<_>>(),
        Err(err) => {
            tracing::warn!(
                "failed to initialize run event stream cache for {}: {}",
                id,
                err
            );
            HashSet::new()
        }
    };
    let stream = stream::unfold(
        RunEventStreamState {
            run_id: id,
            run_service: state.run_service.clone(),
            receiver: state.run_service.subscribe_run_events(),
            seen_event_ids,
            pending_events: VecDeque::new(),
            receiver_closed: false,
        },
        |mut stream_state| async move {
            loop {
                if let Some(event) = stream_state.pending_events.pop_front() {
                    return Some((Ok(run_event_sse_event(&event)), stream_state));
                }

                if stream_state.receiver_closed {
                    tokio::time::sleep(RUN_EVENT_POLL_INTERVAL).await;
                } else {
                    tokio::select! {
                        received = stream_state.receiver.recv() => {
                            match received {
                                Ok(event) => {
                                    if event.run_id == stream_state.run_id
                                        && stream_state.seen_event_ids.insert(event.id.clone())
                                    {
                                        return Some((Ok(run_event_sse_event(&event)), stream_state));
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
                    .list_run_events(&stream_state.run_id)
                    .await
                {
                    Ok(events) => {
                        let mut unseen = events
                            .into_iter()
                            .filter(|event| stream_state.seen_event_ids.insert(event.id.clone()))
                            .collect::<Vec<_>>();
                        unseen.sort_by(|left, right| {
                            left.created_at
                                .cmp(&right.created_at)
                                .then(left.id.cmp(&right.id))
                        });
                        stream_state.pending_events.extend(unseen);
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
    seen_event_ids: HashSet<String>,
    pending_events: VecDeque<TaskRunEventRecord>,
    receiver_closed: bool,
}

fn run_event_sse_event(event: &TaskRunEventRecord) -> Event {
    Event::default()
        .event("run_event")
        .data(serde_json::to_string(event).unwrap_or_else(|_| "{}".to_string()))
}
