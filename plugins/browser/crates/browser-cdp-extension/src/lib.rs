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

include!("lib_part01.rs");
