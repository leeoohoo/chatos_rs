// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::address::*;
use super::policy::*;
use super::*;

mod request;

use self::request::header_value;
pub(in crate::network_proxy) use self::request::{read_http_header, ParsedHttpRequest};

pub(super) async fn run_http_listener(
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

pub(super) async fn handle_http_connection(
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

pub(super) async fn handle_mitm_https(
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
