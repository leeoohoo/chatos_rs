use super::{
    authorize_loopback_request, bounded_file, health_proof, plugin_catalog_error,
    plugin_runtime_error, resolve_non_linked_file, secure_random_token, write_health_response,
    write_session_bootstrap, write_unauthorized_response, LoopbackRequestAuthorization,
    ResolvedPluginApplication, RunningPluginApplication, RuntimeTermination,
};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex as StandardMutex,
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Notify, Semaphore},
    time::{timeout, Duration},
};

const MAXIMUM_STATIC_ENTRY_BYTES: u64 = 2 * 1_024 * 1_024;
const MAXIMUM_STATIC_ASSET_BYTES: u64 = 8 * 1_024 * 1_024;
const MAXIMUM_STATIC_TOTAL_BYTES: u64 = 32 * 1_024 * 1_024;
const MAXIMUM_STATIC_REQUEST_BYTES: usize = 16 * 1_024;
const MAXIMUM_STATIC_CONNECTIONS: usize = 64;

#[derive(Default)]
pub(super) struct StaticServerShutdown {
    requested: AtomicBool,
    wake: Notify,
}

impl StaticServerShutdown {
    pub(super) fn request(&self) {
        self.requested.store(true, Ordering::SeqCst);
        self.wake.notify_one();
    }
}

#[derive(Clone)]
struct StaticAsset {
    bytes: Arc<Vec<u8>>,
    content_type: &'static str,
}

pub(super) async fn spawn_static_runtime(
    resolved: &ResolvedPluginApplication,
) -> Result<Arc<RunningPluginApplication>, String> {
    let assets = Arc::new(load_static_assets(resolved)?);
    let authentication_token = secure_random_token()?;
    let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .await
        .map_err(|_| "无法启动静态 Plugin 应用服务。".to_owned())?;
    let address = listener
        .local_addr()
        .map_err(|_| "无法读取静态 Plugin 应用地址。".to_owned())?;
    let shutdown = Arc::new(StaticServerShutdown::default());
    let process = Arc::new(RunningPluginApplication {
        release_id: resolved.record.spec.release_id.clone(),
        release_digest: resolved.record.spec.release_digest.clone(),
        version: resolved.manifest.version.clone(),
        base_url: format!("http://127.0.0.1:{}/", address.port()),
        health_path: "/".into(),
        authentication_token: authentication_token.clone(),
        termination: RuntimeTermination::Static(shutdown.clone()),
        alive: AtomicBool::new(true),
        finished: Notify::new(),
        resource_permit: StandardMutex::new(None),
    });
    tokio::spawn(run_static_server(
        process.clone(),
        listener,
        assets,
        authentication_token,
        shutdown,
    ));
    Ok(process)
}

fn load_static_assets(
    resolved: &ResolvedPluginApplication,
) -> Result<HashMap<String, StaticAsset>, String> {
    let source_path = resolved.contribution.source.path();
    let source = static_asset(
        &resolved.installation_root,
        source_path,
        MAXIMUM_STATIC_ENTRY_BYTES,
    )?;
    if source.content_type != "text/html; charset=utf-8" {
        return Err(plugin_catalog_error());
    }
    let mut total_bytes = source.bytes.len() as u64;
    let mut routes = HashMap::new();
    routes.insert("/".to_owned(), source.clone());
    insert_static_route(&mut routes, encoded_static_route(source_path)?, source)?;

    let source_directory = source_path
        .rsplit_once('/')
        .map(|(directory, _)| directory)
        .ok_or_else(plugin_catalog_error)?;
    let source_prefix = format!("{source_directory}/");
    for reference in &resolved.contribution.assets {
        let path = reference.path();
        let asset = static_asset(
            &resolved.installation_root,
            path,
            MAXIMUM_STATIC_ASSET_BYTES,
        )?;
        total_bytes = total_bytes
            .checked_add(asset.bytes.len() as u64)
            .filter(|total| *total <= MAXIMUM_STATIC_TOTAL_BYTES)
            .ok_or_else(plugin_catalog_error)?;
        insert_static_route(&mut routes, encoded_static_route(path)?, asset.clone())?;
        if let Some(relative) = path.strip_prefix(&source_prefix) {
            insert_static_route(&mut routes, encoded_static_route(relative)?, asset)?;
        }
    }
    Ok(routes)
}

fn static_asset(root: &Path, relative: &str, maximum: u64) -> Result<StaticAsset, String> {
    let path = resolve_non_linked_file(root, relative)?;
    let content_type = static_content_type(&path).ok_or_else(plugin_catalog_error)?;
    Ok(StaticAsset {
        bytes: Arc::new(bounded_file(&path, maximum)?),
        content_type,
    })
}

fn static_content_type(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "html" => Some("text/html; charset=utf-8"),
        "js" | "mjs" => Some("text/javascript; charset=utf-8"),
        "css" => Some("text/css; charset=utf-8"),
        "json" => Some("application/json; charset=utf-8"),
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "woff" => Some("font/woff"),
        "woff2" => Some("font/woff2"),
        _ => None,
    }
}

