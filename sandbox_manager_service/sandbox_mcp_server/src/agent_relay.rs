// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(unix)]
use std::net::Ipv4Addr;
#[cfg(unix)]
use tokio::io::copy_bidirectional;
#[cfg(unix)]
use tokio::net::{TcpListener, UnixStream};

#[cfg(unix)]
const AGENT_PORT: u16 = 49_888;
const RELAY_FLAG: &str = "--internal-agent-relay";
#[cfg(any(unix, test))]
const AGENT_SOCKET_PATH: &str = "/run/chatos-agent/agent.sock";

pub(crate) fn is_internal_agent_relay() -> bool {
    std::env::args().nth(1).as_deref() == Some(RELAY_FLAG)
}

#[cfg(unix)]
pub(crate) async fn run_internal_agent_relay() -> Result<(), String> {
    let target = std::env::args()
        .nth(2)
        .ok_or_else(|| "internal agent relay requires a target Unix socket".to_string())?;
    validate_agent_socket_path(target.as_str())?;
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, AGENT_PORT))
        .await
        .map_err(|err| format!("bind internal agent relay failed: {err}"))?;
    loop {
        let (mut client, _) = listener
            .accept()
            .await
            .map_err(|err| format!("accept internal agent relay connection failed: {err}"))?;
        let target = target.clone();
        tokio::spawn(async move {
            let result = async {
                let mut agent = UnixStream::connect(target.as_str()).await.map_err(|err| {
                    format!("connect isolated sandbox agent socket failed: {err}")
                })?;
                copy_bidirectional(&mut client, &mut agent)
                    .await
                    .map_err(|err| format!("relay isolated sandbox agent traffic failed: {err}"))?;
                Ok::<(), String>(())
            }
            .await;
            if let Err(err) = result {
                eprintln!("[chatos-sandbox-agent-relay] {err}");
            }
        });
    }
}

#[cfg(not(unix))]
pub(crate) async fn run_internal_agent_relay() -> Result<(), String> {
    Err("internal agent relay requires Unix domain socket support".to_string())
}

#[cfg(any(unix, test))]
fn validate_agent_socket_path(value: &str) -> Result<(), String> {
    if value != AGENT_SOCKET_PATH {
        return Err("internal agent relay target is not the managed agent socket".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_accepts_only_the_managed_agent_socket() {
        assert!(validate_agent_socket_path(AGENT_SOCKET_PATH).is_ok());
        assert!(validate_agent_socket_path("/tmp/agent.sock").is_err());
        assert!(validate_agent_socket_path("example.com:49888").is_err());
    }
}
