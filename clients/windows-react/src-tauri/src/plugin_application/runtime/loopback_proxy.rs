use super::{
    plugin_runtime_error, PLUGIN_AUTHENTICATION_HEADER, PLUGIN_HEALTH_CHALLENGE_HEADER,
    PLUGIN_HEALTH_PROOF_HEADER, PLUGIN_SESSION_BOOTSTRAP_PATH, PLUGIN_SESSION_COOKIE,
};
use ring::hmac;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWrite, AsyncWriteExt, BufStream},
    net::{TcpListener, TcpStream},
    sync::{Notify, Semaphore},
    time::{timeout, Duration},
};

const MAXIMUM_LOOPBACK_PROXY_CONNECTIONS: usize = 64;
const MAXIMUM_LOOPBACK_PROXY_HEADER_BYTES: usize = 256 * 1_024;
const LOOPBACK_PROXY_CONNECT_TIMEOUT_MS: u64 = 700;
const LOOPBACK_PROXY_IDLE_TIMEOUT_MS: u64 = 300_000;

#[derive(Default)]
pub(super) struct LoopbackProxyShutdown {
    requested: AtomicBool,
    wake: Notify,
}

impl LoopbackProxyShutdown {
    pub(super) fn request(&self) {
        self.requested.store(true, Ordering::SeqCst);
        self.wake.notify_waiters();
    }
}

pub(super) async fn start_loopback_proxy(
    proxy_port: u16,
    child_port: u16,
    health_path: String,
    authentication_token: String,
) -> Result<Arc<LoopbackProxyShutdown>, String> {
    let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), proxy_port))
        .await
        .map_err(|_| "无法启动 Plugin 应用 loopback 代理。".to_owned())?;
    let shutdown = Arc::new(LoopbackProxyShutdown::default());
    tokio::spawn(run_loopback_proxy(
        listener,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), child_port),
        health_path,
        authentication_token,
        shutdown.clone(),
    ));
    Ok(shutdown)
}

async fn run_loopback_proxy(
    listener: TcpListener,
    target: SocketAddr,
    health_path: String,
    authentication_token: String,
    shutdown: Arc<LoopbackProxyShutdown>,
) {
    let connections = Arc::new(Semaphore::new(MAXIMUM_LOOPBACK_PROXY_CONNECTIONS));
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
                tokio::spawn(proxy_loopback_connection(
                    stream,
                    target,
                    health_path.clone(),
                    authentication_token.clone(),
                    permit,
                ));
            }
        }
    }
}

async fn proxy_loopback_connection(
    downstream: TcpStream,
    target: SocketAddr,
    health_path: String,
    authentication_token: String,
    _permit: tokio::sync::OwnedSemaphorePermit,
) {
    let mut downstream = BufStream::new(downstream);
    let Some(request_headers) = read_proxy_headers(&mut downstream).await else {
        return;
    };
    if !valid_proxy_request(&request_headers) {
        return;
    }
    let Ok(request_text) = std::str::from_utf8(&request_headers) else {
        return;
    };
    let target_path = request_text
        .lines()
        .next()
        .and_then(|line| line.split_ascii_whitespace().nth(1))
        .unwrap_or_default();
    match authorize_loopback_request(
        request_text,
        target_path,
        &authentication_token,
        &health_path,
    ) {
        Some(LoopbackRequestAuthorization::Bootstrap) => {
            let _ = write_session_bootstrap(&mut downstream, &authentication_token).await;
            return;
        }
        Some(LoopbackRequestAuthorization::Session)
        | Some(LoopbackRequestAuthorization::Health(_)) => {}
        None => {
            let _ = write_unauthorized_response(&mut downstream).await;
            return;
        }
    }
    let Some(request_headers) =
        inject_authentication_header(&request_headers, &authentication_token)
    else {
        return;
    };
    let Ok(Ok(upstream)) = timeout(
        Duration::from_millis(LOOPBACK_PROXY_CONNECT_TIMEOUT_MS),
        TcpStream::connect(target),
    )
    .await
    else {
        return;
    };
    let mut upstream = BufStream::new(upstream);
    if upstream.write_all(&request_headers).await.is_err() || upstream.flush().await.is_err() {
        return;
    }
    let Some(response_headers) = read_proxy_headers(&mut upstream).await else {
        return;
    };
    if !valid_proxy_response(&response_headers)
        || downstream.write_all(&response_headers).await.is_err()
        || downstream.flush().await.is_err()
    {
        return;
    }
    let _ = timeout(
        Duration::from_millis(LOOPBACK_PROXY_IDLE_TIMEOUT_MS),
        tokio::io::copy_bidirectional(&mut downstream, &mut upstream),
    )
    .await;
}

pub(super) async fn read_proxy_headers(stream: &mut BufStream<TcpStream>) -> Option<Vec<u8>> {
    let mut headers = Vec::with_capacity(1_024);
    let mut buffer = [0_u8; 4_096];
    loop {
        let count = timeout(Duration::from_secs(2), stream.read(&mut buffer))
            .await
            .ok()?
            .ok()?;
        if count == 0 {
            return None;
        }
        headers.extend_from_slice(&buffer[..count]);
        if headers.windows(4).any(|window| window == b"\r\n\r\n")
            || headers.windows(2).any(|window| window == b"\n\n")
        {
            return Some(headers);
        }
        if headers.len() > MAXIMUM_LOOPBACK_PROXY_HEADER_BYTES {
            return None;
        }
    }
}

pub(super) fn valid_proxy_request(headers: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(headers) else {
        return false;
    };
    let Some(first_line) = text.lines().next() else {
        return false;
    };
    let mut parts = first_line.split_ascii_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    let version = parts.next().unwrap_or_default();
    parts.next().is_none()
        && !method.eq_ignore_ascii_case("CONNECT")
        && target.starts_with('/')
        && !target.starts_with("//")
        && matches!(version, "HTTP/1.0" | "HTTP/1.1")
}

