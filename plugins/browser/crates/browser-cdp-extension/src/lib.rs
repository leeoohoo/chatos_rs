use std::{
    collections::{HashMap, VecDeque},
    env,
    net::IpAddr,
    path::Path,
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use async_tungstenite::{
    tokio::connect_async_with_config,
    tungstenite::{
        Message,
        client::IntoClientRequest,
        http::{HeaderValue, header::SEC_WEBSOCKET_PROTOCOL},
        protocol::WebSocketConfig,
    },
};
use browser_cdp_core::{BrowserBackend, BrowserBackendFactory, CoreError, CoreResult};
use browser_cdp_policy::{redact_sensitive_json, truncate_serializable};
use browser_cdp_protocol::{
    BackendSessionId, BrowserDescriptor, BrowserMode, CdpEvent, EventBatch, EventFilter,
    OpenBrowserRequest, TargetDescriptor,
};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    sync::{Mutex, Notify, mpsc, oneshot},
    task::JoinHandle,
};
use url::Url;
use uuid::Uuid;

const BRIDGE_PROTOCOL_VERSION: &str = "1.0";
const BRIDGE_SUBPROTOCOL: &str = "chatos-browser-bridge.v1";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_CLIENT_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_BRIDGE_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_EVENT_COUNT: usize = 10_000;
const MAX_EVENT_BYTES: usize = 8 * 1024 * 1024;
const MAX_SINGLE_EVENT_CHARS: usize = 256 * 1024;
const OUTBOUND_QUEUE_CAPACITY: usize = 256;
const EVENT_DISPATCH_CAPACITY: usize = 1024;

pub struct ExtensionBackendFactory {
    fixed_config: Option<BridgeConfig>,
    credential_file_config: Option<(String, std::path::PathBuf)>,
}

impl ExtensionBackendFactory {
    pub fn from_environment() -> Self {
        Self {
            fixed_config: None,
            credential_file_config: None,
        }
    }

    pub fn from_credential_file(
        endpoint: impl Into<String>,
        credential_file: impl Into<std::path::PathBuf>,
    ) -> Self {
        Self {
            fixed_config: None,
            credential_file_config: Some((endpoint.into(), credential_file.into())),
        }
    }

    #[cfg(test)]
    fn with_config(config: BridgeConfig) -> Self {
        Self {
            fixed_config: Some(config),
            credential_file_config: None,
        }
    }
}

#[async_trait]
impl BrowserBackendFactory for ExtensionBackendFactory {
    fn supports(&self, mode: BrowserMode) -> bool {
        mode == BrowserMode::ChromeExtension
    }

    async fn create(&self, mode: BrowserMode) -> CoreResult<Arc<dyn BrowserBackend>> {
        if mode != BrowserMode::ChromeExtension {
            return Err(CoreError::Unsupported(
                "extension backend only supports chrome_extension mode".into(),
            ));
        }
        let config = match (&self.fixed_config, &self.credential_file_config) {
            (Some(config), _) => config.clone(),
            (_, Some((endpoint, credential_file))) => {
                BridgeConfig::new(endpoint, read_credential_file(credential_file).await?)?
            }
            _ => BridgeConfig::from_environment().await?,
        };
        Ok(Arc::new(ExtensionCdpBackend::new(config)))
    }
}

#[derive(Clone)]
struct BridgeConfig {
    endpoint: Url,
    credential: BridgeCredential,
}

#[derive(Clone)]
struct BridgeCredential {
    token: String,
}

#[derive(Deserialize)]
struct CredentialFile {
    token: String,
    expires_at_unix_ms: u64,
}

impl BridgeConfig {
    async fn from_environment() -> CoreResult<Self> {
        let endpoint = env::var("CHATOS_BROWSER_BRIDGE_ENDPOINT").map_err(|_| {
            CoreError::Unsupported("development Browser Bridge endpoint is unavailable".into())
        })?;
        let token = if let Some(path) = env::var_os("CHATOS_BROWSER_BRIDGE_CREDENTIAL_FILE") {
            read_credential_file(Path::new(&path)).await?
        } else {
            env::var("CHATOS_BROWSER_BRIDGE_TOKEN").map_err(|_| {
                CoreError::Unsupported(
                    "development Browser Bridge credential is unavailable".into(),
                )
            })?
        };
        Self::new(&endpoint, token)
    }

