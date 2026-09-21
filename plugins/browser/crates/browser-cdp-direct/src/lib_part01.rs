impl DirectCdpBackend {
    async fn find_page(&self, target_id: &str) -> CoreResult<Page> {
        let pages = {
            let state = self.state.lock().await;
            state
                .browser
                .as_ref()
                .ok_or_else(|| CoreError::InvalidRequest("browser is not open".into()))?
                .pages()
                .await
                .map_err(|error| CoreError::Backend(error.to_string()))?
        };
        pages
            .into_iter()
            .find(|page| page.target_id().as_ref() == target_id)
            .ok_or_else(|| CoreError::NotFound(format!("backend target {target_id}")))
    }
}

impl BoundedEventQueue {
    async fn push(&self, method: String, mut params: Value) {
        redact_sensitive_json(&mut params);
        params = truncate_serializable(&params, MAX_SINGLE_EVENT_CHARS);
        let size = method.len() + serde_json::to_vec(&params).map_or(0, |bytes| bytes.len());
        let mut state = self.state.lock().await;
        state.latest_sequence = state.latest_sequence.wrapping_add(1);
        let sequence = state.latest_sequence;
        while state.events.len() >= MAX_EVENT_COUNT
            || state.total_bytes.saturating_add(size) > MAX_EVENT_BYTES
        {
            let Some(dropped) = state.events.pop_front() else {
                break;
            };
            state.total_bytes = state.total_bytes.saturating_sub(dropped.size);
            state.dropped_event_count = state.dropped_event_count.saturating_add(1);
        }
        state.events.push_back(QueuedEvent {
            event: CdpEvent {
                sequence,
                method,
                params,
            },
            size,
        });
        state.total_bytes = state.total_bytes.saturating_add(size);
        drop(state);
        self.notify.notify_waiters();
    }

    async fn poll(&self, after_sequence: u64, max_events: usize, wait: Duration) -> EventBatch {
        let notified = self.notify.notified();
        {
            let state = self.state.lock().await;
            let batch = state.batch(after_sequence, max_events);
            if !batch.events.is_empty() || wait.is_zero() {
                return batch;
            }
        }
        let _ = tokio::time::timeout(wait, notified).await;
        self.state.lock().await.batch(after_sequence, max_events)
    }
}

impl EventQueueState {
    fn batch(&self, after_sequence: u64, max_events: usize) -> EventBatch {
        EventBatch {
            events: self
                .events
                .iter()
                .filter(|queued| queued.event.sequence > after_sequence)
                .take(max_events)
                .map(|queued| queued.event.clone())
                .collect(),
            dropped_event_count: self.dropped_event_count,
            latest_sequence: self.latest_sequence,
        }
    }
}

async fn execute_page_command(page: &Page, method: &str, params: Value) -> CoreResult<Value> {
    page.execute(RawCommand {
        method: method.to_owned(),
        params,
    })
    .await
    .map(|response| response.result)
    .map_err(|error| CoreError::Backend(error.to_string()))
}

async fn spawn_event_listener<T>(
    page: &Page,
    method: &str,
    queue: Arc<BoundedEventQueue>,
) -> CoreResult<JoinHandle<()>>
where
    T: IntoEventKind + Serialize + Unpin + Send + Sync + 'static,
{
    let mut events = page
        .event_listener::<T>()
        .await
        .map_err(|error| CoreError::Backend(error.to_string()))?;
    let method = method.to_owned();
    Ok(tokio::spawn(async move {
        while let Some(event) = events.next().await {
            if let Ok(params) = serde_json::to_value(&*event) {
                queue.push(method.clone(), params).await;
            }
        }
    }))
}

async fn spawn_browser_event_listener<T>(
    browser: &Browser,
    method: &str,
    queue: Arc<BoundedEventQueue>,
) -> CoreResult<JoinHandle<()>>
where
    T: IntoEventKind + Serialize + Unpin + Send + Sync + 'static,
{
    let mut events = browser
        .event_listener::<T>()
        .await
        .map_err(|error| CoreError::Backend(error.to_string()))?;
    let method = method.to_owned();
    Ok(tokio::spawn(async move {
        while let Some(event) = events.next().await {
            if let Ok(params) = serde_json::to_value(&*event) {
                queue.push(method.clone(), params).await;
            }
        }
    }))
}

async fn spawn_route_worker(
    page: Page,
    rules: Arc<RwLock<Vec<BackendRoute>>>,
) -> CoreResult<JoinHandle<()>> {
    let mut events = page
        .event_listener::<EventRequestPaused>()
        .await
        .map_err(|error| CoreError::Backend(error.to_string()))?;
    Ok(tokio::spawn(async move {
        while let Some(event) = events.next().await {
            let matched = rules
                .read()
                .await
                .iter()
                .rev()
                .find(|route| wildcard_match(&route.rule.url_pattern, &event.request.url))
                .cloned();
            let request_id = event.request_id.as_ref();
            let result = match matched.map(|route| route.rule.action) {
                Some(RouteAction::Abort) => {
                    execute_page_command(
                        &page,
                        "Fetch.failRequest",
                        serde_json::json!({
                            "requestId": request_id,
                            "errorReason": "BlockedByClient"
                        }),
                    )
                    .await
                }
                Some(RouteAction::MockJson { status, body }) => {
                    let body = serde_json::to_vec(&body).unwrap_or_else(|_| b"null".to_vec());
                    execute_page_command(
                        &page,
                        "Fetch.fulfillRequest",
                        serde_json::json!({
                            "requestId": request_id,
                            "responseCode": status,
                            "responseHeaders": [
                                { "name": "Content-Type", "value": "application/json; charset=utf-8" },
                                { "name": "Cache-Control", "value": "no-store" }
                            ],
                            "body": BASE64.encode(body)
                        }),
                    )
                    .await
                }
                None => {
                    execute_page_command(
                        &page,
                        "Fetch.continueRequest",
                        serde_json::json!({ "requestId": request_id }),
                    )
                    .await
                }
            };
            if let Err(error) = result {
                tracing::warn!(%error, "failed to resolve an intercepted browser request");
            }
        }
    }))
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let parts = pattern.split('*').collect::<Vec<_>>();
    let mut position = 0;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let Some(offset) = value[position..].find(part) else {
            return false;
        };
        if index == 0 && !pattern.starts_with('*') && offset != 0 {
            return false;
        }
        position += offset + part.len();
    }
    pattern.ends_with('*') || parts.last().is_none_or(|part| value.ends_with(part))
}

async fn page_descriptor(page: &Page) -> TargetDescriptor {
    TargetDescriptor {
        id: page.target_id().as_ref().to_owned(),
        title: page.get_title().await.ok().flatten(),
        url: page.url().await.ok().flatten(),
        kind: "page".into(),
    }
}

#[derive(Debug)]
struct RawCommand {
    method: String,
    params: Value,
}

impl serde::Serialize for RawCommand {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.params.serialize(serializer)
    }
}

impl Method for RawCommand {
    fn identifier(&self) -> MethodId {
        Cow::Owned(self.method.clone())
    }
}

impl Command for RawCommand {
    type Response = Value;
}

#[cfg(test)]
include!("lib_inline_tests.rs");
