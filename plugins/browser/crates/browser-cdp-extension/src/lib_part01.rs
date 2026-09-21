impl BridgeClient {
    async fn connect(
        config: &BridgeConfig,
    ) -> CoreResult<(
        Self,
        BridgeHello,
        mpsc::Receiver<BridgeEvent>,
        JoinHandle<()>,
    )> {
        let mut request = config
            .endpoint
            .as_str()
            .into_client_request()
            .map_err(|_| CoreError::InvalidRequest("Browser Bridge endpoint is invalid".into()))?;
        request.headers_mut().insert(
            SEC_WEBSOCKET_PROTOCOL,
            HeaderValue::from_static(BRIDGE_SUBPROTOCOL),
        );
        let websocket_config = WebSocketConfig::default()
            .max_message_size(Some(MAX_BRIDGE_MESSAGE_BYTES))
            .max_frame_size(Some(MAX_BRIDGE_MESSAGE_BYTES));
        let (mut socket, response) = tokio::time::timeout(
            CONNECT_TIMEOUT,
            connect_async_with_config(request, Some(websocket_config)),
        )
        .await
        .map_err(|_| CoreError::Timeout("Browser Bridge connection".into()))?
        .map_err(|_| CoreError::Backend("could not connect to Browser Bridge".into()))?;
        if response
            .headers()
            .get(SEC_WEBSOCKET_PROTOCOL)
            .and_then(|value| value.to_str().ok())
            != Some(BRIDGE_SUBPROTOCOL)
        {
            let _ = socket.close(None).await;
            return Err(CoreError::Backend(
                "Browser Bridge did not negotiate the required protocol".into(),
            ));
        }

        let authentication = json!({
            "type": "request",
            "id": 1,
            "method": "bridge.authenticate",
            "params": {
                "protocol_version": BRIDGE_PROTOCOL_VERSION,
                "token": config.credential.token,
                "client": {
                    "name": browser_cdp_protocol::SERVER_NAME,
                    "version": browser_cdp_protocol::SERVER_VERSION
                }
            }
        });
        let authentication = serde_json::to_string(&authentication)
            .map_err(|_| CoreError::Backend("could not encode Bridge authentication".into()))?;
        socket
            .send(Message::text(authentication))
            .await
            .map_err(|_| CoreError::Backend("could not authenticate with Browser Bridge".into()))?;
        let frame = tokio::time::timeout(CONNECT_TIMEOUT, socket.next())
            .await
            .map_err(|_| CoreError::Timeout("Browser Bridge authentication".into()))?
            .ok_or_else(|| {
                CoreError::Backend("Browser Bridge closed during authentication".into())
            })?
            .map_err(|_| CoreError::Backend("Browser Bridge authentication failed".into()))?;
        let hello = parse_authentication_response(frame)?;

        let (outbound_tx, outbound_rx) = mpsc::channel(OUTBOUND_QUEUE_CAPACITY);
        let (event_tx, event_rx) = mpsc::channel(EVENT_DISPATCH_CAPACITY);
        let disconnected = Arc::new(AtomicBool::new(false));
        let connection_disconnected = disconnected.clone();
        let connection_task = tokio::spawn(async move {
            run_connection(socket, outbound_rx, event_tx, connection_disconnected).await;
        });
        Ok((
            Self {
                outbound: outbound_tx,
                next_id: Arc::new(AtomicU64::new(2)),
                disconnected,
            },
            hello,
            event_rx,
            connection_task,
        ))
    }

    async fn request(&self, method: &str, params: Value, timeout: Duration) -> CoreResult<Value> {
        if self.disconnected.load(Ordering::Acquire) {
            return Err(CoreError::Backend(
                "Browser Bridge connection is unavailable".into(),
            ));
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (response_tx, response_rx) = oneshot::channel();
        self.outbound
            .send(Outbound::Request {
                id,
                method: method.to_owned(),
                params,
                response: response_tx,
            })
            .await
            .map_err(|_| CoreError::Backend("Browser Bridge connection is unavailable".into()))?;
        match tokio::time::timeout(timeout, response_rx).await {
            Ok(Ok(result)) => result.map_err(BridgeFailure::into_core_error),
            Ok(Err(_)) => Err(CoreError::Backend(
                "Browser Bridge connection is unavailable".into(),
            )),
            Err(_) => {
                let _ = self.outbound.try_send(Outbound::Cancel { id });
                Err(CoreError::Timeout(method.to_owned()))
            }
        }
    }

    async fn close(&self) {
        let _ = self.outbound.send(Outbound::Close).await;
    }
}

enum Outbound {
    Request {
        id: u64,
        method: String,
        params: Value,
        response: oneshot::Sender<Result<Value, BridgeFailure>>,
    },
    Cancel {
        id: u64,
    },
    Close,
}

#[derive(Debug)]
enum BridgeFailure {
    Remote(BridgeRemoteError),
    Connection(String),
    InvalidRequest(String),
}

impl BridgeFailure {
    fn into_core_error(self) -> CoreError {
        match self {
            Self::Remote(error) => match error.code.as_str() {
                "unsupported_by_backend" => CoreError::Unsupported(error.safe_message()),
                "invalid_request" => CoreError::InvalidRequest(error.safe_message()),
                "not_found" => CoreError::NotFound(error.safe_message()),
                "timeout" => CoreError::Timeout(error.safe_message()),
                "permission_denied" | "token_expired" | "extension_unavailable" => {
                    CoreError::Backend(format!(
                        "Browser Bridge rejected the operation: {}",
                        error.code
                    ))
                }
                _ => CoreError::Backend(error.safe_message()),
            },
            Self::Connection(message) => CoreError::Backend(message),
            Self::InvalidRequest(message) => CoreError::InvalidRequest(message),
        }
    }
}

#[derive(Debug, Deserialize)]
struct BridgeRemoteError {
    code: String,
    #[serde(default)]
    message: String,
}

impl BridgeRemoteError {
    fn safe_message(&self) -> String {
        let message = self.message.trim();
        if message.is_empty() {
            return self.code.chars().take(128).collect();
        }
        message.chars().take(1024).collect()
    }
}

#[derive(Deserialize)]
struct BridgeHello {
    protocol_version: String,
    #[serde(rename = "connection_id")]
    _connection_id: String,
    product: String,
    user_agent: String,
    #[serde(default)]
    capabilities: Vec<String>,
}

#[derive(Deserialize)]
struct WireMessage {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<BridgeRemoteError>,
    #[serde(default)]
    params: Value,
}

fn parse_authentication_response(frame: Message) -> CoreResult<BridgeHello> {
    let Message::Text(text) = frame else {
        return Err(CoreError::Backend(
            "Browser Bridge authentication response was invalid".into(),
        ));
    };
    let response: WireMessage = serde_json::from_str(&text).map_err(|_| {
        CoreError::Backend("Browser Bridge authentication response was invalid".into())
    })?;
    if response.kind != "response" || response.id != Some(1) {
        return Err(CoreError::Backend(
            "Browser Bridge authentication response was invalid".into(),
        ));
    }
    if let Some(error) = response.error {
        return Err(match error.code.as_str() {
            "extension_unavailable" => CoreError::Backend(format!(
                "{}. Start a Browser CDP task, then click First connect in the Chatos Browser Bridge extension.",
                error.safe_message()
            )),
            "invalid_request" => CoreError::Backend(error.safe_message()),
            _ => CoreError::Backend("Browser Bridge authentication failed".into()),
        });
    }
    let hello: BridgeHello = serde_json::from_value(response.result.unwrap_or(Value::Null))
        .map_err(|_| {
            CoreError::Backend("Browser Bridge authentication response was invalid".into())
        })?;
    if hello.protocol_version != BRIDGE_PROTOCOL_VERSION {
        return Err(CoreError::Unsupported(format!(
            "Browser Bridge protocol {} is not supported",
            hello.protocol_version
        )));
    }
    if hello._connection_id.is_empty()
        || hello._connection_id.len() > 256
        || hello.product.is_empty()
        || hello.product.len() > 512
        || hello.user_agent.len() > 4096
        || hello.capabilities.len() > 128
        || hello
            .capabilities
            .iter()
            .any(|capability| capability.is_empty() || capability.len() > 128)
    {
        return Err(CoreError::Backend(
            "Browser Bridge authentication response exceeded protocol bounds".into(),
        ));
    }
    Ok(hello)
}

async fn run_connection<S>(
    mut socket: async_tungstenite::WebSocketStream<S>,
    mut outbound_rx: mpsc::Receiver<Outbound>,
    event_tx: mpsc::Sender<BridgeEvent>,
    disconnected: Arc<AtomicBool>,
) where
    S: futures::AsyncRead + futures::AsyncWrite + Unpin,
{
    let mut pending: HashMap<u64, oneshot::Sender<Result<Value, BridgeFailure>>> = HashMap::new();
    let disconnect_reason = loop {
        tokio::select! {
            outbound = outbound_rx.recv() => {
                match outbound {
                    Some(Outbound::Request { id, method, params, response }) => {
                        let message = json!({
                            "type": "request",
                            "id": id,
                            "method": method,
                            "params": params
                        });
                        let encoded = match serde_json::to_string(&message) {
                            Ok(encoded) if encoded.len() <= MAX_CLIENT_MESSAGE_BYTES => encoded,
                            Ok(_) => {
                                let _ = response.send(Err(BridgeFailure::InvalidRequest(
                                    "Browser Bridge command exceeds 1 MiB".into(),
                                )));
                                continue;
                            }
                            Err(_) => {
                                let _ = response.send(Err(BridgeFailure::InvalidRequest(
                                    "Browser Bridge command is not serializable".into(),
                                )));
                                continue;
                            }
                        };
                        pending.insert(id, response);
                        if socket.send(Message::text(encoded)).await.is_err() {
                            break "Browser Bridge connection was lost".to_owned();
                        }
                    }
                    Some(Outbound::Cancel { id }) => {
                        pending.remove(&id);
                    }
                    Some(Outbound::Close) | None => {
                        let _ = socket.close(None).await;
                        break "Browser Bridge connection is closed".to_owned();
                    }
                }
            }
            incoming = socket.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        let message: WireMessage = match serde_json::from_str(&text) {
                            Ok(message) => message,
                            Err(_) => break "Browser Bridge sent a malformed message".to_owned(),
                        };
                        match message.kind.as_str() {
                            "response" => {
                                let Some(id) = message.id else {
                                    break "Browser Bridge response omitted its ID".to_owned();
                                };
                                if let Some(response) = pending.remove(&id) {
                                    let result = match message.error {
                                        Some(error) => Err(BridgeFailure::Remote(error)),
                                        None => Ok(message.result.unwrap_or(Value::Null)),
                                    };
                                    let _ = response.send(result);
                                }
                            }
                            "event" => {
                                let method = message.method.unwrap_or_default();
                                if method == "bridge.disconnected" {
                                    let reason = message
                                        .params
                                        .get("reason")
                                        .and_then(Value::as_str)
                                        .unwrap_or("extension_unavailable");
                                    break format!(
                                        "Browser Bridge disconnected: {}",
                                        safe_reason(reason)
                                    );
                                }
                                if method == "cdp.event"
                                    && let Some(event) = BridgeEvent::from_params(message.params)
                                    && event_tx.send(event).await.is_err()
                                {
                                    break "Browser Bridge event dispatcher stopped".to_owned();
                                }
                            }
                            _ => break "Browser Bridge sent an invalid message type".to_owned(),
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break "Browser Bridge connection was lost".to_owned();
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) | None => {
                        break "Browser Bridge connection was closed".to_owned();
                    }
                    Some(Ok(Message::Binary(_) | Message::Frame(_))) => {
                        break "Browser Bridge sent a non-JSON message".to_owned();
                    }
                    Some(Err(_)) => break "Browser Bridge connection was lost".to_owned(),
                }
            }
        }
    };
    disconnected.store(true, Ordering::Release);
    for (_, response) in pending {
        let _ = response.send(Err(BridgeFailure::Connection(disconnect_reason.clone())));
    }
    let _ = event_tx
        .send(BridgeEvent::Disconnected {
            reason: disconnect_reason,
        })
        .await;
}