fn encoded_static_route(relative: &str) -> Result<String, String> {
    let relative = relative.strip_prefix("./").unwrap_or(relative);
    let base = reqwest::Url::parse("http://127.0.0.1/").map_err(|_| plugin_catalog_error())?;
    let url = base.join(relative).map_err(|_| plugin_catalog_error())?;
    if url.host_str() != Some("127.0.0.1") || url.path().contains("..") {
        return Err(plugin_catalog_error());
    }
    Ok(url.path().to_owned())
}

fn insert_static_route(
    routes: &mut HashMap<String, StaticAsset>,
    route: String,
    asset: StaticAsset,
) -> Result<(), String> {
    if route == "/" || routes.insert(route, asset).is_some() {
        return Err(plugin_catalog_error());
    }
    Ok(())
}

async fn run_static_server(
    process: Arc<RunningPluginApplication>,
    listener: TcpListener,
    assets: Arc<HashMap<String, StaticAsset>>,
    authentication_token: String,
    shutdown: Arc<StaticServerShutdown>,
) {
    let connections = Arc::new(Semaphore::new(MAXIMUM_STATIC_CONNECTIONS));
    loop {
        if shutdown.requested.load(Ordering::SeqCst) {
            break;
        }
        tokio::select! {
            _ = shutdown.wake.notified() => break,
            accepted = listener.accept() => {
                let Ok((stream, peer)) = accepted else { break };
                if !peer.ip().is_loopback() {
                    continue;
                }
                let Ok(permit) = connections.clone().try_acquire_owned() else {
                    continue;
                };
                let assets = assets.clone();
                let authentication_token = authentication_token.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    let _ = serve_static_request(stream, &assets, &authentication_token).await;
                });
            }
        }
    }
    process.alive.store(false, Ordering::SeqCst);
    process.release_resource();
    process.finished.notify_waiters();
}

async fn serve_static_request(
    mut stream: TcpStream,
    assets: &HashMap<String, StaticAsset>,
    authentication_token: &str,
) -> Result<(), String> {
    let mut request = Vec::with_capacity(1_024);
    let mut buffer = [0_u8; 1_024];
    loop {
        let count = timeout(Duration::from_secs(2), stream.read(&mut buffer))
            .await
            .map_err(|_| plugin_runtime_error())?
            .map_err(|_| plugin_runtime_error())?;
        if count == 0 {
            return Err(plugin_runtime_error());
        }
        request.extend_from_slice(&buffer[..count]);
        if request.len() > MAXIMUM_STATIC_REQUEST_BYTES {
            return write_static_response(
                &mut stream,
                "431 Request Header Fields Too Large",
                None,
                false,
            )
            .await;
        }
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let request_text = std::str::from_utf8(&request).map_err(|_| plugin_runtime_error())?;
    let first_line = request_text.lines().next().unwrap_or_default().trim_end();
    let mut parts = first_line.split_ascii_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    let version = parts.next().unwrap_or_default();
    if parts.next().is_some() || !matches!(version, "HTTP/1.0" | "HTTP/1.1") {
        return write_static_response(&mut stream, "400 Bad Request", None, false).await;
    }
    match authorize_loopback_request(request_text, target, authentication_token, "/") {
        Some(LoopbackRequestAuthorization::Bootstrap) => {
            return write_session_bootstrap(&mut stream, authentication_token).await;
        }
        Some(LoopbackRequestAuthorization::Health(challenge)) => {
            return write_health_response(
                &mut stream,
                &health_proof(authentication_token, &challenge),
            )
            .await;
        }
        Some(LoopbackRequestAuthorization::Session) => {}
        None => return write_unauthorized_response(&mut stream).await,
    }
    if !matches!(method, "GET" | "HEAD") {
        return write_static_response(&mut stream, "405 Method Not Allowed", None, false).await;
    }
    let route = target.split_once('?').map_or(target, |(path, _)| path);
    if route.contains('#') || !route.starts_with('/') || route.contains("..") {
        return write_static_response(&mut stream, "400 Bad Request", None, false).await;
    }
    match assets.get(route) {
        Some(asset) => {
            write_static_response(&mut stream, "200 OK", Some(asset), method == "HEAD").await
        }
        None => write_static_response(&mut stream, "404 Not Found", None, method == "HEAD").await,
    }
}

async fn write_static_response(
    stream: &mut TcpStream,
    status: &str,
    asset: Option<&StaticAsset>,
    head_only: bool,
) -> Result<(), String> {
    let empty = Vec::new();
    let bytes = asset.map_or(empty.as_slice(), |asset| asset.bytes.as_slice());
    let content_type = asset.map_or("text/plain; charset=utf-8", |asset| asset.content_type);
    let headers = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: {content_type}\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'none'; form-action 'none'; navigate-to 'self'; base-uri 'none'\r\nCross-Origin-Resource-Policy: same-origin\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        bytes.len()
    );
    stream
        .write_all(headers.as_bytes())
        .await
        .map_err(|_| plugin_runtime_error())?;
    if !head_only {
        stream
            .write_all(bytes)
            .await
            .map_err(|_| plugin_runtime_error())?;
    }
    let _ = stream.shutdown().await;
    Ok(())
}
