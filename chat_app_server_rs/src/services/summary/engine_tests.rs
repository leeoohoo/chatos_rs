use std::sync::{Arc, Mutex};

use serde_json::json;

use super::engine::{maybe_summarize, retry_after_context_overflow};
use super::traits::{SummaryBoxFuture, SummaryLlmClient};
use super::types::{SummaryLlmRequest, SummaryOptions, SummaryTrigger};

#[derive(Clone)]
struct MockClient {
    max_messages: usize,
    calls: Arc<Mutex<Vec<usize>>>,
}

impl MockClient {
    fn new(max_messages: usize) -> Self {
        Self {
            max_messages,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn calls(&self) -> Vec<usize> {
        self.calls.lock().unwrap().clone()
    }
}

impl SummaryLlmClient for MockClient {
    fn summarize<'a>(
        &'a self,
        request: SummaryLlmRequest,
    ) -> SummaryBoxFuture<'a, Result<String, String>> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(request.context_messages.len());
            if request.context_messages.len() > self.max_messages {
                return Err("context_length_exceeded".to_string());
            }
            Ok(format!("summary({})", request.context_messages.len()))
        })
    }
}

fn options() -> SummaryOptions {
    SummaryOptions {
        message_limit: 4,
        max_context_tokens: 10_000,
        keep_last_n: 0,
        target_summary_tokens: 300,
        merge_target_tokens: 260,
        model: "gpt-4o".to_string(),
        temperature: 0.2,
        bisect_enabled: true,
        bisect_max_depth: 6,
        bisect_min_messages: 2,
        retry_on_context_overflow: true,
    }
}

#[tokio::test]
async fn bisect_summary_recovers_from_context_overflow() {
    let client = MockClient::new(3);
    let messages: Vec<_> = (0..10)
        .map(|i| json!({"role": "user", "content": format!("msg-{i}")}))
        .collect();

    let result = maybe_summarize(
        &client,
        &messages,
        &options(),
        None,
        None,
        SummaryTrigger::Proactive,
    )
    .await
    .expect("summary should succeed");

    assert!(result.summarized);
    assert!(result.stats.chunk_count > 1);
    assert!(result.stats.max_depth > 0);
    assert!(client.calls().iter().any(|size| *size > 3));
}

#[tokio::test]
async fn max_depth_guard_falls_back_to_truncated_summary() {
    let client = MockClient::new(1);
    let mut opts = options();
    opts.bisect_max_depth = 0;

    let messages = vec![
        json!({"role": "user", "content": "a"}),
        json!({"role": "assistant", "content": "b"}),
    ];

    let result = maybe_summarize(
        &client,
        &messages,
        &opts,
        None,
        None,
        SummaryTrigger::OverflowRetry,
    )
    .await
    .expect("summary should fallback");

    assert!(result.summarized);
    assert!(result.truncated);
}

#[tokio::test]
async fn overflow_retry_returns_none_for_non_overflow_errors() {
    let client = MockClient::new(8);
    let messages = vec![json!({"role": "user", "content": "hello"})];
    let result = retry_after_context_overflow(
        &client,
        &messages,
        "rate_limit_exceeded",
        &options(),
        None,
        None,
    )
    .await
    .expect("retry decision should succeed");

    assert!(result.is_none());
}