fn safe_reason(reason: &str) -> String {
    reason
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        .take(128)
        .collect()
}

enum BridgeEvent {
    Cdp {
        subscription_id: String,
        method: String,
        params: Value,
    },
    Disconnected {
        reason: String,
    },
}

impl BridgeEvent {
    fn from_params(params: Value) -> Option<Self> {
        let subscription_id = params.get("subscription_id")?.as_str()?.to_owned();
        let method = params.get("method")?.as_str()?.to_owned();
        let event_params = params.get("params").cloned().unwrap_or_else(|| json!({}));
        if subscription_id.is_empty() || method.is_empty() {
            return None;
        }
        Some(Self::Cdp {
            subscription_id,
            method,
            params: event_params,
        })
    }
}

fn spawn_event_dispatcher(
    state: Arc<Mutex<ExtensionState>>,
    mut event_rx: mpsc::Receiver<BridgeEvent>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                BridgeEvent::Cdp {
                    subscription_id,
                    method,
                    params,
                } => {
                    let queue = state
                        .lock()
                        .await
                        .subscriptions
                        .get(&subscription_id)
                        .map(|subscription| subscription.queue.clone());
                    if let Some(queue) = queue {
                        queue.push(method, params).await;
                    }
                }
                BridgeEvent::Disconnected { reason } => {
                    let queues = {
                        let mut state = state.lock().await;
                        state.disconnected = Some(reason.clone());
                        state
                            .subscriptions
                            .values()
                            .map(|subscription| subscription.queue.clone())
                            .collect::<Vec<_>>()
                    };
                    for queue in queues {
                        queue.close(&reason).await;
                    }
                    break;
                }
            }
        }
    })
}