    fn new(endpoint: &str, token: String) -> CoreResult<Self> {
        let endpoint = validate_endpoint(endpoint)?;
        validate_token(&token)?;
        Ok(Self {
            endpoint,
            credential: BridgeCredential { token },
        })
    }
}

async fn read_credential_file(path: &Path) -> CoreResult<String> {
    let metadata = tokio::fs::symlink_metadata(path)
        .await
        .map_err(|_| CoreError::Unsupported("Browser Bridge credential is unavailable".into()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 64 * 1024 {
        return Err(CoreError::InvalidRequest(
            "Browser Bridge credential file is invalid".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(CoreError::InvalidRequest(
                "Browser Bridge credential file must not be accessible by group or other users"
                    .into(),
            ));
        }
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|_| CoreError::Unsupported("Browser Bridge credential is unavailable".into()))?;
    let credential: CredentialFile = serde_json::from_slice(&bytes).map_err(|_| {
        CoreError::InvalidRequest("Browser Bridge credential file is malformed".into())
    })?;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    if credential.expires_at_unix_ms <= now_ms {
        return Err(CoreError::Backend(
            "Browser Bridge credential has expired".into(),
        ));
    }
    validate_token(&credential.token)?;
    Ok(credential.token)
}

fn validate_endpoint(endpoint: &str) -> CoreResult<Url> {
    let endpoint = Url::parse(endpoint)
        .map_err(|_| CoreError::InvalidRequest("Browser Bridge endpoint is invalid".into()))?;
    if endpoint.scheme() != "ws" {
        return Err(CoreError::InvalidRequest(
            "Browser Bridge endpoint must use ws:// on loopback".into(),
        ));
    }
    if !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(CoreError::InvalidRequest(
            "Browser Bridge endpoint must not contain credentials, query parameters, or fragments"
                .into(),
        ));
    }
    let host = endpoint
        .host_str()
        .ok_or_else(|| CoreError::InvalidRequest("Browser Bridge endpoint has no host".into()))?;
    let address = IpAddr::from_str(host.trim_matches(['[', ']'])).map_err(|_| {
        CoreError::InvalidRequest(
            "Browser Bridge endpoint must use a numeric loopback address".into(),
        )
    })?;
    if !address.is_loopback() {
        return Err(CoreError::InvalidRequest(
            "Browser Bridge endpoint must be loopback-only".into(),
        ));
    }
    Ok(endpoint)
}

fn validate_token(token: &str) -> CoreResult<()> {
    if !(16..=4096).contains(&token.len()) || token.chars().any(char::is_whitespace) {
        return Err(CoreError::InvalidRequest(
            "Browser Bridge credential has an invalid format".into(),
        ));
    }
    Ok(())
}

pub struct ExtensionCdpBackend {
    config: BridgeConfig,
    state: Arc<Mutex<ExtensionState>>,
}

impl ExtensionCdpBackend {
    fn new(config: BridgeConfig) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(ExtensionState::default())),
        }
    }

    async fn client(&self) -> CoreResult<BridgeClient> {
        let state = self.state.lock().await;
        if let Some(reason) = &state.disconnected {
            return Err(CoreError::Backend(reason.clone()));
        }
        state
            .client
            .clone()
            .ok_or_else(|| CoreError::InvalidRequest("browser is not open".into()))
    }

    async fn remote_session_id(&self, session_id: &BackendSessionId) -> CoreResult<String> {
        self.state
            .lock()
            .await
            .sessions
            .get(&session_id.0)
            .map(|session| session.remote_session_id.clone())
            .ok_or_else(|| CoreError::NotFound(format!("backend session {}", session_id.0)))
    }
}

