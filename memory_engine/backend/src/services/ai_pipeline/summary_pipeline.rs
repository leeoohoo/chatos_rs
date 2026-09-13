// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::ai::{AiClient, AiGenerateTextError};

use super::chunking::split_chunks_by_token_limit;
use super::input::build_ai_input;
use super::overflow::is_context_overflow_error;
use super::{
    SummaryBuildResult, MAX_MERGE_ROUNDS, MAX_OVERFLOW_RETRIES, MIN_MERGE_TARGET_TOKENS,
    MIN_TOKEN_LIMIT,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SummaryPipelineSpec {
    pub prompt_title: String,
    pub summary_prompt: Option<String>,
    pub leaf_directive: String,
    pub merge_directive: String,
    pub token_limit: i64,
    pub target_tokens: Option<i64>,
    pub initial_token_limit_floor: i64,
    pub split_oversized_items: bool,
    pub log_label: String,
    pub items: Vec<String>,
}

impl SummaryPipelineSpec {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.items.is_empty() {
            return Err("empty summarize items".to_string());
        }
        for (name, value) in [
            ("prompt_title", self.prompt_title.as_str()),
            ("leaf_directive", self.leaf_directive.as_str()),
            ("merge_directive", self.merge_directive.as_str()),
            ("log_label", self.log_label.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("{name} must not be empty"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PipelinePhase {
    Leaf,
    Merge,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SummaryPipelineState {
    pub spec: SummaryPipelineSpec,
    effective_token_limit: i64,
    overflow_retry_count: usize,
    chunk_count: usize,
    phase: PipelinePhase,
    groups: Vec<Vec<String>>,
    cursor: usize,
    next: Vec<String>,
    merge_round: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SummaryPipelineStep {
    Continue,
    Retry {
        error: String,
        retry_kind: String,
        next_model_attempt: usize,
        backoff_ms: u64,
    },
    Finished(SummaryBuildResult),
}

impl SummaryPipelineState {
    pub(crate) fn new(spec: SummaryPipelineSpec) -> Result<Self, String> {
        spec.validate()?;
        let effective_token_limit = spec
            .token_limit
            .max(MIN_TOKEN_LIMIT)
            .max(spec.initial_token_limit_floor);
        let mut state = Self {
            spec,
            effective_token_limit,
            overflow_retry_count: 0,
            chunk_count: 0,
            phase: PipelinePhase::Leaf,
            groups: Vec::new(),
            cursor: 0,
            next: Vec::new(),
            merge_round: 0,
        };
        state.reset_leaf_groups()?;
        Ok(state)
    }

    pub(crate) fn result(&self, text: String) -> SummaryBuildResult {
        SummaryBuildResult {
            text,
            chunk_count: self.chunk_count,
            overflow_retry_count: self.overflow_retry_count,
        }
    }

    pub(crate) async fn execute_one(
        mut self,
        ai: &AiClient,
        model_attempt: usize,
    ) -> Result<(SummaryPipelineStep, Self), String> {
        self.normalize_progress()?;
        let Some(group) = self.groups.get(self.cursor).cloned() else {
            return Err("summary pipeline cursor escaped its active groups".to_string());
        };

        if self.phase == PipelinePhase::Merge && group.len() <= 1 {
            self.next.extend(group);
            self.cursor = self.cursor.saturating_add(1);
            return self.continue_or_finish("merge_singleton");
        }

        let directive = match self.phase {
            PipelinePhase::Leaf => self.spec.leaf_directive.as_str(),
            PipelinePhase::Merge => self.spec.merge_directive.as_str(),
        };
        let input = build_ai_input(
            self.spec.summary_prompt.as_deref(),
            directive,
            group.as_slice(),
        );
        let target_tokens = match self.phase {
            PipelinePhase::Leaf => self.spec.target_tokens,
            PipelinePhase::Merge => self
                .spec
                .target_tokens
                .map(|value| value.max(MIN_MERGE_TARGET_TOKENS)),
        };
        match ai
            .generate_text_once(
                crate::ai::SUMMARY_SYSTEM_PROMPT,
                input.as_str(),
                target_tokens,
                Some(input.chars().count()),
                true,
            )
            .await
        {
            Ok(text) => {
                self.next.push(text);
                self.cursor = self.cursor.saturating_add(1);
                self.continue_or_finish("summary_pipeline_progress")
            }
            Err(AiGenerateTextError::Retryable {
                message,
                retry_kind,
                ..
            }) if model_attempt <= ai.max_transient_retries() => Ok((
                SummaryPipelineStep::Retry {
                    backoff_ms: crate::ai::transient_retry_backoff_ms(
                        message.as_str(),
                        model_attempt,
                    ),
                    error: message,
                    retry_kind,
                    next_model_attempt: model_attempt.saturating_add(1),
                },
                self,
            )),
            Err(AiGenerateTextError::Retryable { message, .. }) => Err(format!(
                "model request exhausted {} transient retries: {message}",
                ai.max_transient_retries()
            )),
            Err(AiGenerateTextError::Fatal(error)) if is_context_overflow_error(&error) => {
                self.overflow_retry_count = self.overflow_retry_count.saturating_add(1);
                if self.overflow_retry_count > MAX_OVERFLOW_RETRIES {
                    return Err(format!(
                        "context overflow after {} retries while building summary: {}",
                        self.overflow_retry_count, self.spec.prompt_title
                    ));
                }
                let next_limit = (self.effective_token_limit / 2).max(MIN_TOKEN_LIMIT);
                if next_limit >= self.effective_token_limit {
                    return Err(error);
                }
                self.effective_token_limit = next_limit;
                self.reset_leaf_groups()?;
                Ok((SummaryPipelineStep::Continue, self))
            }
            Err(AiGenerateTextError::Fatal(error)) => Err(error),
        }
    }

    fn continue_or_finish(mut self, _reason: &str) -> Result<(SummaryPipelineStep, Self), String> {
        self.normalize_progress()?;
        if self.groups.is_empty() {
            return Err("summary pipeline produced no groups".to_string());
        }
        if self.cursor < self.groups.len() {
            return Ok((SummaryPipelineStep::Continue, self));
        }

        if self.next.len() == 1 {
            let text = self
                .next
                .first()
                .cloned()
                .ok_or_else(|| "summary pipeline lost final output".to_string())?;
            let result = self.result(text.clone());
            return Ok((SummaryPipelineStep::Finished(result), self));
        }
        if self.next.is_empty() {
            return Err("summary pipeline produced no output".to_string());
        }
        if self.merge_round >= MAX_MERGE_ROUNDS {
            return Err("context_length_exceeded: merge rounds exceeded".to_string());
        }

        self.phase = PipelinePhase::Merge;
        self.merge_round = self.merge_round.saturating_add(1);
        self.groups = split_chunks_by_token_limit(
            self.next.as_slice(),
            self.effective_token_limit.max(MIN_TOKEN_LIMIT),
            false,
        );
        self.cursor = 0;
        self.next.clear();
        if self.groups.iter().all(|group| group.len() <= 1) {
            return Err(
                "context_length_exceeded: merge chunks are individually oversized".to_string(),
            );
        }
        Ok((SummaryPipelineStep::Continue, self))
    }

    fn normalize_progress(&mut self) -> Result<(), String> {
        if self.groups.is_empty() {
            return Err("summary pipeline has no active groups".to_string());
        }
        Ok(())
    }

    fn reset_leaf_groups(&mut self) -> Result<(), String> {
        self.phase = PipelinePhase::Leaf;
        self.groups = split_chunks_by_token_limit(
            self.spec.items.as_slice(),
            self.effective_token_limit,
            self.spec.split_oversized_items,
        );
        if self.groups.is_empty() {
            return Err("no chunks".to_string());
        }
        self.chunk_count = self.groups.len();
        self.cursor = 0;
        self.next.clear();
        self.merge_round = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Arc;

    use axum::body::Body;
    use axum::extract::State;
    use axum::http::{header, Response, StatusCode};
    use axum::routing::post;
    use axum::Router;
    use tokio::sync::Mutex;

    use super::*;

    #[derive(Clone)]
    struct TestResponse {
        status: StatusCode,
        body: String,
    }

    async fn model_response(
        State(responses): State<Arc<Mutex<VecDeque<TestResponse>>>>,
    ) -> Response<Body> {
        let response = responses
            .lock()
            .await
            .pop_front()
            .expect("test model response is configured");
        Response::builder()
            .status(response.status)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .body(Body::from(response.body))
            .expect("test response")
    }

    async fn test_ai(responses: Vec<TestResponse>, max_retries: usize) -> AiClient {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test model server");
        let address = listener.local_addr().expect("test model address");
        let state = Arc::new(Mutex::new(VecDeque::from(responses)));
        tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new()
                    .route("/chat/completions", post(model_response))
                    .with_state(state),
            )
            .await
            .expect("test model server");
        });
        AiClient::for_test(format!("http://{address}"), max_retries)
    }

    fn success(text: &str) -> TestResponse {
        TestResponse {
            status: StatusCode::OK,
            body: format!(
                "data: {{\"choices\":[{{\"delta\":{{\"content\":{}}}}}]}}\n\ndata: [DONE]\n\n",
                serde_json::to_string(text).expect("encode response text")
            ),
        }
    }

    fn error(status: StatusCode, message: &str) -> TestResponse {
        TestResponse {
            status,
            body: serde_json::json!({"error": {"message": message}}).to_string(),
        }
    }

    fn spec(items: Vec<String>, token_limit: i64) -> SummaryPipelineSpec {
        SummaryPipelineSpec {
            prompt_title: "test".to_string(),
            summary_prompt: Some("prompt".to_string()),
            leaf_directive: "leaf".to_string(),
            merge_directive: "merge".to_string(),
            token_limit,
            target_tokens: Some(256),
            initial_token_limit_floor: MIN_TOKEN_LIMIT,
            split_oversized_items: true,
            log_label: "test".to_string(),
            items,
        }
    }

    #[tokio::test]
    async fn single_chunk_finishes_in_one_model_step() {
        let ai = test_ai(vec![success("final summary")], 2).await;
        let state = SummaryPipelineState::new(spec(vec!["short input".to_string()], 512))
            .expect("pipeline state");
        let (step, state) = state.execute_one(&ai, 1).await.expect("model step");

        assert!(matches!(
            step,
            SummaryPipelineStep::Finished(ref result) if result.text == "final summary"
        ));
        assert_eq!(state.chunk_count, 1);
    }

    #[tokio::test]
    async fn multiple_leaf_steps_are_merged_before_final() {
        let ai = test_ai(
            vec![
                success("partial-a"),
                success("partial-b"),
                success("merged"),
            ],
            2,
        )
        .await;
        let items = vec!["a".repeat(300), "b".repeat(300)];
        let mut state =
            SummaryPipelineState::new(spec(items, MIN_TOKEN_LIMIT)).expect("pipeline state");

        for _ in 0..2 {
            let (step, next) = state.execute_one(&ai, 1).await.expect("model step");
            assert_eq!(step, SummaryPipelineStep::Continue);
            state = next;
        }
        let (step, state) = state.execute_one(&ai, 1).await.expect("merge step");
        assert!(matches!(
            step,
            SummaryPipelineStep::Finished(ref result) if result.text == "merged"
        ));
        assert_eq!(state.merge_round, 1);
    }

    #[tokio::test]
    async fn transient_failure_is_returned_as_queue_retry() {
        let ai = test_ai(
            vec![error(
                StatusCode::SERVICE_UNAVAILABLE,
                "service unavailable",
            )],
            3,
        )
        .await;
        let state = SummaryPipelineState::new(spec(vec!["input".to_string()], 512))
            .expect("pipeline state");
        let (step, _) = state.execute_one(&ai, 1).await.expect("model step");

        assert!(matches!(
            step,
            SummaryPipelineStep::Retry {
                next_model_attempt: 2,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn transient_retry_backoff_increases_with_the_model_attempt() {
        let ai = test_ai(
            vec![
                error(StatusCode::SERVICE_UNAVAILABLE, "service unavailable"),
                error(StatusCode::SERVICE_UNAVAILABLE, "service unavailable"),
            ],
            3,
        )
        .await;
        let state = SummaryPipelineState::new(spec(vec!["input".to_string()], 512))
            .expect("pipeline state");
        let (first, state) = state.execute_one(&ai, 1).await.expect("first retry");
        let first_backoff = match first {
            SummaryPipelineStep::Retry { backoff_ms, .. } => backoff_ms,
            other => panic!("expected retry, got {other:?}"),
        };
        let (second, _) = state.execute_one(&ai, 2).await.expect("second retry");
        let second_backoff = match second {
            SummaryPipelineStep::Retry { backoff_ms, .. } => backoff_ms,
            other => panic!("expected retry, got {other:?}"),
        };

        assert!(second_backoff > first_backoff);
    }

    #[tokio::test]
    async fn context_overflow_halves_limit_and_persists_continuation() {
        let ai = test_ai(
            vec![error(StatusCode::BAD_REQUEST, "context_length_exceeded")],
            2,
        )
        .await;
        let state =
            SummaryPipelineState::new(spec(vec!["x".repeat(600)], 512)).expect("pipeline state");
        let (step, state) = state.execute_one(&ai, 1).await.expect("model step");

        assert_eq!(step, SummaryPipelineStep::Continue);
        assert_eq!(state.effective_token_limit, 256);
        assert_eq!(state.overflow_retry_count, 1);
    }

    #[test]
    fn merge_round_limit_fails_without_another_model_request() {
        let mut state = SummaryPipelineState::new(spec(vec!["input".to_string()], 512))
            .expect("pipeline state");
        state.phase = PipelinePhase::Merge;
        state.groups = vec![vec!["a".to_string(), "b".to_string()]];
        state.cursor = 1;
        state.next = vec!["a".repeat(600), "b".repeat(600)];
        state.merge_round = MAX_MERGE_ROUNDS;

        let error = state
            .continue_or_finish("test")
            .expect_err("merge round must fail");
        assert!(error.contains("merge rounds exceeded"));
    }

    #[test]
    fn state_preserves_in_memory_progress() {
        let state = SummaryPipelineState::new(spec(
            vec!["a".repeat(300), "b".repeat(300)],
            MIN_TOKEN_LIMIT,
        ))
        .expect("pipeline state");
        assert_eq!(state.phase, PipelinePhase::Leaf);
        assert_eq!(state.cursor, 0);
        assert_eq!(state.groups.len(), 2);
    }
}
