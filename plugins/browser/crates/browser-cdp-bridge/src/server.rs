use std::{
    collections::HashMap,
    future::Future,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_tungstenite::{
    WebSocketStream,
    tokio::{TokioAdapter, accept_hdr_async},
    tungstenite::{
        Message,
        handshake::server::{Request, Response},
        http::{HeaderValue, header::SEC_WEBSOCKET_PROTOCOL},
    },
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Mutex, mpsc, oneshot},
};
use uuid::Uuid;

use crate::{
    STATE_FILE_NAME,
    wire::{
        BROWSER_SUBPROTOCOL, CONTROL_SUBPROTOCOL, EXTENSION_SUBPROTOCOL, MAX_MESSAGE_BYTES,
        PROTOCOL_VERSION, WireError, WireMessage, error_response, event, request, response,
    },
};

const AUTH_TIMEOUT: Duration = Duration::from_secs(5);
const RELAY_TIMEOUT: Duration = Duration::from_secs(15);
const EXTENSION_TOKEN_LIFETIME: Duration = Duration::from_secs(120);
const DEFAULT_MCP_TOKEN_LIFETIME: Duration = Duration::from_secs(8 * 60 * 60);
const BRIDGE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
const CHANNEL_CAPACITY: usize = 256;
const PAIRING_FILE_NAME: &str = "extension-pairing.json";

#[derive(Debug, Clone)]
pub struct BridgeServerConfig {
    pub data_dir: PathBuf,
    pub extension_id: String,
    pub bind_address: String,
    pub mcp_token_lifetime: Duration,
}