#[derive(Default)]
struct ExtensionState {
    client: Option<BridgeClient>,
    browser: Option<BrowserDescriptor>,
    sessions: HashMap<String, ExtensionSession>,
    subscriptions: HashMap<String, ExtensionSubscription>,
    connection_task: Option<JoinHandle<()>>,
    event_task: Option<JoinHandle<()>>,
    disconnected: Option<String>,
}

struct ExtensionSession {
    remote_session_id: String,
}

struct ExtensionSubscription {
    queue: Arc<BoundedEventQueue>,
}

#[async_trait]
impl BrowserBackend for ExtensionCdpBackend {
    async fn open(&self, request: OpenBrowserRequest) -> CoreResult<BrowserDescriptor> {
        if request.mode != BrowserMode::ChromeExtension {
            return Err(CoreError::Unsupported(
                "extension backend only supports chrome_extension mode".into(),
            ));
        }
        if self.state.lock().await.client.is_some() {
            return Err(CoreError::InvalidRequest("backend is already open".into()));
        }

        let (client, hello, event_rx, connection_task) =
            BridgeClient::connect(&self.config).await?;
        let mut capabilities = vec!["existing_chrome".into(), "bridge_authenticated".into()];
        capabilities.extend(hello.capabilities);
        capabilities.sort();
        capabilities.dedup();
        if capabilities
            .iter()
            .any(|capability| capability == "native_tab_groups")
        {
            let session_name = request
                .session_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("ChatOS Browser");
            client
                .request(
                    "bridge.configureSession",
                    json!({ "session_name": session_name }),
                    CONNECT_TIMEOUT,
                )
                .await?;
        } else if request.session_name.is_some() {
            client.close().await;
            return Err(CoreError::Unsupported(
                "the connected Chrome extension does not support native task tab groups; update the Browser Bridge extension"
                    .into(),
            ));
        }
        let descriptor = BrowserDescriptor {
            mode: BrowserMode::ChromeExtension,
            product: hello.product,
            user_agent: hello.user_agent,
            capabilities,
        };
        {
            let mut state = self.state.lock().await;
            state.client = Some(client);
            state.browser = Some(descriptor.clone());
            state.connection_task = Some(connection_task);
            state.disconnected = None;
        }
        let event_task = spawn_event_dispatcher(self.state.clone(), event_rx);
        self.state.lock().await.event_task = Some(event_task);
        Ok(descriptor)
    }

    async fn list_targets(&self) -> CoreResult<Vec<TargetDescriptor>> {
        let result = self
            .client()
            .await?
            .request("bridge.listTargets", json!({}), CONNECT_TIMEOUT)
            .await?;
        serde_json::from_value(
            result
                .get("targets")
                .cloned()
                .ok_or_else(|| CoreError::Backend("Bridge omitted targets".into()))?,
        )
        .map_err(|_| CoreError::Backend("Bridge returned invalid targets".into()))
    }

    async fn create_target(&self, url: &str) -> CoreResult<TargetDescriptor> {
        let result = self
            .client()
            .await?
            .request(
                "bridge.createTarget",
                json!({ "url": url }),
                CONNECT_TIMEOUT,
            )
            .await?;
        deserialize_target(result)
    }

    async fn close_target(&self, target_id: &str) -> CoreResult<()> {
        self.client()
            .await?
            .request(
                "bridge.closeTarget",
                json!({ "target_id": target_id }),
                CONNECT_TIMEOUT,
            )
            .await?;
        Ok(())
    }

    async fn attach_target(&self, target_id: &str) -> CoreResult<BackendSessionId> {
        let result = self
            .client()
            .await?
            .request(
                "bridge.attachTarget",
                json!({ "target_id": target_id }),
                CONNECT_TIMEOUT,
            )
            .await?;
        let remote_session_id = required_result_string(&result, "session_id")?;
        let local_session_id = format!("backend_{}", Uuid::new_v4().simple());
        self.state.lock().await.sessions.insert(
            local_session_id.clone(),
            ExtensionSession { remote_session_id },
        );
        Ok(BackendSessionId(local_session_id))
    }

