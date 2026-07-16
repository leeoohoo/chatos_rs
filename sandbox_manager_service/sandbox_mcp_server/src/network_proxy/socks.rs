// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::address::*;
use super::http::handle_mitm_https;
use super::policy::*;
use super::*;

pub(super) async fn run_socks_listener(
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

pub(super) async fn handle_socks_connection(
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
