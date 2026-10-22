use chatos_local_agent_protocol::{
    HostCommand, HostRequestEnvelope, HostResponseEnvelope, HostResult,
    LOCAL_AGENT_MAX_FRAME_BYTES, LOCAL_AGENT_MAX_MESSAGE_BYTES, LOCAL_AGENT_PROTOCOL_VERSION,
};
use std::time::Duration;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    process::{Child, ChildStdin, ChildStdout},
    time::timeout,
};

const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const TERMINATION_TIMEOUT: Duration = Duration::from_secs(2);
const FRAME_CONTINUATION_BIT: u32 = 1 << 31;
const FRAME_LENGTH_MASK: u32 = !FRAME_CONTINUATION_BIT;

#[derive(Debug)]
pub(crate) struct HostRequestFailure {
    pub(crate) message: String,
    pub(crate) code: Option<String>,
    pub(crate) fatal: bool,
}

impl HostRequestFailure {
    fn fatal(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: None,
            fatal: true,
        }
    }

    fn rejected(code: Option<String>, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code,
            fatal: false,
        }
    }
}

pub(crate) struct ManagedHost {
    pub(super) child: Child,
    pub(super) input: ChildStdin,
    output: ChildStdout,
    pub(super) owner_user_id: String,
    request_sequence: u64,
}

impl ManagedHost {
    pub(super) fn new(
        child: Child,
        input: ChildStdin,
        output: ChildStdout,
        owner_user_id: String,
    ) -> Self {
        Self {
            child,
            input,
            output,
            owner_user_id,
            request_sequence: 0,
        }
    }
}

pub(super) async fn health_check(host: &mut ManagedHost) -> Result<(), String> {
    match request_with_timeout(host, HostCommand::Health, HEALTH_TIMEOUT)
        .await
        .map_err(|error| error.message)?
    {
        HostResult::Health {
            service,
            storage_ready: true,
            max_frame_bytes: LOCAL_AGENT_MAX_FRAME_BYTES,
            max_message_bytes: LOCAL_AGENT_MAX_MESSAGE_BYTES,
            ..
        } if service == "chatos-local-agent-host" => Ok(()),
        _ => Err("Host health response does not match protocol v46.".to_string()),
    }
}

pub(crate) async fn request(
    host: &mut ManagedHost,
    command: HostCommand,
) -> Result<HostResult, HostRequestFailure> {
    request_with_timeout(host, command, REQUEST_TIMEOUT).await
}

async fn request_with_timeout(
    host: &mut ManagedHost,
    command: HostCommand,
    duration: Duration,
) -> Result<HostResult, HostRequestFailure> {
    host.request_sequence = host.request_sequence.saturating_add(1);
    let request = HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: format!(
            "windows-ipc-{}-{}",
            std::process::id(),
            host.request_sequence
        ),
        command,
    };
    request
        .validate()
        .map_err(|message| HostRequestFailure::rejected(None, message))?;
    timeout(duration, async {
        let body = serde_json::to_vec(&request)
            .map_err(|error| HostRequestFailure::rejected(None, error.to_string()))?;
        write_message(&mut host.input, &body)
            .await
            .map_err(HostRequestFailure::fatal)?;
        let response_body = read_message(&mut host.output)
            .await
            .map_err(HostRequestFailure::fatal)?;
        let response: HostResponseEnvelope = serde_json::from_slice(&response_body)
            .map_err(|error| HostRequestFailure::fatal(error.to_string()))?;
        if response.protocol_version != LOCAL_AGENT_PROTOCOL_VERSION
            || response.command_id != request.command_id
        {
            return Err(HostRequestFailure::fatal(
                "Host response envelope is invalid.",
            ));
        }
        if !response.ok {
            let error = response.error;
            return Err(HostRequestFailure::rejected(
                error.as_ref().map(|value| value.code.clone()),
                error
                    .map(|value| value.message)
                    .unwrap_or_else(|| "Host request failed.".to_string()),
            ));
        }
        response
            .result
            .ok_or_else(|| HostRequestFailure::fatal("Host response result is missing."))
    })
    .await
    .map_err(|_| HostRequestFailure::fatal("Host request timed out."))?
}

pub(super) async fn terminate(host: &mut ManagedHost) {
    let _ = host.input.shutdown().await;
    if timeout(TERMINATION_TIMEOUT, host.child.wait())
        .await
        .is_err()
    {
        let _ = host.child.kill().await;
        let _ = host.child.wait().await;
    }
}

async fn write_message<W>(writer: &mut W, body: &[u8]) -> Result<(), String>
where
    W: AsyncWrite + Unpin,
{
    if body.is_empty() || body.len() > LOCAL_AGENT_MAX_MESSAGE_BYTES {
        return Err("Host request message is invalid.".to_string());
    }
    let mut chunks = body.chunks(LOCAL_AGENT_MAX_FRAME_BYTES).peekable();
    while let Some(chunk) = chunks.next() {
        let length = u32::try_from(chunk.len())
            .map_err(|_| "Host request frame is too large.".to_string())?;
        let prefix = if chunks.peek().is_some() {
            length | FRAME_CONTINUATION_BIT
        } else {
            length
        };
        writer
            .write_all(&prefix.to_be_bytes())
            .await
            .map_err(|error| error.to_string())?;
        writer
            .write_all(chunk)
            .await
            .map_err(|error| error.to_string())?;
    }
    writer.flush().await.map_err(|error| error.to_string())
}

async fn read_message<R>(reader: &mut R) -> Result<Vec<u8>, String>
where
    R: AsyncRead + Unpin,
{
    let mut message = Vec::new();
    loop {
        let mut prefix = [0_u8; 4];
        reader
            .read_exact(&mut prefix)
            .await
            .map_err(|error| error.to_string())?;
        let encoded = u32::from_be_bytes(prefix);
        let has_more = encoded & FRAME_CONTINUATION_BIT != 0;
        let length = (encoded & FRAME_LENGTH_MASK) as usize;
        if length == 0
            || length > LOCAL_AGENT_MAX_FRAME_BYTES
            || message.len().saturating_add(length) > LOCAL_AGENT_MAX_MESSAGE_BYTES
        {
            return Err("Host response frame is invalid.".to_string());
        }
        let start = message.len();
        message.resize(start + length, 0);
        reader
            .read_exact(&mut message[start..])
            .await
            .map_err(|error| error.to_string())?;
        if !has_more {
            return Ok(message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{duplex, AsyncWriteExt};

    #[tokio::test]
    async fn message_codec_round_trips_continuation_frames() {
        let payload = vec![0x5a; LOCAL_AGENT_MAX_FRAME_BYTES + 257];
        let (mut writer, mut reader) = duplex(payload.len() + 16);

        write_message(&mut writer, &payload).await.expect("write");
        let received = read_message(&mut reader).await.expect("read");

        assert_eq!(received, payload);
    }

    #[tokio::test]
    async fn message_codec_rejects_truncated_continuation() {
        let (mut writer, mut reader) = duplex(16);
        writer
            .write_all(&(1_u32 | FRAME_CONTINUATION_BIT).to_be_bytes())
            .await
            .expect("prefix");
        writer.write_all(&[0x5a]).await.expect("body");
        writer.shutdown().await.expect("shutdown");

        assert!(read_message(&mut reader).await.is_err());
    }
}