    async fn detach_target(&self, session_id: &BackendSessionId) -> CoreResult<()> {
        let remote_session_id = self.remote_session_id(session_id).await?;
        self.client()
            .await?
            .request(
                "bridge.detachTarget",
                json!({ "session_id": remote_session_id }),
                CONNECT_TIMEOUT,
            )
            .await?;
        self.state.lock().await.sessions.remove(&session_id.0);
        Ok(())
    }

    async fn send_command(
        &self,
        session_id: Option<&BackendSessionId>,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> CoreResult<Value> {
        let remote_session_id = match session_id {
            Some(session_id) => Some(self.remote_session_id(session_id).await?),
            None => None,
        };
        let result = self
            .client()
            .await?
            .request(
                "cdp.send",
                json!({
                    "session_id": remote_session_id,
                    "method": method,
                    "params": params
                }),
                timeout,
            )
            .await?;
        result
            .get("result")
            .cloned()
            .ok_or_else(|| CoreError::Backend("Bridge omitted the CDP result".into()))
    }

    async fn subscribe(&self, filter: EventFilter) -> CoreResult<String> {
        if filter.methods.is_empty() {
            return Err(CoreError::InvalidRequest(
                "an event subscription requires at least one method".into(),
            ));
        }
        let remote_session_id = match &filter.session_id {
            Some(session_id) => Some(self.remote_session_id(session_id).await?),
            None => None,
        };
        let subscription_id = format!("backend_sub_{}", Uuid::new_v4().simple());
        let queue = Arc::new(BoundedEventQueue::default());
        let client = self.client().await?;
        self.state
            .lock()
            .await
            .subscriptions
            .insert(subscription_id.clone(), ExtensionSubscription { queue });
        let result = client
            .request(
                "bridge.subscribe",
                json!({
                    "subscription_id": subscription_id,
                    "session_id": remote_session_id,
                    "methods": filter.methods
                }),
                CONNECT_TIMEOUT,
            )
            .await;
        if let Err(error) = result {
            self.state
                .lock()
                .await
                .subscriptions
                .remove(&subscription_id);
            return Err(error);
        }
        Ok(subscription_id)
    }

    async fn poll_events(
        &self,
        subscription_id: &str,
        after_sequence: u64,
        max_events: usize,
        wait: Duration,
    ) -> CoreResult<EventBatch> {
        let queue = self
            .state
            .lock()
            .await
            .subscriptions
            .get(subscription_id)
            .map(|subscription| subscription.queue.clone())
            .ok_or_else(|| {
                CoreError::NotFound(format!("backend subscription {subscription_id}"))
            })?;
        queue.poll(after_sequence, max_events, wait).await
    }

    async fn unsubscribe(&self, subscription_id: &str) -> CoreResult<()> {
        let client = self.client().await;
        let subscription = self
            .state
            .lock()
            .await
            .subscriptions
            .remove(subscription_id)
            .ok_or_else(|| {
                CoreError::NotFound(format!("backend subscription {subscription_id}"))
            })?;
        subscription.queue.close("subscription was removed").await;
        let result = client?
            .request(
                "bridge.unsubscribe",
                json!({ "subscription_id": subscription_id }),
                CONNECT_TIMEOUT,
            )
            .await;
        result.map(|_| ())
    }

    async fn close(&self) -> CoreResult<()> {
        let (client, mut connection_task, mut event_task, subscriptions) = {
            let mut state = self.state.lock().await;
            state.sessions.clear();
            state.browser = None;
            state.disconnected = Some("Browser Bridge connection is closed".into());
            (
                state.client.take(),
                state.connection_task.take(),
                state.event_task.take(),
                state
                    .subscriptions
                    .drain()
                    .map(|(_, subscription)| subscription.queue)
                    .collect::<Vec<_>>(),
            )
        };
        for queue in subscriptions {
            queue.close("Browser Bridge connection is closed").await;
        }
        if let Some(client) = client {
            let _ = client
                .request("bridge.close", json!({}), CLOSE_TIMEOUT)
                .await;
            client.close().await;
        }
        if let Some(task) = connection_task.as_mut()
            && tokio::time::timeout(CLOSE_TIMEOUT, &mut *task)
                .await
                .is_err()
        {
            task.abort();
        }
        if let Some(task) = event_task.as_mut()
            && tokio::time::timeout(CLOSE_TIMEOUT, &mut *task)
                .await
                .is_err()
        {
            task.abort();
        }
        Ok(())
    }
}

fn deserialize_target(result: Value) -> CoreResult<TargetDescriptor> {
    serde_json::from_value(
        result
            .get("target")
            .cloned()
            .ok_or_else(|| CoreError::Backend("Bridge omitted target".into()))?,
    )
    .map_err(|_| CoreError::Backend("Bridge returned an invalid target".into()))
}

fn required_result_string(result: &Value, field: &str) -> CoreResult<String> {
    result
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| CoreError::Backend(format!("Bridge omitted {field}")))
}

