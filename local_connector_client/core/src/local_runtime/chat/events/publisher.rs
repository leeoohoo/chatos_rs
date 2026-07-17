// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::local_runtime::storage::{AppendLocalRuntimeEventInput, LocalDatabase};

use super::callbacks::runtime_callbacks;

#[derive(Debug)]
struct PendingEvent {
    event_name: &'static str,
    stream_type: Option<&'static str>,
    payload: Value,
}

#[derive(Clone, Debug)]
pub(in crate::local_runtime) struct LocalChatEventSender {
    sender: mpsc::UnboundedSender<PendingEvent>,
}

impl LocalChatEventSender {
    pub(in crate::local_runtime) fn publish(
        &self,
        event_name: &'static str,
        stream_type: Option<&'static str>,
        payload: Value,
    ) {
        let _ = self.sender.send(PendingEvent {
            event_name,
            stream_type,
            payload,
        });
    }
}

pub(in crate::local_runtime) struct LocalChatEventStream {
    sender: LocalChatEventSender,
    writer: JoinHandle<()>,
}

impl LocalChatEventStream {
    pub(in crate::local_runtime) fn start(
        database: LocalDatabase,
        owner_user_id: &str,
        session_id: &str,
        turn_id: &str,
    ) -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<PendingEvent>();
        let owner_user_id = owner_user_id.to_string();
        let session_id = session_id.to_string();
        let turn_id = turn_id.to_string();
        let writer = tokio::spawn(async move {
            while let Some(event) = receiver.recv().await {
                if let Err(error) = database
                    .append_runtime_event(AppendLocalRuntimeEventInput {
                        owner_user_id: owner_user_id.clone(),
                        session_id: session_id.clone(),
                        turn_id: turn_id.clone(),
                        event_name: event.event_name.to_string(),
                        stream_type: event.stream_type.map(ToOwned::to_owned),
                        payload: event.payload,
                    })
                    .await
                {
                    eprintln!(
                        "failed to persist local runtime event for session {session_id} turn {turn_id}: {error}"
                    );
                }
            }
        });
        Self {
            sender: LocalChatEventSender { sender },
            writer,
        }
    }

    pub(in crate::local_runtime) fn callbacks(&self) -> chatos_ai_runtime::RuntimeCallbacks {
        runtime_callbacks(self.sender.clone())
    }

    pub(in crate::local_runtime) fn publish(
        &self,
        event_name: &'static str,
        stream_type: Option<&'static str>,
        payload: Value,
    ) {
        self.sender.publish(event_name, stream_type, payload);
    }

    pub(in crate::local_runtime) async fn finish(self) -> Result<()> {
        drop(self.sender);
        self.writer.await.context("join local runtime event writer")
    }
}
