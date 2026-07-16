// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::io::Cursor;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chatos_sandbox_contract::{NetworkDomainPermission, NetworkProxyMode, NetworkRequirements};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tracing::warn;
use url::{Host, Url};

use crate::network_proxy_mitm::{MitmAuthority, CUSTOM_CA_ENV_KEYS};

#[cfg(target_os = "linux")]
use tokio::net::UnixListener;
#[cfg(unix)]
use tokio::net::UnixStream;

const MAX_HTTP_HEADER_BYTES: usize = 64 * 1024;
const MAX_CHUNK_LINE_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct NetworkProxyEndpoints {
    http_port: u16,
    socks_port: Option<u16>,
    ca_bundle_path: Option<PathBuf>,
    #[cfg(target_os = "linux")]
    http_bridge_socket: PathBuf,
    #[cfg(target_os = "linux")]
    socks_bridge_socket: Option<PathBuf>,
}

impl NetworkProxyEndpoints {
    #[cfg(target_os = "macos")]
    pub(crate) fn loopback_ports(&self) -> impl Iterator<Item = u16> + '_ {
        std::iter::once(self.http_port).chain(self.socks_port)
    }

    pub(crate) fn apply_to_command(&self, command: &mut tokio::process::Command) {
        let http_proxy = format!("http://127.0.0.1:{}", self.http_port);
        for name in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "WS_PROXY",
            "WSS_PROXY",
            "http_proxy",
            "https_proxy",
            "ws_proxy",
            "wss_proxy",
        ] {
            command.env(name, http_proxy.as_str());
        }
        command.env("NO_PROXY", "").env("no_proxy", "");
        if let Some(port) = self.socks_port {
            let socks_proxy = format!("socks5h://127.0.0.1:{port}");
            command
                .env("ALL_PROXY", socks_proxy.as_str())
                .env("all_proxy", socks_proxy);
        } else {
            command.env_remove("ALL_PROXY").env_remove("all_proxy");
        }
        if let Some(path) = self.ca_bundle_path.as_ref() {
            for name in CUSTOM_CA_ENV_KEYS {
                command.env(name, path);
            }
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn linux_wrapper_arguments(&self) -> Vec<String> {
        let mut args = vec![
            "--internal-network-proxy-wrapper".to_string(),
            "--http-port".to_string(),
            self.http_port.to_string(),
            "--http-socket".to_string(),
            self.http_bridge_socket.to_string_lossy().to_string(),
        ];
        if let (Some(port), Some(socket)) = (self.socks_port, self.socks_bridge_socket.as_ref()) {
            args.extend([
                "--socks-port".to_string(),
                port.to_string(),
                "--socks-socket".to_string(),
                socket.to_string_lossy().to_string(),
            ]);
        }
        args.push("--".to_string());
        args
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn linux_bridge_directories(&self) -> impl Iterator<Item = &Path> {
        let http_bridge = self
            .http_bridge_socket
            .parent()
            .expect("HTTP bridge parent");
        let ca_bundle = self
            .ca_bundle_path
            .as_deref()
            .and_then(|path| path.parent());
        std::iter::once(http_bridge).chain(ca_bundle)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct NetworkProxyRuntime {
    endpoints: NetworkProxyEndpoints,
    _lifetime: Arc<NetworkProxyLifetime>,
}

impl NetworkProxyRuntime {
    pub(crate) async fn start(
        state_dir: &Path,
        requirements: &NetworkRequirements,
    ) -> Result<Option<Self>, String> {
        let _ = state_dir;
        if requirements.enabled != Some(true) {
            return Ok(None);
        }
        if requirements.enable_socks5_udp == Some(true) {
            return Err(
                "SOCKS5 UDP is not yet supported by the native ChatOS network proxy".to_string(),
            );
        }

        let policy = Arc::new(NetworkProxyPolicy::from_requirements(requirements)?);
        let mitm = if policy.mode == NetworkProxyMode::Limited {
            Some(Arc::new(MitmAuthority::new(state_dir)?))
        } else {
            None
        };
        let http_listener =
            TcpListener::bind((Ipv4Addr::LOCALHOST, requirements.http_port.unwrap_or(0)))
                .await
                .map_err(|err| format!("bind sandbox HTTP proxy failed: {err}"))?;
        let http_port = http_listener
            .local_addr()
            .map_err(|err| format!("read sandbox HTTP proxy address failed: {err}"))?
            .port();

        let socks_listener = if requirements.enable_socks5.unwrap_or(true) {
            Some(
                TcpListener::bind((Ipv4Addr::LOCALHOST, requirements.socks_port.unwrap_or(0)))
                    .await
                    .map_err(|err| format!("bind sandbox SOCKS5 proxy failed: {err}"))?,
            )
        } else {
            None
        };
        let socks_port = socks_listener
            .as_ref()
            .map(|listener| listener.local_addr().map(|address| address.port()))
            .transpose()
            .map_err(|err| format!("read sandbox SOCKS5 proxy address failed: {err}"))?;

        let mut tasks = vec![tokio::spawn(run_http_listener(
            http_listener,
            policy.clone(),
            mitm.clone(),
        ))];
        if let Some(listener) = socks_listener {
            tasks.push(tokio::spawn(run_socks_listener(
                listener,
                policy.clone(),
                mitm.clone(),
            )));
        }

        let artifact_paths = mitm
            .as_ref()
            .map(|authority| vec![authority.ca_bundle_path().to_path_buf()])
            .unwrap_or_default();

        #[cfg(target_os = "linux")]
        let mut socket_paths = Vec::new();
        #[cfg(not(target_os = "linux"))]
        let socket_paths = Vec::new();
        #[cfg(target_os = "linux")]
        let mut socket_directories = Vec::new();
        #[cfg(not(target_os = "linux"))]
        let socket_directories = Vec::new();
        #[cfg(target_os = "linux")]
        let (http_bridge_socket, socks_bridge_socket) = {
            use std::os::unix::fs::PermissionsExt;

            // Linux sockaddr_un paths are limited to roughly 108 bytes. Lease/workspace paths can
            // be much longer, so allocate a short private directory directly below /tmp instead
            // of deriving the socket path from the project or state directory.
            let bridge_dir = PathBuf::from("/tmp").join(format!(
                "chatos-proxy-{}-{}",
                std::process::id(),
                &uuid::Uuid::new_v4().simple().to_string()[..12]
            ));
            std::fs::create_dir_all(&bridge_dir)
                .map_err(|err| format!("create network proxy bridge directory failed: {err}"))?;
            std::fs::set_permissions(&bridge_dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|err| format!("secure network proxy bridge directory failed: {err}"))?;
            socket_directories.push(bridge_dir.clone());

            let http_socket = bridge_dir.join("http.sock");
            let http_bridge = bind_unix_listener(http_socket.as_path()).await?;
            tasks.push(tokio::spawn(run_host_bridge(
                http_bridge,
                SocketAddr::from((Ipv4Addr::LOCALHOST, http_port)),
            )));
            socket_paths.push(http_socket.clone());

            let socks_socket = if let Some(port) = socks_port {
                let path = bridge_dir.join("socks5.sock");
                let listener = bind_unix_listener(path.as_path()).await?;
                tasks.push(tokio::spawn(run_host_bridge(
                    listener,
                    SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
                )));
                socket_paths.push(path.clone());
                Some(path)
            } else {
                None
            };
            (http_socket, socks_socket)
        };

        let endpoints = NetworkProxyEndpoints {
            http_port,
            socks_port,
            ca_bundle_path: mitm
                .as_ref()
                .map(|authority| authority.ca_bundle_path().to_path_buf()),
            #[cfg(target_os = "linux")]
            http_bridge_socket,
            #[cfg(target_os = "linux")]
            socks_bridge_socket,
        };
        Ok(Some(Self {
            endpoints,
            _lifetime: Arc::new(NetworkProxyLifetime {
                tasks: Mutex::new(tasks),
                socket_paths,
                socket_directories,
                artifact_paths,
            }),
        }))
    }

    pub(crate) fn endpoints(&self) -> &NetworkProxyEndpoints {
        &self.endpoints
    }
}

#[derive(Debug)]
struct NetworkProxyLifetime {
    tasks: Mutex<Vec<JoinHandle<()>>>,
    socket_paths: Vec<PathBuf>,
    socket_directories: Vec<PathBuf>,
    artifact_paths: Vec<PathBuf>,
}

impl Drop for NetworkProxyLifetime {
    fn drop(&mut self) {
        if let Ok(tasks) = self.tasks.lock() {
            for task in tasks.iter() {
                task.abort();
            }
        }
        for path in &self.socket_paths {
            let _ = std::fs::remove_file(path);
        }
        for path in self.socket_directories.iter().rev() {
            let _ = std::fs::remove_dir(path);
        }
        for path in &self.artifact_paths {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[derive(Debug, Clone)]
struct NetworkProxyPolicy {
    mode: NetworkProxyMode,
    allow_local_binding: bool,
    allow: Vec<DomainPattern>,
    deny: Vec<DomainPattern>,
    unix_sockets: BTreeMap<PathBuf, NetworkDomainPermission>,
    dangerously_allow_all_unix_sockets: bool,
}

impl NetworkProxyPolicy {
    fn from_requirements(requirements: &NetworkRequirements) -> Result<Self, String> {
        if requirements.allow_upstream_proxy == Some(true) {
            return Err(
                "upstream proxy chaining is not yet supported by the native ChatOS network proxy"
                    .to_string(),
            );
        }
        let mut permissions = requirements.domains.clone().unwrap_or_default();
        for host in requirements.allowed_domains.as_deref().unwrap_or_default() {
            permissions
                .entry(host.clone())
                .or_insert(NetworkDomainPermission::Allow);
        }
        for host in requirements.denied_domains.as_deref().unwrap_or_default() {
            permissions.insert(host.clone(), NetworkDomainPermission::Deny);
        }

        let mut allow = Vec::new();
        let mut deny = Vec::new();
        for (pattern, permission) in permissions {
            let pattern = DomainPattern::parse(pattern.as_str())?;
            match permission {
                NetworkDomainPermission::Allow => allow.push(pattern),
                NetworkDomainPermission::Deny => deny.push(pattern),
            }
        }
        let mut unix_sockets = BTreeMap::new();
        for (path, permission) in requirements.unix_sockets.clone().unwrap_or_default() {
            unix_sockets.insert(normalize_unix_socket_path(path.as_str())?, permission);
        }
        for path in requirements
            .allow_unix_sockets
            .as_deref()
            .unwrap_or_default()
        {
            unix_sockets
                .entry(normalize_unix_socket_path(path.as_str())?)
                .or_insert(NetworkDomainPermission::Allow);
        }
        Ok(Self {
            mode: requirements.mode.unwrap_or_default(),
            allow_local_binding: requirements.allow_local_binding.unwrap_or(false),
            allow,
            deny,
            unix_sockets,
            dangerously_allow_all_unix_sockets: requirements
                .dangerously_allow_all_unix_sockets
                .unwrap_or(false),
        })
    }

    fn authorize_unix_socket(&self, path: &str) -> Result<PathBuf, ProxyBlock> {
        let path = normalize_unix_socket_path(path).map_err(|detail| ProxyBlock {
            reason: "invalid-unix-socket",
            host: "unix-socket".to_string(),
            detail: Some(detail),
        })?;
        if self.unix_sockets.get(&path) == Some(&NetworkDomainPermission::Deny) {
            return Err(ProxyBlock::new(
                "blocked-by-denylist",
                path.to_string_lossy().to_string(),
            ));
        }
        if !self.dangerously_allow_all_unix_sockets
            && self.unix_sockets.get(&path) != Some(&NetworkDomainPermission::Allow)
        {
            return Err(ProxyBlock::new(
                "blocked-by-allowlist",
                path.to_string_lossy().to_string(),
            ));
        }
        Ok(path)
    }

    async fn connect(&self, host: &str, port: u16) -> Result<TcpStream, ProxyBlock> {
        let host = normalize_host(host).map_err(|detail| ProxyBlock {
            reason: "invalid-host",
            host: host.to_string(),
            detail: Some(detail),
        })?;
        if self
            .deny
            .iter()
            .any(|pattern| pattern.matches(host.as_str()))
        {
            return Err(ProxyBlock::new("blocked-by-denylist", host));
        }
        if !self
            .allow
            .iter()
            .any(|pattern| pattern.matches(host.as_str()))
        {
            return Err(ProxyBlock::new("blocked-by-allowlist", host));
        }

        let exact_local_allow = self
            .allow
            .iter()
            .any(|pattern| pattern.is_exact_match(host.as_str()));
        let literal_ip = unscoped_ip_literal(host.as_str())
            .unwrap_or(host.as_str())
            .parse()
            .ok();
        if let Some(ip) = literal_ip {
            if is_non_public_ip(ip) && !self.allow_local_binding && !exact_local_allow {
                return Err(ProxyBlock::new("blocked-local-destination", host));
            }
            return TcpStream::connect(SocketAddr::new(ip, port))
                .await
                .map_err(|err| ProxyBlock::io(host, err));
        }
        if host == "localhost" {
            if !self.allow_local_binding && !exact_local_allow {
                return Err(ProxyBlock::new("blocked-local-destination", host));
            }
            return TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
                .await
                .map_err(|err| ProxyBlock::io(host, err));
        }

        let resolved = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|err| ProxyBlock::io(host.clone(), err))?
            .collect::<Vec<_>>();
        if resolved.is_empty() {
            return Err(ProxyBlock::new("dns-no-addresses", host));
        }
        if !self.allow_local_binding
            && resolved
                .iter()
                .any(|address| is_non_public_ip(address.ip()))
        {
            // A hostname that resolves to any local/private destination is rejected as a unit.
            // The checked SocketAddr is also used for the actual connect, so the transport cannot
            // perform a second DNS lookup after policy evaluation.
            return Err(ProxyBlock::new("blocked-local-destination", host));
        }
        let mut last_error = None;
        for address in resolved {
            match TcpStream::connect(address).await {
                Ok(stream) => return Ok(stream),
                Err(err) => last_error = Some(err),
            }
        }
        Err(ProxyBlock::io(
            host,
            last_error.unwrap_or_else(|| std::io::Error::other("connection failed")),
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DomainPattern {
    AnyPublic,
    Exact(String),
    Subdomains(String),
    ApexAndSubdomains(String),
}

impl DomainPattern {
    fn parse(pattern: &str) -> Result<Self, String> {
        let pattern = pattern.trim();
        if pattern == "*" {
            return Ok(Self::AnyPublic);
        }
        if let Some(suffix) = pattern.strip_prefix("**.") {
            return Ok(Self::ApexAndSubdomains(normalize_host(suffix)?));
        }
        if let Some(suffix) = pattern.strip_prefix("*.") {
            return Ok(Self::Subdomains(normalize_host(suffix)?));
        }
        if pattern.contains('*') {
            return Err(format!("unsupported network domain wildcard: {pattern:?}"));
        }
        Ok(Self::Exact(normalize_host(pattern)?))
    }

    fn matches(&self, host: &str) -> bool {
        match self {
            Self::AnyPublic => true,
            Self::Exact(pattern) => host == pattern,
            Self::Subdomains(suffix) => {
                host.len() > suffix.len()
                    && host.ends_with(suffix)
                    && host.as_bytes()[host.len() - suffix.len() - 1] == b'.'
            }
            Self::ApexAndSubdomains(suffix) => {
                host == suffix
                    || (host.len() > suffix.len()
                        && host.ends_with(suffix)
                        && host.as_bytes()[host.len() - suffix.len() - 1] == b'.')
            }
        }
    }

    fn is_exact_match(&self, host: &str) -> bool {
        matches!(self, Self::Exact(pattern) if pattern == host)
    }
}

#[derive(Debug)]
struct ProxyBlock {
    reason: &'static str,
    host: String,
    detail: Option<String>,
}

impl ProxyBlock {
    fn new(reason: &'static str, host: String) -> Self {
        Self {
            reason,
            host,
            detail: None,
        }
    }

    fn io(host: String, error: impl std::fmt::Display) -> Self {
        Self {
            reason: "upstream-connection-failed",
            host,
            detail: Some(error.to_string()),
        }
    }
}

async fn run_http_listener(
    listener: TcpListener,
    policy: Arc<NetworkProxyPolicy>,
    mitm: Option<Arc<MitmAuthority>>,
) {
    loop {
        let Ok((stream, client)) = listener.accept().await else {
            return;
        };
        let policy = policy.clone();
        let mitm = mitm.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_http_connection(stream, policy, mitm).await {
                warn!(client = %client, error = %err, "sandbox HTTP proxy connection failed");
            }
        });
    }
}

async fn handle_http_connection(
    mut client: TcpStream,
    policy: Arc<NetworkProxyPolicy>,
    mitm: Option<Arc<MitmAuthority>>,
) -> Result<(), String> {
    let header = read_http_header(&mut client).await?;
    let parsed = ParsedHttpRequest::parse(header.as_slice())?;
    if parsed.method == "CONNECT" {
        let (host, port) = parse_authority(parsed.target.as_str(), 443)?;
        let host = normalize_host(host.as_str())?;
        let mut upstream = match policy.connect(host.as_str(), port).await {
            Ok(stream) => stream,
            Err(block) => return write_proxy_block(&mut client, block).await,
        };
        client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await
            .map_err(|err| err.to_string())?;
        if policy.mode == NetworkProxyMode::Limited {
            let mitm = mitm.ok_or_else(|| {
                "limited sandbox network mode is missing its HTTPS MITM authority".to_string()
            })?;
            return handle_mitm_https(client, upstream, host.as_str(), port, mitm).await;
        }
        tokio::io::copy_bidirectional(&mut client, &mut upstream)
            .await
            .map_err(|err| err.to_string())?;
        return Ok(());
    }

    if policy.mode == NetworkProxyMode::Limited
        && !matches!(parsed.method.as_str(), "GET" | "HEAD" | "OPTIONS")
    {
        return write_http_block(&mut client, "blocked-by-method-policy").await;
    }
    let destination = parsed.destination()?;
    #[cfg(unix)]
    if let Some(socket_path) = header_value(parsed.headers.as_slice(), "x-unix-socket") {
        let socket_path = match policy.authorize_unix_socket(socket_path) {
            Ok(path) => path,
            Err(block) => return write_proxy_block(&mut client, block).await,
        };
        let upstream = match UnixStream::connect(socket_path.as_path()).await {
            Ok(stream) => stream,
            Err(err) => {
                return write_proxy_block(
                    &mut client,
                    ProxyBlock::io(socket_path.to_string_lossy().to_string(), err),
                )
                .await;
            }
        };
        return forward_http_request(
            &mut client,
            upstream,
            &parsed,
            header.as_slice(),
            destination.origin_form.as_str(),
        )
        .await;
    }
    #[cfg(not(unix))]
    if header_value(parsed.headers.as_slice(), "x-unix-socket").is_some() {
        return write_http_block(&mut client, "unix-socket-unsupported").await;
    }
    let upstream = match policy
        .connect(destination.host.as_str(), destination.port)
        .await
    {
        Ok(stream) => stream,
        Err(block) => return write_proxy_block(&mut client, block).await,
    };

    forward_http_request(
        &mut client,
        upstream,
        &parsed,
        header.as_slice(),
        destination.origin_form.as_str(),
    )
    .await
}

async fn handle_mitm_https(
    client: TcpStream,
    upstream: TcpStream,
    host: &str,
    port: u16,
    mitm: Arc<MitmAuthority>,
) -> Result<(), String> {
    let mut client = mitm.accept_client(host, client).await?;
    let header = read_http_header(&mut client).await?;
    let parsed = ParsedHttpRequest::parse(header.as_slice())?;
    if !matches!(parsed.method.as_str(), "GET" | "HEAD" | "OPTIONS") {
        write_http_block(&mut client, "blocked-by-method-policy").await?;
        client.shutdown().await.map_err(|err| err.to_string())?;
        return Ok(());
    }
    if header_value(parsed.headers.as_slice(), "x-unix-socket").is_some() {
        write_http_block(&mut client, "unix-socket-not-allowed-over-https").await?;
        client.shutdown().await.map_err(|err| err.to_string())?;
        return Ok(());
    }
    let origin_form = match parsed.https_origin_form(host, port) {
        Ok(origin_form) => origin_form,
        Err(err) => {
            warn!(host, port, error = %err, "sandbox HTTPS request target validation failed");
            write_http_block(&mut client, "https-host-mismatch").await?;
            client.shutdown().await.map_err(|err| err.to_string())?;
            return Ok(());
        }
    };
    let upstream = mitm.connect_upstream(host, upstream).await?;
    let result = forward_http_request(
        &mut client,
        upstream,
        &parsed,
        header.as_slice(),
        origin_form.as_str(),
    )
    .await;
    let shutdown = client.shutdown().await.map_err(|err| err.to_string());
    result?;
    shutdown
}

async fn forward_http_request<C, S>(
    client: &mut C,
    mut upstream: S,
    parsed: &ParsedHttpRequest,
    header: &[u8],
    origin_form: &str,
) -> Result<(), String>
where
    C: AsyncRead + AsyncWrite + Unpin,
    S: AsyncRead + AsyncWrite + Unpin,
{
    upstream
        .write_all(parsed.forward_header(origin_form).as_bytes())
        .await
        .map_err(|err| err.to_string())?;
    let body_prefix = header[parsed.header_len..].to_vec();
    let mut body_reader = BufReader::new(Cursor::new(body_prefix).chain(&mut *client));
    if parsed.chunked {
        relay_chunked_body(&mut body_reader, &mut upstream).await?;
    } else if let Some(content_length) = parsed.content_length {
        let mut body = body_reader.take(content_length);
        let copied = tokio::io::copy(&mut body, &mut upstream)
            .await
            .map_err(|err| err.to_string())?;
        if copied != content_length {
            return Err("HTTP proxy request body ended before Content-Length".to_string());
        }
    }
    upstream.shutdown().await.map_err(|err| err.to_string())?;
    if let Err(err) = tokio::io::copy(&mut upstream, &mut *client).await {
        // A significant number of real HTTPS servers close TCP without a TLS close_notify.
        // Once the HTTP response bytes have been relayed, rustls reports this as UnexpectedEof;
        // treating that transport close as end-of-response matches browser/curl behavior.
        if err.kind() != std::io::ErrorKind::UnexpectedEof {
            return Err(err.to_string());
        }
        warn!("sandbox HTTPS upstream closed without TLS close_notify");
    }
    Ok(())
}

async fn read_http_header<S>(stream: &mut S) -> Result<Vec<u8>, String>
where
    S: AsyncRead + Unpin,
{
    let mut bytes = Vec::with_capacity(2048);
    let mut buffer = [0_u8; 2048];
    loop {
        let read = stream
            .read(&mut buffer)
            .await
            .map_err(|err| err.to_string())?;
        if read == 0 {
            return Err("HTTP proxy client closed before sending headers".to_string());
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > MAX_HTTP_HEADER_BYTES {
            return Err("HTTP proxy request headers exceed 64 KiB".to_string());
        }
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            return Ok(bytes);
        }
    }
}

#[derive(Debug)]
struct ParsedHttpRequest {
    method: String,
    target: String,
    version: u8,
    headers: Vec<(String, String)>,
    header_len: usize,
    content_length: Option<u64>,
    chunked: bool,
}

impl ParsedHttpRequest {
    fn parse(bytes: &[u8]) -> Result<Self, String> {
        let mut headers = [httparse::EMPTY_HEADER; 128];
        let mut request = httparse::Request::new(&mut headers);
        let header_len = match request.parse(bytes).map_err(|err| err.to_string())? {
            httparse::Status::Complete(length) => length,
            httparse::Status::Partial => return Err("incomplete HTTP proxy request".to_string()),
        };
        let method = request
            .method
            .ok_or_else(|| "HTTP proxy request is missing a method".to_string())?
            .to_ascii_uppercase();
        let target = request
            .path
            .ok_or_else(|| "HTTP proxy request is missing a target".to_string())?
            .to_string();
        let version = request.version.unwrap_or(1);
        let headers = request
            .headers
            .iter()
            .map(|header| {
                let value = std::str::from_utf8(header.value)
                    .map_err(|_| "HTTP proxy header is not valid UTF-8".to_string())?;
                Ok((header.name.to_string(), value.trim().to_string()))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let content_length = unique_header_value(headers.as_slice(), "content-length")?
            .map(str::parse::<u64>)
            .transpose()
            .map_err(|_| "invalid Content-Length header".to_string())?;
        let transfer_encoding = unique_header_value(headers.as_slice(), "transfer-encoding")?;
        let chunked = match transfer_encoding {
            None => false,
            Some(value) if value.trim().eq_ignore_ascii_case("chunked") => true,
            Some(_) => {
                return Err(
                    "HTTP proxy only supports an exact Transfer-Encoding: chunked value"
                        .to_string(),
                )
            }
        };
        let _ = unique_header_value(headers.as_slice(), "host")?;
        let _ = unique_header_value(headers.as_slice(), "x-unix-socket")?;
        if chunked && content_length.is_some() {
            return Err("HTTP proxy rejects ambiguous body framing".to_string());
        }
        Ok(Self {
            method,
            target,
            version,
            headers,
            header_len,
            content_length,
            chunked,
        })
    }

    fn destination(&self) -> Result<HttpDestination, String> {
        if let Ok(url) = Url::parse(self.target.as_str()) {
            if url.scheme() != "http" {
                return Err("non-CONNECT proxy requests must use http:// URLs".to_string());
            }
            let host = url
                .host_str()
                .ok_or_else(|| "HTTP proxy URL is missing a host".to_string())?;
            let port = url.port_or_known_default().unwrap_or(80);
            if let Some(host_header) = header_value(self.headers.as_slice(), "host") {
                let (header_host, header_port) = parse_authority(host_header, port)?;
                if normalize_host(header_host.as_str())? != normalize_host(host)?
                    || header_port != port
                {
                    return Err("HTTP proxy URL and Host header disagree".to_string());
                }
            }
            let mut origin_form = url.path().to_string();
            if origin_form.is_empty() {
                origin_form.push('/');
            }
            if let Some(query) = url.query() {
                origin_form.push('?');
                origin_form.push_str(query);
            }
            return Ok(HttpDestination {
                host: host.to_string(),
                port,
                origin_form,
            });
        }

        let host_header = header_value(self.headers.as_slice(), "host")
            .ok_or_else(|| "origin-form HTTP proxy request is missing Host".to_string())?;
        let (host, port) = parse_authority(host_header, 80)?;
        Ok(HttpDestination {
            host,
            port,
            origin_form: self.target.clone(),
        })
    }

    fn https_origin_form(&self, expected_host: &str, expected_port: u16) -> Result<String, String> {
        let expected_host = normalize_host(expected_host)?;
        let host_header = header_value(self.headers.as_slice(), "host")
            .ok_or_else(|| "HTTPS request inside CONNECT is missing Host".to_string())?;
        let (header_host, header_port) = parse_authority(host_header, expected_port)?;
        if normalize_host(header_host.as_str())? != expected_host || header_port != expected_port {
            return Err("HTTPS Host header disagrees with the CONNECT target".to_string());
        }

        if let Ok(url) = Url::parse(self.target.as_str()) {
            if url.scheme() != "https" {
                return Err("absolute HTTPS proxy request must use an https:// URL".to_string());
            }
            let host = url
                .host_str()
                .ok_or_else(|| "absolute HTTPS proxy URL is missing a host".to_string())?;
            let port = url.port_or_known_default().unwrap_or(443);
            if normalize_host(host)? != expected_host || port != expected_port {
                return Err("absolute HTTPS proxy URL disagrees with CONNECT target".to_string());
            }
            let mut origin_form = url.path().to_string();
            if origin_form.is_empty() {
                origin_form.push('/');
            }
            if let Some(query) = url.query() {
                origin_form.push('?');
                origin_form.push_str(query);
            }
            return Ok(origin_form);
        }

        if self.target == "*" && self.method == "OPTIONS" {
            return Ok(self.target.clone());
        }
        if !self.target.starts_with('/') || self.target.starts_with("//") {
            return Err("HTTPS request target must use origin-form".to_string());
        }
        Ok(self.target.clone())
    }

    fn forward_header(&self, origin_form: &str) -> String {
        let mut output = format!(
            "{} {} HTTP/1.{}\r\n",
            self.method, origin_form, self.version
        );
        let connection_headers = header_value(self.headers.as_slice(), "connection")
            .map(|value| {
                value
                    .split(',')
                    .map(|name| name.trim().to_ascii_lowercase())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (name, value) in &self.headers {
            if is_hop_by_hop_header(name)
                || connection_headers
                    .iter()
                    .any(|candidate| name.eq_ignore_ascii_case(candidate))
            {
                continue;
            }
            output.push_str(name);
            output.push_str(": ");
            output.push_str(value);
            output.push_str("\r\n");
        }
        output.push_str("Connection: close\r\n\r\n");
        output
    }
}

#[derive(Debug)]
struct HttpDestination {
    host: String,
    port: u16,
    origin_form: String,
}

fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn unique_header_value<'a>(
    headers: &'a [(String, String)],
    name: &str,
) -> Result<Option<&'a str>, String> {
    let mut values = headers
        .iter()
        .filter(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str());
    let first = values.next();
    if values.next().is_some() {
        return Err(format!("HTTP proxy rejects duplicate {name} headers"));
    }
    Ok(first)
}

fn is_hop_by_hop_header(name: &str) -> bool {
    [
        "connection",
        "proxy-connection",
        "proxy-authorization",
        "proxy-authenticate",
        "keep-alive",
        "upgrade",
        "x-unix-socket",
    ]
    .iter()
    .any(|candidate| name.eq_ignore_ascii_case(candidate))
}

async fn relay_chunked_body<R, W>(reader: &mut R, writer: &mut W) -> Result<(), String>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    loop {
        let mut line = Vec::new();
        let read = reader
            .read_until(b'\n', &mut line)
            .await
            .map_err(|err| err.to_string())?;
        if read == 0 || line.len() > MAX_CHUNK_LINE_BYTES {
            return Err("invalid chunked HTTP request body".to_string());
        }
        writer
            .write_all(&line)
            .await
            .map_err(|err| err.to_string())?;
        let size_text = std::str::from_utf8(&line)
            .map_err(|_| "invalid chunk size line".to_string())?
            .trim()
            .split(';')
            .next()
            .unwrap_or_default();
        let size =
            u64::from_str_radix(size_text, 16).map_err(|_| "invalid chunk size".to_string())?;
        if size == 0 {
            loop {
                line.clear();
                let read = reader
                    .read_until(b'\n', &mut line)
                    .await
                    .map_err(|err| err.to_string())?;
                if read == 0 || line.len() > MAX_HTTP_HEADER_BYTES {
                    return Err("invalid chunk trailer".to_string());
                }
                writer
                    .write_all(&line)
                    .await
                    .map_err(|err| err.to_string())?;
                if line == b"\r\n" {
                    return Ok(());
                }
            }
        }
        let mut chunk = reader.take(size + 2);
        let copied = tokio::io::copy(&mut chunk, writer)
            .await
            .map_err(|err| err.to_string())?;
        if copied != size + 2 {
            return Err("chunked HTTP request body ended early".to_string());
        }
    }
}

async fn write_proxy_block<W>(client: &mut W, block: ProxyBlock) -> Result<(), String>
where
    W: AsyncWrite + Unpin,
{
    warn!(host = %block.host, reason = block.reason, detail = ?block.detail, "sandbox network request blocked");
    write_http_block(client, block.reason).await
}

async fn write_http_block<W>(client: &mut W, reason: &str) -> Result<(), String>
where
    W: AsyncWrite + Unpin,
{
    let body = format!("request blocked by sandbox network policy: {reason}\n");
    let response = format!(
        "HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nx-proxy-error: {}\r\n\r\n{}",
        body.len(),
        reason,
        body
    );
    client
        .write_all(response.as_bytes())
        .await
        .map_err(|err| err.to_string())
}

async fn run_socks_listener(
    listener: TcpListener,
    policy: Arc<NetworkProxyPolicy>,
    mitm: Option<Arc<MitmAuthority>>,
) {
    loop {
        let Ok((stream, client)) = listener.accept().await else {
            return;
        };
        let policy = policy.clone();
        let mitm = mitm.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_socks_connection(stream, policy, mitm).await {
                warn!(client = %client, error = %err, "sandbox SOCKS5 proxy connection failed");
            }
        });
    }
}

async fn handle_socks_connection(
    mut client: TcpStream,
    policy: Arc<NetworkProxyPolicy>,
    mitm: Option<Arc<MitmAuthority>>,
) -> Result<(), String> {
    let version = client.read_u8().await.map_err(|err| err.to_string())?;
    if version != 5 {
        return Err("unsupported SOCKS version".to_string());
    }
    let method_count = client.read_u8().await.map_err(|err| err.to_string())? as usize;
    let mut methods = vec![0_u8; method_count];
    client
        .read_exact(&mut methods)
        .await
        .map_err(|err| err.to_string())?;
    if !methods.contains(&0) {
        client
            .write_all(&[5, 0xff])
            .await
            .map_err(|err| err.to_string())?;
        return Ok(());
    }
    client
        .write_all(&[5, 0])
        .await
        .map_err(|err| err.to_string())?;

    let version = client.read_u8().await.map_err(|err| err.to_string())?;
    let command = client.read_u8().await.map_err(|err| err.to_string())?;
    let reserved = client.read_u8().await.map_err(|err| err.to_string())?;
    let address_type = client.read_u8().await.map_err(|err| err.to_string())?;
    if version != 5 || reserved != 0 || command != 1 {
        send_socks_reply(&mut client, 7).await?;
        return Ok(());
    }
    let host = match address_type {
        1 => {
            let mut bytes = [0_u8; 4];
            client
                .read_exact(&mut bytes)
                .await
                .map_err(|err| err.to_string())?;
            Ipv4Addr::from(bytes).to_string()
        }
        3 => {
            let length = client.read_u8().await.map_err(|err| err.to_string())? as usize;
            let mut bytes = vec![0_u8; length];
            client
                .read_exact(&mut bytes)
                .await
                .map_err(|err| err.to_string())?;
            String::from_utf8(bytes).map_err(|_| "SOCKS domain is not UTF-8".to_string())?
        }
        4 => {
            let mut bytes = [0_u8; 16];
            client
                .read_exact(&mut bytes)
                .await
                .map_err(|err| err.to_string())?;
            Ipv6Addr::from(bytes).to_string()
        }
        _ => {
            send_socks_reply(&mut client, 8).await?;
            return Ok(());
        }
    };
    let port = client.read_u16().await.map_err(|err| err.to_string())?;
    if policy.mode == NetworkProxyMode::Limited && port != 443 {
        send_socks_reply(&mut client, 7).await?;
        return Ok(());
    }
    let host = normalize_host(host.as_str())?;
    let mut upstream = match policy.connect(host.as_str(), port).await {
        Ok(stream) => stream,
        Err(block) => {
            warn!(host = %block.host, reason = block.reason, detail = ?block.detail, "sandbox SOCKS5 request blocked");
            send_socks_reply(&mut client, 2).await?;
            return Ok(());
        }
    };
    send_socks_reply(&mut client, 0).await?;
    if policy.mode == NetworkProxyMode::Limited {
        let mitm = mitm.ok_or_else(|| {
            "limited sandbox network mode is missing its HTTPS MITM authority".to_string()
        })?;
        return handle_mitm_https(client, upstream, host.as_str(), port, mitm).await;
    }
    tokio::io::copy_bidirectional(&mut client, &mut upstream)
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

async fn send_socks_reply(client: &mut TcpStream, code: u8) -> Result<(), String> {
    client
        .write_all(&[5, code, 0, 1, 0, 0, 0, 0, 0, 0])
        .await
        .map_err(|err| err.to_string())
}

fn parse_authority(authority: &str, default_port: u16) -> Result<(String, u16), String> {
    let candidate = if authority.contains("://") {
        authority.to_string()
    } else {
        format!("http://{authority}")
    };
    let url = Url::parse(candidate.as_str())
        .map_err(|_| format!("invalid network authority: {authority:?}"))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(format!("invalid network authority: {authority:?}"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| format!("network authority has no host: {authority:?}"))?;
    Ok((host.to_string(), url.port().unwrap_or(default_port)))
}

fn normalize_host(input: &str) -> Result<String, String> {
    let input = input.trim().trim_end_matches('.');
    if input.is_empty() || input.contains('\0') || input.contains('/') || input.contains('@') {
        return Err(format!("invalid network host: {input:?}"));
    }
    let unscoped = unscoped_ip_literal(input).unwrap_or(input);
    if let Ok(ip) = unscoped.parse::<IpAddr>() {
        return Ok(ip.to_string().to_ascii_lowercase());
    }
    Host::parse(unscoped)
        .map(|host| host.to_string().to_ascii_lowercase())
        .map_err(|_| format!("invalid network host: {input:?}"))
}

fn normalize_unix_socket_path(input: &str) -> Result<PathBuf, String> {
    let path = Path::new(input.trim());
    if !path.is_absolute() || input.contains('\0') {
        return Err("unix socket paths must be absolute and must not contain NUL".to_string());
    }
    if path.exists() {
        return path
            .canonicalize()
            .map_err(|err| format!("canonicalize unix socket path failed: {err}"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| "unix socket path has no parent".to_string())?;
    let parent = parent
        .canonicalize()
        .map_err(|err| format!("canonicalize unix socket parent failed: {err}"))?;
    let name = path
        .file_name()
        .ok_or_else(|| "unix socket path has no filename".to_string())?;
    Ok(parent.join(name))
}

fn unscoped_ip_literal(host: &str) -> Option<&str> {
    host.strip_prefix('[')?.strip_suffix(']')
}

fn is_non_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ipv4_in_cidr(ip, [0, 0, 0, 0], 8)
                || ipv4_in_cidr(ip, [100, 64, 0, 0], 10)
                || ipv4_in_cidr(ip, [192, 0, 0, 0], 24)
                || ipv4_in_cidr(ip, [192, 0, 2, 0], 24)
                || ipv4_in_cidr(ip, [198, 18, 0, 0], 15)
                || ipv4_in_cidr(ip, [198, 51, 100, 0], 24)
                || ipv4_in_cidr(ip, [203, 0, 113, 0], 24)
                || ipv4_in_cidr(ip, [240, 0, 0, 0], 4)
        }
        IpAddr::V6(ip) => {
            if let Some(ipv4) = ip.to_ipv4_mapped() {
                return is_non_public_ip(IpAddr::V4(ipv4));
            }
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || (ip.segments()[0] & 0xfe00) == 0xfc00
                || (ip.segments()[0] & 0xffc0) == 0xfe80
                || (ip.segments()[0] & 0xffc0) == 0xfec0
                || (ip.segments()[0] & 0xffc0) == 0x0000
        }
    }
}

fn ipv4_in_cidr(ip: Ipv4Addr, base: [u8; 4], prefix: u8) -> bool {
    let ip = u32::from(ip);
    let base = u32::from(Ipv4Addr::from(base));
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    (ip & mask) == (base & mask)
}

#[cfg(target_os = "linux")]
async fn bind_unix_listener(path: &Path) -> Result<UnixListener, String> {
    if path.exists() {
        std::fs::remove_file(path).map_err(|err| err.to_string())?;
    }
    UnixListener::bind(path)
        .map_err(|err| format!("bind proxy bridge {} failed: {err}", path.display()))
}

#[cfg(target_os = "linux")]
async fn run_host_bridge(listener: UnixListener, endpoint: SocketAddr) {
    loop {
        let Ok((mut unix, _)) = listener.accept().await else {
            return;
        };
        tokio::spawn(async move {
            let Ok(mut tcp) = TcpStream::connect(endpoint).await else {
                return;
            };
            let _ = tokio::io::copy_bidirectional(&mut unix, &mut tcp).await;
        });
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn is_internal_network_proxy_wrapper() -> bool {
    std::env::args().nth(1).as_deref() == Some("--internal-network-proxy-wrapper")
}

#[cfg(target_os = "linux")]
pub(crate) fn is_internal_command_wrapper() -> bool {
    std::env::args().nth(1).as_deref() == Some("--internal-command-wrapper")
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn is_internal_command_wrapper() -> bool {
    false
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn is_internal_network_proxy_wrapper() -> bool {
    false
}

#[cfg(target_os = "linux")]
pub(crate) async fn run_internal_network_proxy_wrapper() -> Result<(), String> {
    let spec = LinuxProxyWrapperSpec::from_args(std::env::args().skip(2).collect())?;
    ensure_loopback_interface_up()
        .map_err(|err| format!("enable sandbox loopback failed: {err}"))?;

    let http_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, spec.http_port))
        .await
        .map_err(|err| format!("bind sandbox HTTP proxy bridge failed: {err}"))?;
    let mut tasks = vec![tokio::spawn(run_local_bridge(
        http_listener,
        spec.http_socket,
    ))];
    if let (Some(port), Some(socket)) = (spec.socks_port, spec.socks_socket) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .await
            .map_err(|err| format!("bind sandbox SOCKS5 proxy bridge failed: {err}"))?;
        tasks.push(tokio::spawn(run_local_bridge(listener, socket)));
    }

    let status = run_seccomp_wrapped_command(spec.command).await?;
    for task in tasks {
        task.abort();
    }
    std::process::exit(status.code().unwrap_or(1));
}

#[cfg(target_os = "linux")]
pub(crate) async fn run_internal_command_wrapper() -> Result<(), String> {
    let args = std::env::args().skip(2).collect::<Vec<_>>();
    let command = args
        .strip_prefix(&["--".to_string()])
        .ok_or_else(|| "command wrapper is missing command separator".to_string())?
        .to_vec();
    let status = run_seccomp_wrapped_command(command).await?;
    std::process::exit(status.code().unwrap_or(1));
}

#[cfg(not(target_os = "linux"))]
pub(crate) async fn run_internal_command_wrapper() -> Result<(), String> {
    Err("command wrapper is only available on Linux".to_string())
}

#[cfg(target_os = "linux")]
async fn run_seccomp_wrapped_command(
    command_parts: Vec<String>,
) -> Result<std::process::ExitStatus, String> {
    use std::os::unix::process::CommandExt;

    let executable = command_parts
        .first()
        .ok_or_else(|| "sandbox command wrapper is missing a command".to_string())?;
    let mut command = tokio::process::Command::new(executable);
    command.args(&command_parts[1..]);
    unsafe {
        command
            .as_std_mut()
            .pre_exec(install_no_unix_socket_seccomp);
    }
    command.status().await.map_err(|err| err.to_string())
}

#[cfg(target_os = "linux")]
fn install_no_unix_socket_seccomp() -> std::io::Result<()> {
    const BPF_LD_W_ABS: u16 = 0x20;
    const BPF_JMP_JEQ_K: u16 = 0x15;
    const BPF_RET_K: u16 = 0x06;
    const SECCOMP_RET_KILL_PROCESS: u32 = 0x8000_0000;
    const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
    const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
    const SECCOMP_SET_MODE_FILTER: libc::c_uint = 1;
    const SECCOMP_DATA_ARCH_OFFSET: u32 = 4;
    const SECCOMP_DATA_NR_OFFSET: u32 = 0;
    const SECCOMP_DATA_ARG0_OFFSET: u32 = 16;

    #[cfg(target_arch = "x86_64")]
    const AUDIT_ARCH: u32 = 0xc000_003e;
    #[cfg(target_arch = "aarch64")]
    const AUDIT_ARCH: u32 = 0xc000_00b7;
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    return Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "seccomp Unix-socket filter is unsupported on this Linux architecture",
    ));

    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    {
        let mut filters = [
            libc::sock_filter {
                code: BPF_LD_W_ABS,
                jt: 0,
                jf: 0,
                k: SECCOMP_DATA_ARCH_OFFSET,
            },
            libc::sock_filter {
                code: BPF_JMP_JEQ_K,
                jt: 1,
                jf: 0,
                k: AUDIT_ARCH,
            },
            libc::sock_filter {
                code: BPF_RET_K,
                jt: 0,
                jf: 0,
                k: SECCOMP_RET_KILL_PROCESS,
            },
            libc::sock_filter {
                code: BPF_LD_W_ABS,
                jt: 0,
                jf: 0,
                k: SECCOMP_DATA_NR_OFFSET,
            },
            libc::sock_filter {
                code: BPF_JMP_JEQ_K,
                jt: 0,
                jf: 3,
                k: libc::SYS_socket as u32,
            },
            libc::sock_filter {
                code: BPF_LD_W_ABS,
                jt: 0,
                jf: 0,
                k: SECCOMP_DATA_ARG0_OFFSET,
            },
            libc::sock_filter {
                code: BPF_JMP_JEQ_K,
                jt: 0,
                jf: 1,
                k: libc::AF_UNIX as u32,
            },
            libc::sock_filter {
                code: BPF_RET_K,
                jt: 0,
                jf: 0,
                k: SECCOMP_RET_ERRNO | libc::EACCES as u32,
            },
            libc::sock_filter {
                code: BPF_RET_K,
                jt: 0,
                jf: 0,
                k: SECCOMP_RET_ALLOW,
            },
        ];
        let program = libc::sock_fprog {
            len: filters.len() as u16,
            filter: filters.as_mut_ptr(),
        };
        if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        if unsafe {
            libc::syscall(
                libc::SYS_seccomp,
                SECCOMP_SET_MODE_FILTER,
                0,
                &program as *const libc::sock_fprog,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) async fn run_internal_network_proxy_wrapper() -> Result<(), String> {
    Err("network proxy wrapper is only available on Linux".to_string())
}

#[cfg(target_os = "linux")]
struct LinuxProxyWrapperSpec {
    http_port: u16,
    http_socket: PathBuf,
    socks_port: Option<u16>,
    socks_socket: Option<PathBuf>,
    command: Vec<String>,
}

#[cfg(target_os = "linux")]
impl LinuxProxyWrapperSpec {
    fn from_args(args: Vec<String>) -> Result<Self, String> {
        let separator = args
            .iter()
            .position(|arg| arg == "--")
            .ok_or_else(|| "network proxy wrapper is missing command separator".to_string())?;
        let options = &args[..separator];
        let command = args[separator + 1..].to_vec();
        if command.is_empty() {
            return Err("network proxy wrapper is missing a command".to_string());
        }
        let values = options
            .chunks_exact(2)
            .map(|pair| (pair[0].as_str(), pair[1].clone()))
            .collect::<BTreeMap<_, _>>();
        if !options.len().is_multiple_of(2) {
            return Err("network proxy wrapper options must be key/value pairs".to_string());
        }
        let http_port = values
            .get("--http-port")
            .ok_or_else(|| "network proxy wrapper is missing HTTP port".to_string())?
            .parse()
            .map_err(|_| "network proxy wrapper HTTP port is invalid".to_string())?;
        let http_socket = PathBuf::from(
            values
                .get("--http-socket")
                .ok_or_else(|| "network proxy wrapper is missing HTTP socket".to_string())?,
        );
        let socks_port = values
            .get("--socks-port")
            .map(|value| value.parse())
            .transpose()
            .map_err(|_| "network proxy wrapper SOCKS port is invalid".to_string())?;
        let socks_socket = values.get("--socks-socket").map(PathBuf::from);
        if socks_port.is_some() != socks_socket.is_some() {
            return Err("network proxy wrapper SOCKS route is incomplete".to_string());
        }
        Ok(Self {
            http_port,
            http_socket,
            socks_port,
            socks_socket,
            command,
        })
    }
}

#[cfg(target_os = "linux")]
async fn run_local_bridge(listener: TcpListener, socket: PathBuf) {
    loop {
        let Ok((mut tcp, _)) = listener.accept().await else {
            return;
        };
        let socket = socket.clone();
        tokio::spawn(async move {
            let Ok(mut unix) = UnixStream::connect(socket).await else {
                return;
            };
            let _ = tokio::io::copy_bidirectional(&mut tcp, &mut unix).await;
        });
    }
}

#[cfg(target_os = "linux")]
fn ensure_loopback_interface_up() -> std::io::Result<()> {
    const LOOPBACK_INTERFACE_NAME: &[u8] = b"lo";
    let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut ifreq = unsafe { std::mem::zeroed::<libc::ifreq>() };
    for (index, byte) in LOOPBACK_INTERFACE_NAME.iter().copied().enumerate() {
        ifreq.ifr_name[index] = byte as libc::c_char;
    }
    let result = unsafe { libc::ioctl(fd, libc::SIOCGIFFLAGS as libc::Ioctl, &mut ifreq) };
    if result < 0 {
        let err = std::io::Error::last_os_error();
        unsafe { libc::close(fd) };
        return Err(err);
    }
    let flags = unsafe { ifreq.ifr_ifru.ifru_flags };
    if flags & libc::IFF_UP as libc::c_short == 0 {
        ifreq.ifr_ifru.ifru_flags = flags | libc::IFF_UP as libc::c_short;
        let result = unsafe { libc::ioctl(fd, libc::SIOCSIFFLAGS as libc::Ioctl, &ifreq) };
        if result < 0 {
            let err = std::io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(err);
        }
    }
    unsafe { libc::close(fd) };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::{
        BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
        KeyUsagePurpose, PKCS_ECDSA_P256_SHA256,
    };
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
    use rustls::{ClientConfig, RootCertStore, ServerConfig};
    use std::collections::BTreeMap;
    use std::io::BufReader as StdBufReader;
    use std::sync::Arc;
    #[cfg(unix)]
    use tokio::net::UnixListener;
    use tokio_rustls::{TlsAcceptor, TlsConnector};

    fn requirements(entries: &[(&str, NetworkDomainPermission)]) -> NetworkRequirements {
        NetworkRequirements {
            enabled: Some(true),
            domains: Some(
                entries
                    .iter()
                    .map(|(host, permission)| ((*host).to_string(), *permission))
                    .collect(),
            ),
            ..Default::default()
        }
    }

    fn test_upstream_tls_acceptor() -> (TlsAcceptor, CertificateDer<'static>) {
        let mut ca_params = CertificateParams::default();
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let mut distinguished_name = DistinguishedName::new();
        distinguished_name.push(DnType::CommonName, "ChatOS network proxy test CA");
        ca_params.distinguished_name = distinguished_name;
        let ca_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("test CA key");
        let ca_certificate = ca_params.self_signed(&ca_key).expect("test CA certificate");
        let ca_der = ca_certificate.der().clone();
        let issuer = Issuer::new(ca_params, ca_key);

        let leaf_params =
            CertificateParams::new(vec!["localhost".to_string()]).expect("leaf params");
        let leaf_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("leaf key");
        let leaf_certificate = leaf_params
            .signed_by(&leaf_key, &issuer)
            .expect("leaf certificate");
        let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der()));
        let mut server_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![leaf_certificate.der().clone()], private_key)
            .expect("TLS server config");
        server_config.alpn_protocols = vec![b"http/1.1".to_vec()];
        (TlsAcceptor::from(Arc::new(server_config)), ca_der)
    }

    fn client_tls_connector(bundle_path: &Path) -> TlsConnector {
        let file = std::fs::File::open(bundle_path).expect("open managed trust bundle");
        let mut reader = StdBufReader::new(file);
        let certificates = rustls_pemfile::certs(&mut reader)
            .collect::<Result<Vec<_>, _>>()
            .expect("parse managed trust bundle");
        let mut roots = RootCertStore::empty();
        let (added, _ignored) = roots.add_parsable_certificates(certificates);
        assert!(added > 0);
        let mut config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        TlsConnector::from(Arc::new(config))
    }

    #[test]
    fn wildcard_semantics_and_deny_precedence_match_codex_profiles() {
        let policy = NetworkProxyPolicy::from_requirements(&requirements(&[
            ("**.example.com", NetworkDomainPermission::Allow),
            ("private.example.com", NetworkDomainPermission::Deny),
        ]))
        .expect("policy");
        assert!(policy
            .allow
            .iter()
            .any(|pattern| pattern.matches("example.com")));
        assert!(policy
            .allow
            .iter()
            .any(|pattern| pattern.matches("api.example.com")));
        assert!(policy
            .deny
            .iter()
            .any(|pattern| pattern.matches("private.example.com")));

        let subdomains = DomainPattern::parse("*.example.com").expect("subdomains");
        assert!(!subdomains.matches("example.com"));
        assert!(subdomains.matches("api.example.com"));
    }

    #[test]
    fn private_and_special_ip_ranges_are_classified_as_non_public() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "100.64.0.1",
            "169.254.1.1",
            "192.0.2.1",
            "198.18.0.1",
            "224.0.0.1",
            "::1",
            "fe80::1",
            "fc00::1",
        ] {
            assert!(is_non_public_ip(ip.parse().expect("IP")), "{ip}");
        }
        assert!(!is_non_public_ip("1.1.1.1".parse().expect("IP")));
        assert!(!is_non_public_ip(
            "2606:4700:4700::1111".parse().expect("IP")
        ));
    }

    #[tokio::test]
    async fn explicit_literal_allow_is_required_for_loopback() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("loopback listener");
        let port = listener.local_addr().expect("address").port();

        let wildcard = NetworkProxyPolicy::from_requirements(&requirements(&[(
            "*",
            NetworkDomainPermission::Allow,
        )]))
        .expect("wildcard policy");
        assert!(wildcard.connect("127.0.0.1", port).await.is_err());

        let exact = NetworkProxyPolicy::from_requirements(&requirements(&[(
            "127.0.0.1",
            NetworkDomainPermission::Allow,
        )]))
        .expect("exact policy");
        assert!(exact.connect("127.0.0.1", port).await.is_ok());
    }

    #[tokio::test]
    async fn http_proxy_enforces_allowlist_and_forwards_allowed_request() {
        let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("upstream");
        let upstream_port = upstream.local_addr().expect("upstream address").port();
        tokio::spawn(async move {
            let (mut stream, _) = upstream.accept().await.expect("accept upstream");
            let mut request = vec![0_u8; 4096];
            let _ = stream.read(&mut request).await.expect("read request");
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .expect("write response");
        });

        let root = std::env::temp_dir().join(format!(
            "chatos-network-proxy-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("state root");
        let runtime = NetworkProxyRuntime::start(
            root.as_path(),
            &NetworkRequirements {
                enabled: Some(true),
                domains: Some(BTreeMap::from([(
                    "127.0.0.1".to_string(),
                    NetworkDomainPermission::Allow,
                )])),
                enable_socks5: Some(false),
                ..Default::default()
            },
        )
        .await
        .expect("start proxy")
        .expect("enabled proxy");

        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, runtime.endpoints.http_port))
            .await
            .expect("connect proxy");
        client
            .write_all(
                format!(
                    "GET http://127.0.0.1:{upstream_port}/health HTTP/1.1\r\nHost: 127.0.0.1:{upstream_port}\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .expect("write proxy request");
        let mut response = String::new();
        client
            .read_to_string(&mut response)
            .await
            .expect("read proxy response");
        assert!(response.contains("200 OK"), "{response}");
        assert!(response.ends_with("ok"), "{response}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn https_inner_request_rejects_host_mismatch_and_ambiguous_targets() {
        let mismatched =
            ParsedHttpRequest::parse(b"GET / HTTP/1.1\r\nHost: other.example.com\r\n\r\n")
                .expect("request");
        assert!(mismatched.https_origin_form("example.com", 443).is_err());

        let network_path = ParsedHttpRequest::parse(
            b"GET //other.example.com/path HTTP/1.1\r\nHost: example.com\r\n\r\n",
        )
        .expect("request");
        assert!(network_path.https_origin_form("example.com", 443).is_err());
    }

    #[tokio::test]
    async fn limited_https_connect_mitm_allows_get() {
        let (upstream_acceptor, upstream_ca) = test_upstream_tls_acceptor();
        let upstream_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("upstream listener");
        let upstream_port = upstream_listener.local_addr().expect("address").port();
        let upstream_task = tokio::spawn(async move {
            let (stream, _) = upstream_listener.accept().await.expect("accept upstream");
            let mut stream = upstream_acceptor
                .accept(stream)
                .await
                .expect("upstream TLS");
            let header = read_http_header(&mut stream)
                .await
                .expect("upstream request");
            let request = ParsedHttpRequest::parse(header.as_slice()).expect("parse request");
            assert_eq!(request.method, "GET");
            assert_eq!(request.target, "/health");
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .expect("write upstream response");
            stream.shutdown().await.expect("shutdown upstream TLS");
        });

        let state = tempfile::tempdir().expect("state");
        let mitm = Arc::new(
            MitmAuthority::new_with_additional_roots(state.path(), vec![upstream_ca])
                .expect("MITM authority"),
        );
        let policy = Arc::new(
            NetworkProxyPolicy::from_requirements(&NetworkRequirements {
                enabled: Some(true),
                mode: Some(NetworkProxyMode::Limited),
                domains: Some(BTreeMap::from([(
                    "localhost".to_string(),
                    NetworkDomainPermission::Allow,
                )])),
                enable_socks5: Some(false),
                ..Default::default()
            })
            .expect("policy"),
        );
        let proxy_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("proxy listener");
        let proxy_port = proxy_listener.local_addr().expect("address").port();
        let handler_mitm = mitm.clone();
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.expect("accept proxy client");
            handle_http_connection(stream, policy, Some(handler_mitm)).await
        });

        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, proxy_port))
            .await
            .expect("connect proxy");
        client
            .write_all(
                format!(
                    "CONNECT localhost:{upstream_port} HTTP/1.1\r\nHost: localhost:{upstream_port}\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .expect("write CONNECT");
        let response = read_http_header(&mut client)
            .await
            .expect("CONNECT response");
        assert!(String::from_utf8_lossy(response.as_slice()).contains("200 Connection Established"));

        let connector = client_tls_connector(mitm.ca_bundle_path());
        let server_name = ServerName::try_from("localhost".to_string()).expect("server name");
        let mut client = connector
            .connect(server_name, client)
            .await
            .expect("client TLS");
        client
            .write_all(
                format!(
                    "GET /health HTTP/1.1\r\nHost: localhost:{upstream_port}\r\nConnection: close\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .expect("write HTTPS request");
        let mut response = String::new();
        client
            .read_to_string(&mut response)
            .await
            .expect("read HTTPS response");
        assert!(response.contains("200 OK"), "{response}");
        assert!(response.ends_with("ok"), "{response}");
        proxy_task.await.expect("proxy task").expect("proxy result");
        upstream_task.await.expect("upstream task");
    }

    #[tokio::test]
    async fn limited_https_connect_mitm_blocks_post_before_upstream_tls() {
        let upstream_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("upstream listener");
        let upstream_port = upstream_listener.local_addr().expect("address").port();
        let upstream_task = tokio::spawn(async move {
            let (mut stream, _) = upstream_listener.accept().await.expect("accept upstream");
            let mut byte = [0_u8; 1];
            let read = stream.read(&mut byte).await.expect("read upstream");
            assert_eq!(read, 0, "method policy must run before upstream TLS");
        });

        let state = tempfile::tempdir().expect("state");
        let mitm = Arc::new(MitmAuthority::new(state.path()).expect("MITM authority"));
        let policy = Arc::new(
            NetworkProxyPolicy::from_requirements(&NetworkRequirements {
                enabled: Some(true),
                mode: Some(NetworkProxyMode::Limited),
                domains: Some(BTreeMap::from([(
                    "127.0.0.1".to_string(),
                    NetworkDomainPermission::Allow,
                )])),
                enable_socks5: Some(false),
                ..Default::default()
            })
            .expect("policy"),
        );
        let proxy_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("proxy listener");
        let proxy_port = proxy_listener.local_addr().expect("address").port();
        let handler_mitm = mitm.clone();
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.expect("accept proxy client");
            handle_http_connection(stream, policy, Some(handler_mitm)).await
        });

        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, proxy_port))
            .await
            .expect("connect proxy");
        client
            .write_all(
                format!(
                    "CONNECT 127.0.0.1:{upstream_port} HTTP/1.1\r\nHost: 127.0.0.1:{upstream_port}\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .expect("write CONNECT");
        let response = read_http_header(&mut client)
            .await
            .expect("CONNECT response");
        assert!(String::from_utf8_lossy(response.as_slice()).contains("200 Connection Established"));

        let connector = client_tls_connector(mitm.ca_bundle_path());
        let server_name = ServerName::try_from("127.0.0.1".to_string()).expect("server name");
        let mut client = connector
            .connect(server_name, client)
            .await
            .expect("client TLS");
        client
            .write_all(
                format!(
                    "POST /upload HTTP/1.1\r\nHost: 127.0.0.1:{upstream_port}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .expect("write HTTPS request");
        let mut response = String::new();
        client
            .read_to_string(&mut response)
            .await
            .expect("read HTTPS response");
        assert!(response.contains("403 Forbidden"), "{response}");
        assert!(response.contains("blocked-by-method-policy"), "{response}");
        proxy_task.await.expect("proxy task").expect("proxy result");
        upstream_task.await.expect("upstream task");
    }

    #[tokio::test]
    async fn limited_socks_rejects_non_https_ports() {
        let policy = Arc::new(
            NetworkProxyPolicy::from_requirements(&NetworkRequirements {
                enabled: Some(true),
                mode: Some(NetworkProxyMode::Limited),
                domains: Some(BTreeMap::from([(
                    "127.0.0.1".to_string(),
                    NetworkDomainPermission::Allow,
                )])),
                ..Default::default()
            })
            .expect("policy"),
        );
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("SOCKS listener");
        let port = listener.local_addr().expect("address").port();
        let handler = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept SOCKS client");
            handle_socks_connection(stream, policy, None).await
        });

        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .expect("connect SOCKS");
        client
            .write_all(&[5, 1, 0])
            .await
            .expect("write SOCKS methods");
        let mut method_reply = [0_u8; 2];
        client
            .read_exact(&mut method_reply)
            .await
            .expect("read SOCKS method reply");
        assert_eq!(method_reply, [5, 0]);

        client
            .write_all(&[5, 1, 0, 1, 127, 0, 0, 1, 0, 80])
            .await
            .expect("write SOCKS request");
        let mut reply = [0_u8; 10];
        client
            .read_exact(&mut reply)
            .await
            .expect("read SOCKS reply");
        assert_eq!(reply[1], 7);
        handler
            .await
            .expect("handler task")
            .expect("handler result");
    }

    #[tokio::test]
    async fn limited_proxy_injects_and_cleans_up_managed_ca_bundle() {
        let state = tempfile::tempdir().expect("state");
        let runtime = NetworkProxyRuntime::start(
            state.path(),
            &NetworkRequirements {
                enabled: Some(true),
                mode: Some(NetworkProxyMode::Limited),
                domains: Some(BTreeMap::from([(
                    "example.com".to_string(),
                    NetworkDomainPermission::Allow,
                )])),
                enable_socks5: Some(false),
                ..Default::default()
            },
        )
        .await
        .expect("start proxy")
        .expect("enabled proxy");
        let bundle_path = runtime
            .endpoints
            .ca_bundle_path
            .clone()
            .expect("managed CA bundle");
        assert!(bundle_path.is_file());

        let mut command = tokio::process::Command::new("/usr/bin/true");
        runtime.endpoints.apply_to_command(&mut command);
        let environment = command
            .as_std()
            .get_envs()
            .filter_map(|(name, value)| {
                value.map(|value| {
                    (
                        name.to_string_lossy().to_string(),
                        value.to_string_lossy().to_string(),
                    )
                })
            })
            .collect::<BTreeMap<_, _>>();
        for name in CUSTOM_CA_ENV_KEYS {
            assert_eq!(
                environment.get(name).map(String::as_str),
                Some(bundle_path.to_string_lossy().as_ref()),
                "{name}"
            );
        }

        drop(runtime);
        assert!(!bundle_path.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn http_proxy_enforces_unix_socket_allowlist() {
        let root = std::env::temp_dir().join(format!(
            "chatos-network-unix-proxy-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("state root");
        let socket_path = PathBuf::from(format!(
            "/tmp/chatos-unix-proxy-{}.sock",
            uuid::Uuid::new_v4()
        ));
        let listener = UnixListener::bind(socket_path.as_path()).expect("unix listener");
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept unix request");
            let mut request = vec![0_u8; 4096];
            let _ = stream.read(&mut request).await.expect("read unix request");
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nunix")
                .await
                .expect("write unix response");
        });

        let runtime = NetworkProxyRuntime::start(
            root.as_path(),
            &NetworkRequirements {
                enabled: Some(true),
                unix_sockets: Some(BTreeMap::from([(
                    socket_path.to_string_lossy().to_string(),
                    NetworkDomainPermission::Allow,
                )])),
                enable_socks5: Some(false),
                ..Default::default()
            },
        )
        .await
        .expect("start proxy")
        .expect("enabled proxy");

        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, runtime.endpoints.http_port))
            .await
            .expect("connect proxy");
        client
            .write_all(
                format!(
                    "GET http://unix.local/health HTTP/1.1\r\nHost: unix.local\r\nx-unix-socket: {}\r\n\r\n",
                    socket_path.display()
                )
                .as_bytes(),
            )
            .await
            .expect("write proxy request");
        let mut response = String::new();
        client
            .read_to_string(&mut response)
            .await
            .expect("read proxy response");
        assert!(response.contains("200 OK"), "{response}");
        assert!(response.ends_with("unix"), "{response}");
        let _ = std::fs::remove_file(socket_path);
        let _ = std::fs::remove_dir_all(root);
    }
}