#[derive(Clone)]
struct BridgeClient {
    outbound: mpsc::Sender<Outbound>,
    next_id: Arc<AtomicU64>,
    disconnected: Arc<AtomicBool>,
}

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
mod tests {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use async_tungstenite::{
        tokio::accept_hdr_async,
        tungstenite::{
            Message,
            handshake::server::{Request, Response},
            http::HeaderValue,
        },
    };
    use browser_cdp_core::BrowserBackend;
    use futures::StreamExt;
    use tempfile::tempdir;
    use tokio::net::TcpListener;

    use super::*;

    const TEST_TOKEN: &str = "test-bridge-token-0123456789";

    #[test]
    fn endpoint_is_strictly_loopback_and_carries_no_credentials() {
        assert!(validate_endpoint("ws://127.0.0.1:9223/v1/browser").is_ok());
        assert!(validate_endpoint("ws://[::1]:9223/v1/browser").is_ok());
        assert!(validate_endpoint("ws://localhost:9223/v1/browser").is_err());
        assert!(validate_endpoint("ws://192.0.2.1:9223/v1/browser").is_err());
        assert!(validate_endpoint("wss://127.0.0.1:9223/v1/browser").is_err());
        assert!(validate_endpoint("ws://user:secret@127.0.0.1:9223/").is_err());
        assert!(validate_endpoint("ws://127.0.0.1:9223/?token=secret").is_err());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn credential_file_must_be_private_and_unexpired() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("bridge.json");
        let expires = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            + 60_000;
        tokio::fs::write(
            &path,
            serde_json::to_vec(&json!({
                "token": TEST_TOKEN,
                "expires_at_unix_ms": expires
            }))
            .unwrap(),
        )
        .await
        .unwrap();
        tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .await
            .unwrap();
        assert_eq!(read_credential_file(&path).await.unwrap(), TEST_TOKEN);
        tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .await
            .unwrap();
        assert!(read_credential_file(&path).await.is_err());
    }

