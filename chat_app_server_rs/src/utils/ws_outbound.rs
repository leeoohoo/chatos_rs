// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::ws::Message;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::warn;

pub type WsOutboundSender = mpsc::Sender<Message>;
pub type WsOutboundReceiver = mpsc::Receiver<Message>;

pub fn channel(capacity: usize) -> (WsOutboundSender, WsOutboundReceiver) {
    mpsc::channel(capacity)
}

pub fn try_send(tx: &WsOutboundSender, msg: Message, channel: &str) -> bool {
    match tx.try_send(msg) {
        Ok(()) => true,
        Err(mpsc::error::TrySendError::Full(_)) => {
            warn!(
                channel = %channel,
                "websocket outbound queue full; treating client as slow"
            );
            false
        }
        Err(mpsc::error::TrySendError::Closed(_)) => false,
    }
}

pub fn try_send_or_close(
    tx: &WsOutboundSender,
    msg: Message,
    channel: &str,
    shutdown: &CancellationToken,
) -> bool {
    let sent = try_send(tx, msg, channel);
    if !sent {
        shutdown.cancel();
    }
    sent
}