struct QueuedEvent {
    event: CdpEvent,
    size: usize,
}

#[derive(Default)]
struct EventQueueState {
    events: VecDeque<QueuedEvent>,
    total_bytes: usize,
    latest_sequence: u64,
    dropped_event_count: u64,
    closed: Option<String>,
}

#[derive(Default)]
struct BoundedEventQueue {
    state: Mutex<EventQueueState>,
    notify: Notify,
}

impl BoundedEventQueue {
    async fn push(&self, method: String, mut params: Value) {
        redact_sensitive_json(&mut params);
        params = truncate_serializable(&params, MAX_SINGLE_EVENT_CHARS);
        let size = method.len() + serde_json::to_vec(&params).map_or(0, |bytes| bytes.len());
        let mut state = self.state.lock().await;
        if state.closed.is_some() {
            return;
        }
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

    async fn poll(
        &self,
        after_sequence: u64,
        max_events: usize,
        wait: Duration,
    ) -> CoreResult<EventBatch> {
        let notified = self.notify.notified();
        {
            let state = self.state.lock().await;
            if let Some(reason) = &state.closed {
                return Err(CoreError::Backend(reason.clone()));
            }
            let batch = state.batch(after_sequence, max_events);
            if !batch.events.is_empty() || wait.is_zero() {
                return Ok(batch);
            }
        }
        let _ = tokio::time::timeout(wait, notified).await;
        let state = self.state.lock().await;
        if let Some(reason) = &state.closed {
            return Err(CoreError::Backend(reason.clone()));
        }
        Ok(state.batch(after_sequence, max_events))
    }

    async fn close(&self, reason: &str) {
        self.state.lock().await.closed = Some(reason.to_owned());
        self.notify.notify_waiters();
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

#[cfg(test)]
include!("lib_inline_tests.rs");
