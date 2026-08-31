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
const CHANNEL_CAPACITY: usize = 256;

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
        tokio::fs::create_dir_all(&config.data_dir)
            .await
            .map_err(|error| format!("could not create Bridge data directory: {error}"))?;
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
                    paired: AtomicBool::new(false),
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

async fn accept_connection(stream: TcpStream, state: Arc<ServerState>) -> Result<(), String> {
    let route_slot = Arc::new(StdMutex::new(None));
    let callback_slot = route_slot.clone();
    let socket = accept_hdr_async(stream, move |request: &Request, mut response: Response| {
        let route = Route::for_request(request);
        if let Some(route) = route {
            response.headers_mut().insert(
                SEC_WEBSOCKET_PROTOCOL,
                HeaderValue::from_static(route.subprotocol()),
            );
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
        while let Some(value) = outbound_rx.recv().await {
            let Ok(encoded) = serde_json::to_string(&value) else {
                break;
            };
            if encoded.len() > MAX_MESSAGE_BYTES || sink.send(Message::text(encoded)).await.is_err()
            {
                break;
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

async fn handle_browser(mut socket: BridgeSocket, state: Arc<ServerState>) -> Result<(), String> {
    let authentication = next_wire(&mut socket).await?;
    let auth_id = request_id(&authentication)?;
    let token_valid = authentication.method.as_deref() == Some("bridge.authenticate")
        && authentication
            .params
            .get("protocol_version")
            .and_then(Value::as_str)
            == Some(PROTOCOL_VERSION)
        && authentication.params.get("token").and_then(Value::as_str)
            == Some(state.mcp_token.as_str())
        && state.mcp_expires_at_unix_ms > unix_ms();
    if !token_valid {
        send_value(
            &mut socket,
            error_response(
                auth_id,
                WireError::new("token_expired", "Browser Bridge authentication failed"),
            ),
        )
        .await?;
        return Ok(());
    }
    if state.extension.lock().await.is_none() {
        send_value(
            &mut socket,
            error_response(
                auth_id,
                WireError::new("extension_unavailable", "Chrome extension is not connected"),
            ),
        )
        .await?;
        return Ok(());
    }
    if state.mcp.lock().await.is_some() {
        send_value(
            &mut socket,
            error_response(
                auth_id,
                WireError::new("invalid_request", "An MCP client is already connected"),
            ),
        )
        .await?;
        return Ok(());
    }
    if state
        .mcp_token_used
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        send_value(
            &mut socket,
            error_response(
                auth_id,
                WireError::new(
                    "token_expired",
                    "Browser Bridge credential was already used",
                ),
            ),
        )
        .await?;
        return Ok(());
    }
    let extension = state
        .extension
        .lock()
        .await
        .clone()
        .ok_or_else(|| "Chrome extension disconnected during authentication".to_string())?;
    let extension_info = match extension
        .request("extension.getCapabilities", json!({}))
        .await
    {
        Ok(value) => value,
        Err(error) => {
            send_value(&mut socket, error_response(auth_id, error)).await?;
            return Ok(());
        }
    };
    let capabilities = extension_info
        .get("capabilities")
        .cloned()
        .unwrap_or_else(|| json!([]));
    send_value(
        &mut socket,
        response(
            auth_id,
            json!({
                "protocol_version":PROTOCOL_VERSION,
                "connection_id":format!("bridge_{}", Uuid::new_v4().simple()),
                "product":"Chrome via Chatos Extension",
                "user_agent":"unavailable",
                "capabilities":capabilities
            }),
        ),
    )
    .await?;
    let (mut sink, mut stream) = socket.split();
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<Value>(CHANNEL_CAPACITY);
    let peer = McpPeer {
        id: format!("mcp_{}", Uuid::new_v4().simple()),
        outbound: outbound_tx,
    };
    *state.mcp.lock().await = Some(peer.clone());
    let mut writer = tokio::spawn(async move {
        while let Some(value) = outbound_rx.recv().await {
            let Ok(encoded) = serde_json::to_string(&value) else {
                break;
            };
            if encoded.len() > MAX_MESSAGE_BYTES || sink.send(Message::text(encoded)).await.is_err()
            {
                break;
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
        if message.kind != "request" {
            break;
        }
        let id = match request_id(&message) {
            Ok(id) => id,
            Err(_) => break,
        };
        let method = message.method.clone().unwrap_or_default();
        let closing = method == "bridge.close";
        let result = handle_browser_request(&state, &method, message.params).await;
        let value = match result {
            Ok(result) => response(id, result),
            Err(error) => error_response(id, error),
        };
        if peer.outbound.send(value).await.is_err() || closing {
            break;
        }
    }
    let peer_id = peer.id.clone();
    {
        let mut mcp = state.mcp.lock().await;
        if mcp.as_ref().is_some_and(|current| current.id == peer_id) {
            mcp.take();
        }
    }
    drop(peer);
    if tokio::time::timeout(Duration::from_secs(1), &mut writer)
        .await
        .is_err()
    {
        writer.abort();
    }
    Ok(())
}

async fn handle_browser_request(
    state: &ServerState,
    method: &str,
    params: Value,
) -> Result<Value, WireError> {
    let extension = match state.extension.lock().await.clone() {
        Some(extension) => extension,
        None if method == "bridge.close" => return Ok(json!({})),
        None => {
            return Err(WireError::new(
                "extension_unavailable",
                "Chrome extension is not connected",
            ));
        }
    };
    if method == "bridge.close" {
        return extension.request("extension.endSession", params).await;
    }
    let extension_method = match method {
        "bridge.configureSession" => "extension.configureSession",
        "bridge.listTargets" => "extension.listTargets",
        "bridge.createTarget" => "extension.createTarget",
        "bridge.closeTarget" => "extension.closeTarget",
        "bridge.attachTarget" => "extension.attachTarget",
        "bridge.detachTarget" => "extension.detachTarget",
        "cdp.send" => "extension.cdpSend",
        "bridge.subscribe" => "extension.subscribe",
        "bridge.unsubscribe" => "extension.unsubscribe",
        _ => {
            return Err(WireError::new(
                "unsupported_by_backend",
                format!("Unsupported Browser Bridge method: {method}"),
            ));
        }
    };
    extension.request(extension_method, params).await
}

async fn relay_extension_event(state: &ServerState, message: &WireMessage) {
    match message.method.as_deref() {
        Some("extension.cdpEvent") => {
            if let Some(peer) = state.mcp.lock().await.clone() {
                let _ = peer
                    .outbound
                    .send(event("cdp.event", message.params.clone()))
                    .await;
            }
        }
        Some("extension.detached") => {
            notify_mcp_disconnected(state, "debugger_detached").await;
        }
        _ => {}
    }
}

async fn notify_mcp_disconnected(state: &ServerState, reason: &str) {
    if let Some(peer) = state.mcp.lock().await.clone() {
        let _ = peer
            .outbound
            .send(event("bridge.disconnected", json!({"reason":reason})))
            .await;
    }
}

async fn next_wire(socket: &mut BridgeSocket) -> Result<WireMessage, String> {
    let frame = tokio::time::timeout(AUTH_TIMEOUT, socket.next())
        .await
        .map_err(|_| "WebSocket message timed out".to_owned())?
        .ok_or_else(|| "WebSocket closed".to_owned())?
        .map_err(|_| "WebSocket read failed".to_owned())?;
    let Message::Text(text) = frame else {
        return Err("WebSocket message must be JSON text".into());
    };
    if text.len() > MAX_MESSAGE_BYTES {
        return Err("WebSocket message exceeds 8 MiB".into());
    }
    serde_json::from_str(&text).map_err(|_| "WebSocket message is invalid JSON".into())
}

async fn send_value(socket: &mut BridgeSocket, value: Value) -> Result<(), String> {
    let encoded =
        serde_json::to_string(&value).map_err(|_| "could not encode response".to_owned())?;
    if encoded.len() > MAX_MESSAGE_BYTES {
        return Err("response exceeds 8 MiB".into());
    }
    socket
        .send(Message::text(encoded))
        .await
        .map_err(|_| "WebSocket write failed".to_owned())
}

fn request_id(message: &WireMessage) -> Result<Value, String> {
    if message.kind != "request" {
        return Err("expected a request message".into());
    }
    message
        .id
        .clone()
        .filter(|id| id.is_number() || id.is_string())
        .ok_or_else(|| "request omitted its ID".into())
}

fn offered_protocol(header: &str, expected: &str) -> bool {
    header.split(',').any(|value| value.trim() == expected)
}

fn random_token(prefix: &str) -> String {
    format!(
        "{prefix}_{}_{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn validate_extension_id(extension_id: &str) -> Result<(), String> {
    if extension_id.len() != 32
        || !extension_id
            .chars()
            .all(|character| matches!(character, 'a'..='p'))
    {
        return Err("Chrome extension ID must contain 32 characters in the range a-p".into());
    }
    Ok(())
}

async fn write_private_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("could not serialize {}: {error}", path.display()))?;
    tokio::fs::write(path, bytes)
        .await
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|error| format!("could not secure {}: {error}", path.display()))?;
    }
    Ok(())
}

fn tracing_compat_warn(error: &str) {
    eprintln!("Browser Bridge connection closed: {error}");
}

#[cfg(test)]
mod tests {
    use async_tungstenite::{
        tokio::connect_async,
        tungstenite::{
            Message,
            client::IntoClientRequest,
            http::{HeaderValue, header::SEC_WEBSOCKET_PROTOCOL},
        },
    };
    use futures::StreamExt;
    use tempfile::tempdir;
    use tokio::sync::oneshot;

    use super::*;

    const EXTENSION_ID: &str = "nkeimhogjdpnpccoofpliimaahmaaome";

    #[tokio::test]
    async fn relays_browser_commands_and_extension_events() {
        let directory = tempdir().unwrap();
        let (server, ready) = BridgeServer::bind(BridgeServerConfig::development(
            directory.path().to_owned(),
            EXTENSION_ID,
        ))
        .await
        .unwrap();
        let persisted: BridgeStateFile =
            serde_json::from_slice(&tokio::fs::read(&ready.state_file).await.unwrap()).unwrap();
        let credential: Value =
            serde_json::from_slice(&tokio::fs::read(&ready.mcp_credential_file).await.unwrap())
                .unwrap();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let server_task = tokio::spawn(server.serve_until(async {
            let _ = shutdown_rx.await;
        }));

        let mut control = connect(&persisted.control_endpoint, CONTROL_SUBPROTOCOL).await;
        send_json(
            &mut control,
            json!({
                "type":"request","id":1,"method":"control.authenticate",
                "params":{"protocol_version":PROTOCOL_VERSION,"token":persisted.control_token}
            }),
        )
        .await;
        assert_eq!(receive_json(&mut control).await["id"], 1);
        send_json(
            &mut control,
            json!({
                "type":"request","id":2,"method":"control.bootstrapExtension",
                "params":{"origin":format!("chrome-extension://{EXTENSION_ID}/"),"pairing_requested":true}
            }),
        )
        .await;
        let bootstrap = receive_json(&mut control).await;
        let extension_token = bootstrap["result"]["token"].as_str().unwrap().to_owned();

        let mut extension = connect(&persisted.extension_endpoint, EXTENSION_SUBPROTOCOL).await;
        send_json(
            &mut extension,
            json!({
                "type":"request","id":1,"method":"extension.authenticate",
                "params":{"protocol_version":PROTOCOL_VERSION,"token":extension_token}
            }),
        )
        .await;
        assert_eq!(
            receive_json(&mut extension).await["result"]["protocol_version"],
            "1.0"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;

        let mut browser = connect(&persisted.browser_endpoint, BROWSER_SUBPROTOCOL).await;
        send_json(
            &mut browser,
            json!({
                "type":"request","id":1,"method":"bridge.authenticate",
                "params":{"protocol_version":PROTOCOL_VERSION,"token":credential["token"]}
            }),
        )
        .await;
        let capabilities_request = receive_json(&mut extension).await;
        assert_eq!(capabilities_request["method"], "extension.getCapabilities");
        send_json(
            &mut extension,
            json!({
                "type":"response","id":capabilities_request["id"],
                "result":{"capabilities":["page_control","raw_cdp","native_tab_groups"]}
            }),
        )
        .await;
        assert_eq!(
            receive_json(&mut browser).await["result"]["protocol_version"],
            "1.0"
        );

        send_json(
            &mut browser,
            json!({"type":"request","id":2,"method":"bridge.listTargets","params":{}}),
        )
        .await;
        let extension_request = receive_json(&mut extension).await;
        assert_eq!(extension_request["method"], "extension.listTargets");
        send_json(
            &mut extension,
            json!({
                "type":"response","id":extension_request["id"],
                "result":{"targets":[{"id":"target_one","title":"Shared","url":"https://example.test/","kind":"page"}]}
            }),
        )
        .await;
        let targets = receive_json(&mut browser).await;
        assert_eq!(targets["result"]["targets"][0]["id"], "target_one");

        send_json(
            &mut browser,
            json!({
                "type":"request","id":3,"method":"bridge.subscribe",
                "params":{"subscription_id":"sub_one","session_id":"session_one","methods":["Runtime.consoleAPICalled"]}
            }),
        )
        .await;
        let subscribe = receive_json(&mut extension).await;
        assert_eq!(subscribe["method"], "extension.subscribe");
        send_json(
            &mut extension,
            json!({"type":"response","id":subscribe["id"],"result":{}}),
        )
        .await;
        assert_eq!(receive_json(&mut browser).await["id"], 3);
        send_json(
            &mut extension,
            json!({
                "type":"event","method":"extension.cdpEvent",
                "params":{"subscription_id":"sub_one","session_id":"session_one","method":"Runtime.consoleAPICalled","params":{"type":"log"}}
            }),
        )
        .await;
        let event = receive_json(&mut browser).await;
        assert_eq!(event["method"], "cdp.event");
        assert_eq!(event["params"]["subscription_id"], "sub_one");

        send_json(
            &mut browser,
            json!({"type":"request","id":4,"method":"bridge.close","params":{}}),
        )
        .await;
        let end_session = receive_json(&mut extension).await;
        assert_eq!(end_session["method"], "extension.endSession");
        send_json(
            &mut extension,
            json!({"type":"response","id":end_session["id"],"result":{}}),
        )
        .await;
        assert_eq!(receive_json(&mut browser).await["id"], 4);
        let _ = shutdown_tx.send(());
        server_task.await.unwrap().unwrap();
    }

    async fn connect(
        endpoint: &str,
        subprotocol: &'static str,
    ) -> async_tungstenite::WebSocketStream<async_tungstenite::tokio::ConnectStream> {
        let mut request = endpoint.into_client_request().unwrap();
        request.headers_mut().insert(
            SEC_WEBSOCKET_PROTOCOL,
            HeaderValue::from_static(subprotocol),
        );
        let (socket, response) = connect_async(request).await.unwrap();
        assert_eq!(
            response
                .headers()
                .get(SEC_WEBSOCKET_PROTOCOL)
                .unwrap()
                .to_str()
                .unwrap(),
            subprotocol
        );
        socket
    }

    async fn send_json<S>(socket: &mut WebSocketStream<S>, value: Value)
    where
        S: futures::AsyncRead + futures::AsyncWrite + Unpin,
    {
        socket.send(Message::text(value.to_string())).await.unwrap();
    }

    async fn receive_json<S>(socket: &mut WebSocketStream<S>) -> Value
    where
        S: futures::AsyncRead + futures::AsyncWrite + Unpin,
    {
        loop {
            match socket.next().await.unwrap().unwrap() {
                Message::Text(text) => return serde_json::from_str(&text).unwrap(),
                Message::Ping(_) | Message::Pong(_) => continue,
                message => panic!("unexpected WebSocket message: {message:?}"),
            }
        }
    }
}