    #[tokio::test]
    async fn bridge_backend_authenticates_routes_commands_and_events() {
        let (endpoint, server) = spawn_mock_bridge(TEST_TOKEN, false, false).await;
        let config = BridgeConfig::new(&endpoint, TEST_TOKEN.into()).unwrap();
        let factory = ExtensionBackendFactory::with_config(config);
        let backend = factory.create(BrowserMode::ChromeExtension).await.unwrap();
        let descriptor = backend
            .open(OpenBrowserRequest {
                mode: BrowserMode::ChromeExtension,
                headless: true,
                persistent_profile: false,
                session_name: Some("Release verification".into()),
            })
            .await
            .unwrap();
        assert_eq!(descriptor.product, "Mock Chrome/1");
        assert!(descriptor.capabilities.contains(&"existing_chrome".into()));

        let targets = backend.list_targets().await.unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].id, "approved-tab-1");
        let session = backend.attach_target(&targets[0].id).await.unwrap();
        let value = backend
            .send_command(
                Some(&session),
                "Runtime.evaluate",
                json!({ "expression": "1 + 1" }),
                Duration::from_secs(1),
            )
            .await
            .unwrap();
        assert_eq!(value["result"]["value"], 2);

        let subscription_id = backend
            .subscribe(EventFilter {
                methods: vec!["Runtime.consoleAPICalled".into()],
                session_id: Some(session.clone()),
            })
            .await
            .unwrap();
        let events = backend
            .poll_events(&subscription_id, 0, 10, Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(events.events.len(), 1);
        assert_eq!(events.events[0].method, "Runtime.consoleAPICalled");
        assert_eq!(events.events[0].params["authorization"], "[REDACTED]");

        let unsupported = backend
            .send_command(
                None,
                "Browser.unsupported",
                json!({}),
                Duration::from_secs(1),
            )
            .await
            .unwrap_err();
        assert!(matches!(unsupported, CoreError::Unsupported(_)));
        backend.unsubscribe(&subscription_id).await.unwrap();
        backend.detach_target(&session).await.unwrap();
        backend.close().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn authentication_failure_is_generic_and_fails_closed() {
        let (endpoint, server) = spawn_mock_bridge(TEST_TOKEN, true, false).await;
        let config = BridgeConfig::new(&endpoint, "wrong-bridge-token-012345".into()).unwrap();
        let backend = ExtensionCdpBackend::new(config);
        let error = backend
            .open(OpenBrowserRequest {
                mode: BrowserMode::ChromeExtension,
                headless: true,
                persistent_profile: false,
                session_name: None,
            })
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "browser backend error: Browser Bridge authentication failed"
        );
        assert!(!error.to_string().contains(TEST_TOKEN));
        server.await.unwrap();
    }

    #[test]
    fn disconnected_extension_authentication_error_is_actionable() {
        let result = parse_authentication_response(Message::text(
            json!({
                "type": "response",
                "id": 1,
                "error": {
                    "code": "extension_unavailable",
                    "message": "Chrome extension is not connected"
                }
            })
            .to_string(),
        ));
        let error = match result {
            Ok(_) => panic!("disconnected extension authentication must fail"),
            Err(error) => error,
        };
        assert_eq!(
            error.to_string(),
            "browser backend error: Chrome extension is not connected. Start a Browser CDP task, then click First connect in the Chatos Browser Bridge extension."
        );
    }

    #[tokio::test]
    async fn extension_disconnect_immediately_invalidates_event_polling() {
        let (endpoint, server) = spawn_mock_bridge(TEST_TOKEN, false, true).await;
        let config = BridgeConfig::new(&endpoint, TEST_TOKEN.into()).unwrap();
        let backend = ExtensionCdpBackend::new(config);
        backend
            .open(OpenBrowserRequest {
                mode: BrowserMode::ChromeExtension,
                headless: true,
                persistent_profile: false,
                session_name: None,
            })
            .await
            .unwrap();
        let subscription_id = backend
            .subscribe(EventFilter {
                methods: vec!["Browser.downloadWillBegin".into()],
                session_id: None,
            })
            .await
            .unwrap();
        let error = backend
            .poll_events(&subscription_id, 0, 10, Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("extension_disabled"));
        backend.close().await.unwrap();
        server.await.unwrap();
    }

    async fn spawn_mock_bridge(
        expected_token: &'static str,
        reject_auth: bool,
        disconnect_after_subscribe: bool,
    ) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket =
                accept_hdr_async(stream, |request: &Request, mut response: Response| {
                    assert_eq!(
                        request.headers().get(SEC_WEBSOCKET_PROTOCOL).unwrap(),
                        BRIDGE_SUBPROTOCOL
                    );
                    response.headers_mut().insert(
                        SEC_WEBSOCKET_PROTOCOL,
                        HeaderValue::from_static(BRIDGE_SUBPROTOCOL),
                    );
                    Ok(response)
                })
                .await
                .unwrap();
            let authentication = socket.next().await.unwrap().unwrap();
            let Message::Text(authentication) = authentication else {
                panic!("authentication must be text");
            };
            let authentication: Value = serde_json::from_str(&authentication).unwrap();
            assert_eq!(authentication["method"], "bridge.authenticate");
            if reject_auth || authentication["params"]["token"] != expected_token {
                socket
                    .send(Message::text(
                        json!({
                            "type": "response",
                            "id": 1,
                            "error": {"code": "token_expired", "message": "secret details"}
                        })
                        .to_string(),
                    ))
                    .await
                    .unwrap();
                let _ = socket.close(None).await;
                return;
            }
            socket
                .send(Message::text(
                    json!({
                        "type": "response",
                        "id": 1,
                        "result": {
                            "protocol_version": BRIDGE_PROTOCOL_VERSION,
                            "connection_id": "mock-connection",
                            "product": "Mock Chrome/1",
                            "user_agent": "Mock Chrome",
                            "capabilities": ["page_control", "raw_cdp", "native_tab_groups"]
                        }
                    })
                    .to_string(),
                ))
                .await
                .unwrap();
            while let Some(Ok(Message::Text(text))) = socket.next().await {
                let request: Value = serde_json::from_str(&text).unwrap();
                let id = request["id"].as_u64().unwrap();
                let method = request["method"].as_str().unwrap();
                let result = match method {
                    "bridge.configureSession" => {
                        assert!(
                            request["params"]["session_name"]
                                .as_str()
                                .is_some_and(|value| !value.trim().is_empty())
                        );
                        json!({})
                    }
                    "bridge.listTargets" => json!({
                        "targets": [{
                            "id": "approved-tab-1",
                            "title": "Approved",
                            "url": "https://example.test/",
                            "kind": "page"
                        }]
                    }),
                    "bridge.attachTarget" => json!({"session_id": "remote-session-1"}),
                    "bridge.detachTarget" | "bridge.unsubscribe" | "bridge.close" => json!({}),
                    "cdp.send" if request["params"]["method"] == "Browser.unsupported" => {
                        socket
                            .send(Message::text(
                                json!({
                                    "type": "response",
                                    "id": id,
                                    "error": {
                                        "code": "unsupported_by_backend",
                                        "message": "browser command is unavailable"
                                    }
                                })
                                .to_string(),
                            ))
                            .await
                            .unwrap();
                        continue;
                    }
                    "cdp.send" => json!({
                        "result": {"result": {"type": "number", "value": 2}}
                    }),
                    "bridge.subscribe" => {
                        let subscription_id = request["params"]["subscription_id"].clone();
                        socket
                            .send(Message::text(
                                json!({"type": "response", "id": id, "result": {}}).to_string(),
                            ))
                            .await
                            .unwrap();
                        if disconnect_after_subscribe {
                            socket
                                .send(Message::text(
                                    json!({
                                        "type": "event",
                                        "method": "bridge.disconnected",
                                        "params": {"reason": "extension_disabled"}
                                    })
                                    .to_string(),
                                ))
                                .await
                                .unwrap();
                            let _ = socket.close(None).await;
                            break;
                        }
                        socket
                            .send(Message::text(
                                json!({
                                    "type": "event",
                                    "method": "cdp.event",
                                    "params": {
                                        "subscription_id": subscription_id,
                                        "session_id": "remote-session-1",
                                        "method": "Runtime.consoleAPICalled",
                                        "params": {"authorization": "Bearer secret"}
                                    }
                                })
                                .to_string(),
                            ))
                            .await
                            .unwrap();
                        continue;
                    }
                    other => panic!("unexpected Bridge method {other}"),
                };
                socket
                    .send(Message::text(
                        json!({"type": "response", "id": id, "result": result}).to_string(),
                    ))
                    .await
                    .unwrap();
                if method == "bridge.close" {
                    let _ = socket.close(None).await;
                    break;
                }
            }
        });
        (format!("ws://{address}/v1/browser"), task)
    }
}
