// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use serde_json::json;

use crate::models::TaskRunEventRecord;
use crate::store::AppStore;

#[derive(Debug, Default)]
pub(super) struct PendingRunStreamEvent {
    event_type: Option<&'static str>,
    text: String,
    chunk_count: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct FlushedRunStreamEvent {
    pub(super) event_type: &'static str,
    pub(super) text: String,
    pub(super) chunk_count: usize,
}

impl PendingRunStreamEvent {
    pub(super) fn push(
        &mut self,
        event_type: &'static str,
        chunk: &str,
    ) -> Option<FlushedRunStreamEvent> {
        let flushed = if self.event_type.is_some() && self.event_type != Some(event_type) {
            self.take()
        } else {
            None
        };

        if self.event_type.is_none() {
            self.event_type = Some(event_type);
        }
        self.text.push_str(chunk);
        self.chunk_count += 1;
        flushed
    }

    pub(super) fn take(&mut self) -> Option<FlushedRunStreamEvent> {
        let event_type = self.event_type.take()?;
        let text = std::mem::take(&mut self.text);
        let chunk_count = std::mem::take(&mut self.chunk_count);
        if text.is_empty() {
            return None;
        }
        Some(FlushedRunStreamEvent {
            event_type,
            text,
            chunk_count,
        })
    }
}

pub(super) fn flush_pending_stream_event(
    store: &AppStore,
    run_id: &str,
    pending: &Arc<parking_lot::Mutex<PendingRunStreamEvent>>,
) {
    let flushed = {
        let mut state = pending.lock();
        state.take()
    };
    if let Some(flushed) = flushed {
        append_pending_stream_event(store, run_id, flushed);
    }
}

pub(super) fn append_pending_stream_event(
    store: &AppStore,
    run_id: &str,
    event: FlushedRunStreamEvent,
) {
    let chunk_chars = event.text.chars().count();
    store.append_run_event_sync(TaskRunEventRecord::new(
        run_id.to_string(),
        event.event_type,
        None,
        Some(json!({
            "text": event.text,
            "chunk_count": event.chunk_count,
            "chunk_chars": chunk_chars,
        })),
    ));
}

#[cfg(test)]
mod tests {
    use super::{FlushedRunStreamEvent, PendingRunStreamEvent};

    #[test]
    fn pending_run_stream_event_merges_same_type_chunks() {
        let mut pending = PendingRunStreamEvent::default();

        assert_eq!(pending.push("chunk", "hello"), None);
        assert_eq!(pending.push("chunk", " world"), None);
        assert_eq!(
            pending.take(),
            Some(FlushedRunStreamEvent {
                event_type: "chunk",
                text: "hello world".to_string(),
                chunk_count: 2,
            })
        );
    }

    #[test]
    fn pending_run_stream_event_flushes_when_type_changes() {
        let mut pending = PendingRunStreamEvent::default();

        assert_eq!(pending.push("thinking", "step1"), None);
        assert_eq!(
            pending.push("chunk", "answer"),
            Some(FlushedRunStreamEvent {
                event_type: "thinking",
                text: "step1".to_string(),
                chunk_count: 1,
            })
        );
        assert_eq!(
            pending.take(),
            Some(FlushedRunStreamEvent {
                event_type: "chunk",
                text: "answer".to_string(),
                chunk_count: 1,
            })
        );
    }
}