pub(super) fn valid_proxy_response(headers: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(headers) else {
        return false;
    };
    let Some(first_line) = text.lines().next() else {
        return false;
    };
    let mut parts = first_line.split_ascii_whitespace();
    let version = parts.next().unwrap_or_default();
    let status = parts.next().unwrap_or_default();
    matches!(version, "HTTP/1.0" | "HTTP/1.1")
        && status.len() == 3
        && status.as_bytes().iter().all(u8::is_ascii_digit)
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum LoopbackRequestAuthorization {
    Bootstrap,
    Health(String),
    Session,
}

pub(super) fn authorize_loopback_request(
    request: &str,
    target: &str,
    authentication_token: &str,
    health_path: &str,
) -> Option<LoopbackRequestAuthorization> {
    let method = request.lines().next()?.split_ascii_whitespace().next()?;
    let bootstrap_prefix = format!("{PLUGIN_SESSION_BOOTSTRAP_PATH}?token=");
    if method == "GET" {
        if let Some(candidate) = target.strip_prefix(&bootstrap_prefix) {
            if !candidate.contains('&') && secure_equal(candidate, authentication_token) {
                return Some(LoopbackRequestAuthorization::Bootstrap);
            }
            return None;
        }
    }
    let path = target.split_once('?').map_or(target, |(path, _)| path);
    if path == health_path {
        if let Some(challenge) = unique_header_value(request, PLUGIN_HEALTH_CHALLENGE_HEADER)
            .filter(|challenge| valid_authentication_value(challenge))
        {
            return Some(LoopbackRequestAuthorization::Health(challenge.to_owned()));
        }
    }
    let cookie = unique_header_value(request, "cookie")?;
    let mut session_value = None;
    for part in cookie.split(';') {
        let Some((name, value)) = part.trim().split_once('=') else {
            continue;
        };
        if name != PLUGIN_SESSION_COOKIE {
            continue;
        }
        if session_value.replace(value).is_some() {
            return None;
        }
    }
    session_value
        .filter(|value| secure_equal(value, authentication_token))
        .map(|_| LoopbackRequestAuthorization::Session)
}

pub(super) fn unique_header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
    let mut found = None;
    for line in request.lines().skip(1) {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            break;
        }
        let (header_name, value) = line.split_once(':')?;
        if header_name.eq_ignore_ascii_case(name) {
            if found.is_some() {
                return None;
            }
            found = Some(value.trim());
        }
    }
    found
}

pub(super) fn valid_authentication_value(value: &str) -> bool {
    (32..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub(super) fn secure_equal(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let comparison_message = b"chatos-plugin-authentication-value";
    let right_key = hmac::Key::new(hmac::HMAC_SHA256, right.as_bytes());
    let left_key = hmac::Key::new(hmac::HMAC_SHA256, left.as_bytes());
    let left_tag = hmac::sign(&left_key, comparison_message);
    hmac::verify(&right_key, comparison_message, left_tag.as_ref()).is_ok()
}

pub(super) fn inject_authentication_header(
    headers: &[u8],
    authentication_token: &str,
) -> Option<Vec<u8>> {
    let request = std::str::from_utf8(headers).ok()?;
    if has_header(request, PLUGIN_AUTHENTICATION_HEADER) {
        return None;
    }
    let (boundary, separator) = headers
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| (position + 2, b"\r\n".as_slice()))
        .or_else(|| {
            headers
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|position| (position + 1, b"\n".as_slice()))
        })?;
    let mut authenticated = Vec::with_capacity(headers.len() + authentication_token.len() + 32);
    authenticated.extend_from_slice(&headers[..boundary]);
    authenticated.extend_from_slice(PLUGIN_AUTHENTICATION_HEADER.as_bytes());
    authenticated.extend_from_slice(b": ");
    authenticated.extend_from_slice(authentication_token.as_bytes());
    authenticated.extend_from_slice(separator);
    authenticated.extend_from_slice(&headers[boundary..]);
    Some(authenticated)
}

fn has_header(request: &str, name: &str) -> bool {
    request
        .lines()
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .any(|line| {
            line.split_once(':')
                .is_some_and(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        })
}

pub(super) async fn write_session_bootstrap(
    stream: &mut (impl AsyncWrite + Unpin),
    authentication_token: &str,
) -> Result<(), String> {
    write_loopback_response(
        stream,
        &format!(
            "HTTP/1.1 303 See Other\r\nLocation: /\r\nSet-Cookie: {PLUGIN_SESSION_COOKIE}={authentication_token}; HttpOnly; SameSite=Strict; Path=/\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ),
    )
    .await
}

pub(super) async fn write_unauthorized_response(
    stream: &mut (impl AsyncWrite + Unpin),
) -> Result<(), String> {
    write_loopback_response(
        stream,
        "HTTP/1.1 401 Unauthorized\r\nCache-Control: no-store\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    )
    .await
}

pub(super) async fn write_health_response(
    stream: &mut (impl AsyncWrite + Unpin),
    proof: &str,
) -> Result<(), String> {
    write_loopback_response(
        stream,
        &format!(
            "HTTP/1.1 200 OK\r\n{PLUGIN_HEALTH_PROOF_HEADER}: {proof}\r\nCache-Control: no-store\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ),
    )
    .await
}

async fn write_loopback_response(
    stream: &mut (impl AsyncWrite + Unpin),
    response: &str,
) -> Result<(), String> {
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|_| plugin_runtime_error())?;
    stream.flush().await.map_err(|_| plugin_runtime_error())
}
