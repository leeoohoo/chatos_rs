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
    #[cfg_attr(
        not(any(target_os = "linux", target_os = "macos", test)),
        allow(dead_code)
    )]
    http_port: u16,
    #[cfg_attr(
        not(any(target_os = "linux", target_os = "macos", test)),
        allow(dead_code)
    )]
    socks_port: Option<u16>,
    #[cfg_attr(
        not(any(target_os = "linux", target_os = "macos", test)),
        allow(dead_code)
    )]
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

    #[cfg_attr(
        not(any(target_os = "linux", target_os = "macos", test)),
        allow(dead_code)
    )]
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

mod address;
mod http;
mod policy;
mod socks;
#[cfg(test)]
mod tests;
mod wrapper;

use self::http::run_http_listener;
use self::policy::NetworkProxyPolicy;
use self::socks::run_socks_listener;
#[cfg(target_os = "linux")]
use self::wrapper::{bind_unix_listener, run_host_bridge};
pub(crate) use self::wrapper::{
    is_internal_command_wrapper, is_internal_network_proxy_wrapper, run_internal_command_wrapper,
    run_internal_network_proxy_wrapper,
};
#[cfg(test)]
use self::{
    address::*,
    http::{handle_http_connection, read_http_header, ParsedHttpRequest},
    policy::DomainPattern,
    socks::handle_socks_connection,
};