impl BridgeServerConfig {
    pub fn development(data_dir: PathBuf, extension_id: impl Into<String>) -> Self {
        Self {
            data_dir,
            extension_id: extension_id.into(),
            bind_address: "127.0.0.1:0".into(),
            mcp_token_lifetime: DEFAULT_MCP_TOKEN_LIFETIME,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BridgeReady {
    pub state_file: PathBuf,
    pub browser_endpoint: String,
    pub mcp_credential_file: PathBuf,
    pub extension_origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BridgeStateFile {
    pub protocol_version: String,
    pub control_endpoint: String,
    pub extension_endpoint: String,
    pub browser_endpoint: String,
    pub control_token: String,
    pub mcp_credential_file: PathBuf,
    pub allowed_extension_origin: String,
    pub expires_at_unix_ms: u64,
}

#[derive(Serialize)]
struct McpCredentialFile<'a> {
    token: &'a str,
    expires_at_unix_ms: u64,
}

#[derive(Serialize, Deserialize)]
struct PersistedPairing {
    protocol_version: String,
    allowed_extension_origin: String,
}

pub struct BridgeServer {
    listener: TcpListener,
    state: Arc<ServerState>,
}

struct ServerState {
    control_token: String,
    mcp_token: String,
    mcp_expires_at_unix_ms: u64,
    mcp_token_used: AtomicBool,
    allowed_extension_origin: String,
    pairing_file: PathBuf,
    paired: AtomicBool,
    extension_tokens: Mutex<HashMap<String, u64>>,
    extension: Mutex<Option<Arc<ExtensionClient>>>,
    mcp: Mutex<Option<McpPeer>>,
}

#[derive(Clone)]
struct McpPeer {
    id: String,
    outbound: mpsc::Sender<Value>,
}

struct ExtensionClient {
    id: String,
    outbound: mpsc::Sender<Value>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, WireError>>>>,
    next_id: AtomicU64,
    connected: AtomicBool,
}

#[derive(Clone, Copy)]
enum Route {
    Control,
    Extension,
    Browser,
}

impl Route {
    fn for_request(request: &Request) -> Option<Self> {
        let offered = request
            .headers()
            .get(SEC_WEBSOCKET_PROTOCOL)
            .and_then(|value| value.to_str().ok())?;
        match request.uri().path() {
            "/v1/control" if offered_protocol(offered, CONTROL_SUBPROTOCOL) => Some(Self::Control),
            "/v1/extension" if offered_protocol(offered, EXTENSION_SUBPROTOCOL) => {
                Some(Self::Extension)
            }
            "/v1/browser" if offered_protocol(offered, BROWSER_SUBPROTOCOL) => Some(Self::Browser),
            _ => None,
        }
    }

    fn subprotocol(self) -> &'static str {
        match self {
            Self::Control => CONTROL_SUBPROTOCOL,
            Self::Extension => EXTENSION_SUBPROTOCOL,
            Self::Browser => BROWSER_SUBPROTOCOL,
        }
    }
}

impl BridgeServer {
    pub async fn bind(config: BridgeServerConfig) -> Result<(Self, BridgeReady), String> {
        validate_extension_id(&config.extension_id)?;
        let listener = TcpListener::bind(&config.bind_address)
            .await
            .map_err(|error| format!("could not bind Browser Bridge: {error}"))?;
        let address = listener
            .local_addr()
            .map_err(|error| format!("could not inspect Browser Bridge listener: {error}"))?;
        if !address.ip().is_loopback() {
            return Err("Browser Bridge must bind to loopback".into());
        }
        let now = unix_ms();
        let expires_at_unix_ms = now.saturating_add(config.mcp_token_lifetime.as_millis() as u64);
        let control_token = random_token("control");
        let mcp_token = random_token("mcp");
        let allowed_extension_origin = format!("chrome-extension://{}/", config.extension_id);
        let base = format!("ws://{address}");
        let state_file = config.data_dir.join(STATE_FILE_NAME);
        let credential_file = config.data_dir.join("mcp-credential.json");
        let pairing_file = config.data_dir.join(PAIRING_FILE_NAME);
        tokio::fs::create_dir_all(&config.data_dir)
            .await
            .map_err(|error| format!("could not create Bridge data directory: {error}"))?;
        let paired = load_persisted_pairing(&pairing_file, &allowed_extension_origin).await;
        let persisted = BridgeStateFile {
            protocol_version: PROTOCOL_VERSION.into(),
            control_endpoint: format!("{base}/v1/control"),
            extension_endpoint: format!("{base}/v1/extension"),
            browser_endpoint: format!("{base}/v1/browser"),
            control_token: control_token.clone(),
            mcp_credential_file: credential_file.clone(),
            allowed_extension_origin: allowed_extension_origin.clone(),
            expires_at_unix_ms,
        };
        write_private_json(&state_file, &persisted).await?;
        write_private_json(
            &credential_file,
            &McpCredentialFile {
                token: &mcp_token,
                expires_at_unix_ms,
            },
        )
        .await?;
        let ready = BridgeReady {
            state_file,
            browser_endpoint: persisted.browser_endpoint,
            mcp_credential_file: credential_file,
            extension_origin: allowed_extension_origin.clone(),
        };
        Ok((
            Self {
                listener,
                state: Arc::new(ServerState {
                    control_token,
                    mcp_token,
                    mcp_expires_at_unix_ms: expires_at_unix_ms,
                    mcp_token_used: AtomicBool::new(false),
                    allowed_extension_origin,
                    pairing_file,
                    paired: AtomicBool::new(paired),
                    extension_tokens: Mutex::new(HashMap::new()),
                    extension: Mutex::new(None),
                    mcp: Mutex::new(None),
                }),
            },
            ready,
        ))
    }

    pub async fn serve(self) -> Result<(), String> {
        self.serve_until(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
    }

    pub async fn serve_until<F>(self, shutdown: F) -> Result<(), String>
    where
        F: Future<Output = ()>,
    {
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                _ = &mut shutdown => return Ok(()),
                accepted = self.listener.accept() => {
                    let (stream, _) = accepted.map_err(|error| format!("Bridge accept failed: {error}"))?;
                    let state = self.state.clone();
                    tokio::spawn(async move {
                        if let Err(error) = accept_connection(stream, state).await {
                            tracing_compat_warn(&error);
                        }
                    });
                }
            }
        }
    }
}

impl ExtensionClient {
    async fn request(&self, method: &str, params: Value) -> Result<Value, WireError> {
        if !self.connected.load(Ordering::Acquire) {
            return Err(WireError::new(
                "extension_unavailable",
                "Chrome extension is disconnected",
            ));
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        if self
            .outbound
            .send(request(id, method, params))
            .await
            .is_err()
        {
            self.pending.lock().await.remove(&id);
            return Err(WireError::new(
                "extension_unavailable",
                "Chrome extension is disconnected",
            ));
        }
        match tokio::time::timeout(RELAY_TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(WireError::new(
                "extension_unavailable",
                "Chrome extension is disconnected",
            )),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(WireError::new("timeout", format!("{method} timed out")))
            }
        }
    }

    async fn close(&self, reason: &str) {
        self.connected.store(false, Ordering::Release);
        let pending = self.pending.lock().await.drain().collect::<Vec<_>>();
        for (_, callback) in pending {
            let _ = callback.send(Err(WireError::new(
                "extension_unavailable",
                reason.to_owned(),
            )));
        }
    }
}

#[allow(clippy::result_large_err)] // Tungstenite fixes the callback error response type.
async fn accept_connection(stream: TcpStream, state: Arc<ServerState>) -> Result<(), String> {
    let route_slot = Arc::new(StdMutex::new(None));
    let callback_slot = route_slot.clone();
    let socket = accept_hdr_async(stream, move |request: &Request, mut response: Response| {
        let route = Route::for_request(request);
        if let Some(route) = route {
            let protocol = HeaderValue::from_static(route.subprotocol());
            let headers = response.headers_mut();
            headers.insert(SEC_WEBSOCKET_PROTOCOL, protocol);
        }
        *callback_slot.lock().expect("route lock") = route;
        Ok(response)
    })
    .await
    .map_err(|_| "WebSocket handshake failed".to_owned())?;
    let route = route_slot
        .lock()
        .expect("route lock")
        .take()
        .ok_or_else(|| "WebSocket route or subprotocol was rejected".to_owned())?;
    match route {
        Route::Control => handle_control(socket, state).await,
        Route::Extension => handle_extension(socket, state).await,
        Route::Browser => handle_browser(socket, state).await,
    }
}

type BridgeSocket = WebSocketStream<TokioAdapter<TcpStream>>;

async fn handle_control(mut socket: BridgeSocket, state: Arc<ServerState>) -> Result<(), String> {
    let authentication = next_wire(&mut socket).await?;
    let auth_id = request_id(&authentication)?;
    if authentication.method.as_deref() != Some("control.authenticate")
        || authentication
            .params
            .get("protocol_version")
            .and_then(Value::as_str)
            != Some(PROTOCOL_VERSION)
        || authentication.params.get("token").and_then(Value::as_str)
            != Some(state.control_token.as_str())
    {
        send_value(
            &mut socket,
            error_response(
                auth_id,
                WireError::new("permission_denied", "Control authentication failed"),
            ),
        )
        .await?;
        return Ok(());
    }
    send_value(
        &mut socket,
        response(auth_id, json!({"protocol_version":PROTOCOL_VERSION})),
    )
    .await?;

    while let Ok(message) = next_wire(&mut socket).await {
        let id = request_id(&message)?;
        if message.method.as_deref() != Some("control.bootstrapExtension") {
            send_value(
                &mut socket,
                error_response(
                    id,
                    WireError::new("unsupported_by_backend", "Unsupported control method"),
                ),
            )
            .await?;
            continue;
        }
        let origin = message.params.get("origin").and_then(Value::as_str);
        let pairing_requested = message
            .params
            .get("pairing_requested")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if origin != Some(state.allowed_extension_origin.as_str()) {
            send_value(
                &mut socket,
                error_response(
                    id,
                    WireError::new(
                        "permission_denied",
                        "Chrome extension identity was rejected",
                    ),
                ),
            )
            .await?;
            continue;
        }
        if pairing_requested {
            if let Err(error) = persist_pairing(&state).await {
                send_value(
                    &mut socket,
                    error_response(id, WireError::new("backend_error", error)),
                )
                .await?;
                continue;
            }
            state.paired.store(true, Ordering::Release);
        } else if !state.paired.load(Ordering::Acquire) {
            send_value(
                &mut socket,
                error_response(
                    id,
                    WireError::new("permission_denied", "Extension is not paired"),
                ),
            )
            .await?;
            continue;
        }
        let token = random_token("extension");
        let expires_at_unix_ms =
            unix_ms().saturating_add(EXTENSION_TOKEN_LIFETIME.as_millis() as u64);
        state
            .extension_tokens
            .lock()
            .await
            .insert(token.clone(), expires_at_unix_ms);
        send_value(
            &mut socket,
            response(
                id,
                json!({
                    "protocol_version":PROTOCOL_VERSION,
                    "token":token,
                    "expires_at_unix_ms":expires_at_unix_ms
                }),
            ),
        )
        .await?;
    }
    Ok(())
}

async fn handle_extension(mut socket: BridgeSocket, state: Arc<ServerState>) -> Result<(), String> {
    let authentication = next_wire(&mut socket).await?;
    let auth_id = request_id(&authentication)?;
    let token = authentication.params.get("token").and_then(Value::as_str);
    let valid = if authentication.method.as_deref() == Some("extension.authenticate")
        && authentication
            .params
            .get("protocol_version")
            .and_then(Value::as_str)
            == Some(PROTOCOL_VERSION)
    {
        if let Some(token) = token {
            state
                .extension_tokens
                .lock()
                .await
                .remove(token)
                .is_some_and(|expiry| expiry > unix_ms())
        } else {
            false
        }
    } else {
        false
    };
    if !valid {
        send_value(
            &mut socket,
            error_response(
                auth_id,
                WireError::new("token_expired", "Extension authentication failed"),
            ),
        )
        .await?;
        return Ok(());
    }
    if state.extension.lock().await.is_some() {
        send_value(
            &mut socket,
            error_response(
                auth_id,
                WireError::new("invalid_request", "An extension is already connected"),
            ),
        )
        .await?;
        return Ok(());
    }
    send_value(
        &mut socket,
        response(auth_id, json!({"protocol_version":PROTOCOL_VERSION})),
    )
    .await?;
    let (mut sink, mut stream) = socket.split();
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<Value>(CHANNEL_CAPACITY);
    let client = Arc::new(ExtensionClient {
        id: format!("extension_{}", Uuid::new_v4().simple()),
        outbound: outbound_tx,
        pending: Mutex::new(HashMap::new()),
        next_id: AtomicU64::new(1),
        connected: AtomicBool::new(true),
    });
    *state.extension.lock().await = Some(client.clone());
    let writer = tokio::spawn(async move {
        let mut heartbeat = tokio::time::interval(BRIDGE_HEARTBEAT_INTERVAL);
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        heartbeat.tick().await;
        loop {
            tokio::select! {
                value = outbound_rx.recv() => {
                    let Some(value) = value else { break; };
                    let Ok(encoded) = serde_json::to_string(&value) else { break; };
                    if encoded.len() > MAX_MESSAGE_BYTES
                        || sink.send(Message::text(encoded)).await.is_err()
                    {
                        break;
                    }
                }
                _ = heartbeat.tick() => {
                    if sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    });
    while let Some(frame) = stream.next().await {
        let message = match frame {
            Ok(Message::Text(text)) if text.len() <= MAX_MESSAGE_BYTES => {
                match serde_json::from_str::<WireMessage>(&text) {
                    Ok(message) => message,
                    Err(_) => break,
                }
            }
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => continue,
            _ => break,
        };
        match message.kind.as_str() {
            "response" => {
                if let Some(id) = message.id.as_ref().and_then(Value::as_u64)
                    && let Some(callback) = client.pending.lock().await.remove(&id)
                {
                    let result = match message.error {
                        Some(error) => Err(error),
                        None => Ok(message.result.unwrap_or(Value::Null)),
                    };
                    let _ = callback.send(result);
                }
            }
            "event" => relay_extension_event(&state, &message).await,
            _ => break,
        }
    }
    client.close("Chrome extension disconnected").await;
    {
        let mut extension = state.extension.lock().await;
        if extension
            .as_ref()
            .is_some_and(|current| current.id == client.id)
        {
            extension.take();
        }
    }
    notify_mcp_disconnected(&state, "extension_unavailable").await;
    writer.abort();
    Ok(())
}

include!("server_part01.rs");
