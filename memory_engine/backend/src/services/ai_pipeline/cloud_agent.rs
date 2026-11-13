// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::{AiRuntimeResult, AiSingleStepOutcome};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::ai::{AiClient, AiGenerateTextError};

use super::chunking::{contains_source_ref, split_chunks_by_token_limit};
use super::input::build_ai_input;
use super::overflow::is_context_overflow_error;
use super::{SummaryBuildResult, MAX_OVERFLOW_RETRIES};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct CloudSummaryPipelineSpec {
    pub pipeline_revision: String,
    pub prompt_title: String,
    pub managed_prompt: Option<String>,
    pub leaf_directive: String,
    pub merge_directive: String,
    pub token_limit: i64,
    pub split_oversized_items: bool,
    pub log_label: String,
    pub items: Vec<String>,
    #[serde(default)]
    pub resume: Value,
}

impl CloudSummaryPipelineSpec {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.items.is_empty() {
            return Err("empty summarize items".to_string());
        }
        if self.token_limit <= 0 {
            return Err("model source capacity must be positive".into());
        }
        for (name, value) in [
            ("pipeline_revision", self.pipeline_revision.as_str()),
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PipelinePhase {
    Leaf,
    Merge,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct CloudSummaryPipelineState {
    pub spec: CloudSummaryPipelineSpec,
    payload_contract: crate::models::structured_payload::MemoryPayloadContract,
    effective_token_limit: i64,
    pub budget: crate::services::model_budget::MemoryModelBudget,
    overflow_retry_count: usize,
    #[serde(default)]
    transient_split_count: usize,
    chunk_count: usize,
    phase: PipelinePhase,
    groups: Vec<Vec<String>>,
    cursor: usize,
    next: Vec<String>,
    merge_round: usize,
}

impl CloudSummaryPipelineState {
    pub(crate) fn new(
        spec: CloudSummaryPipelineSpec,
        payload_contract: crate::models::structured_payload::MemoryPayloadContract,
        budget: crate::services::model_budget::MemoryModelBudget,
    ) -> Result<Self, String> {
        spec.validate()?;
        payload_contract.validate()?;
        let effective_token_limit = spec.token_limit.min(budget.input_limit()?);
        let mut state = Self {
            spec,
            payload_contract,
            effective_token_limit,
            budget,
            overflow_retry_count: 0,
            transient_split_count: 0,
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

    pub(crate) fn result(
        &self,
        structured_payload: serde_json::Value,
    ) -> Result<SummaryBuildResult, String> {
        let text =
            if crate::models::structured_payload::durable_fact_count(&structured_payload)? == 0 {
                String::new()
            } else {
                crate::models::structured_payload::render_payload(&structured_payload)?
            };
        Ok(SummaryBuildResult {
            text,
            structured_payload,
            chunk_count: self.chunk_count,
            overflow_retry_count: self.overflow_retry_count,
        })
    }

    pub(crate) async fn execute_one(
        mut self,
        ai: &AiClient,
        model_attempt: usize,
    ) -> Result<(AiSingleStepOutcome, Self, Option<SummaryBuildResult>), String> {
        if ai.budget() != &self.budget {
            return Err(
                "Memory model identity or capability changed during the frozen generation".into(),
            );
        }
        let directive = format!(
            "{}\n\n{}",
            match self.phase {
                PipelinePhase::Leaf => &self.spec.leaf_directive,
                PipelinePhase::Merge => &self.spec.merge_directive,
            },
            self.payload_contract.model_instruction()?
        );
        let empty_input = build_ai_input(self.spec.managed_prompt.as_deref(), &directive, &[]);
        let capacity = ai.payload_source_capacity(
            crate::ai::SUMMARY_SYSTEM_PROMPT,
            &empty_input,
            &self.payload_contract.response_format()?,
        )?;
        if capacity < self.effective_token_limit {
            self.effective_token_limit = capacity;
            self.repack_remaining_groups()?;
        }
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
        let payload_contract = self.payload_contract_for_group(group.as_slice())?;
        let directive = format!("{}\n\n{}", directive, payload_contract.model_instruction()?);
        let input = build_ai_input(
            self.spec.managed_prompt.as_deref(),
            directive.as_str(),
            group.as_slice(),
        );
        let response_format = payload_contract.response_format()?;
        match ai
            .generate_memory_payload_once(
                crate::ai::SUMMARY_SYSTEM_PROMPT,
                input.as_str(),
                None,
                Some(input.chars().count()),
                &response_format,
            )
            .await
        {
            Ok(output) => {
                let payload = match payload_contract.finalize_model_output(output.as_str()) {
                    Ok(payload) => payload,
                    Err(error) if model_attempt <= ai.max_transient_retries() => {
                        return Ok((
                            AiSingleStepOutcome::Retry {
                                error,
                                retry_kind: "memory_payload_validation".to_string(),
                                next_model_attempt: model_attempt.saturating_add(1),
                                backoff_ms: 0,
                            },
                            self,
                            None,
                        ));
                    }
                    Err(error) => {
                        return Ok((
                            AiSingleStepOutcome::Failed {
                                error: format!(
                                    "model memory payload validation exhausted {} retries: {error}",
                                    ai.max_transient_retries()
                                ),
                            },
                            self,
                            None,
                        ));
                    }
                };
                self.next
                    .push(serde_json::to_string(&payload).map_err(|error| error.to_string())?);
                self.cursor = self.cursor.saturating_add(1);
                self.continue_or_finish("summary_pipeline_progress")
            }
            Err(AiGenerateTextError::Retryable {
                message,
                retry_kind,
                backoff_ms,
            }) if model_attempt <= ai.max_transient_retries() => Ok((
                AiSingleStepOutcome::Retry {
                    error: message,
                    retry_kind,
                    next_model_attempt: model_attempt.saturating_add(1),
                    backoff_ms,
                },
                self,
                None,
            )),
            Err(AiGenerateTextError::Retryable { message, .. }) => Ok((
                if self.split_after_exhausted_transient(group.as_slice())? {
                    continuation_outcome("summary_transient_split")
                } else {
                    AiSingleStepOutcome::Failed {
                        error: format!(
                            "model request exhausted {} transient retries: {message}",
                            ai.max_transient_retries()
                        ),
                    }
                },
                self,
                None,
            )),
            Err(AiGenerateTextError::Fatal(error)) if is_context_overflow_error(&error) => {
                self.overflow_retry_count = self.overflow_retry_count.saturating_add(1);
                if self.overflow_retry_count > MAX_OVERFLOW_RETRIES {
                    return Ok((
                        AiSingleStepOutcome::Failed {
                            error: format!(
                                "context overflow after {} retries while building summary: {}",
                                self.overflow_retry_count, self.spec.prompt_title
                            ),
                        },
                        self,
                        None,
                    ));
                }
                let next_limit = self.effective_token_limit / 2;
                if next_limit <= 0 || next_limit >= self.effective_token_limit {
                    return Ok((AiSingleStepOutcome::Failed { error }, self, None));
                }
                self.effective_token_limit = next_limit;
                self.reset_leaf_groups()?;
                Ok((continuation_outcome("summary_context_overflow"), self, None))
            }
            Err(AiGenerateTextError::Fatal(error)) => {
                Ok((AiSingleStepOutcome::Failed { error }, self, None))
            }
        }
    }

    fn continue_or_finish(
        mut self,
        reason: &str,
    ) -> Result<(AiSingleStepOutcome, Self, Option<SummaryBuildResult>), String> {
        self.normalize_progress()?;
        if self.groups.is_empty() {
            return Err("summary pipeline produced no groups".to_string());
        }
        if self.cursor < self.groups.len() {
            return Ok((continuation_outcome(reason), self, None));
        }

        if self.next.len() == 1 {
            let output = self
                .next
                .first()
                .cloned()
                .ok_or_else(|| "summary pipeline lost final output".to_string())?;
            let payload = self
                .payload_contract
                .finalize_model_output(output.as_str())?;
            let result = self.result(payload.clone())?;
            let content = serde_json::to_string(&payload).map_err(|error| error.to_string())?;
            return Ok((final_outcome(content), self, Some(result)));
        }
        if self.next.is_empty() {
            return Err("summary pipeline produced no output".to_string());
        }
        // Every merge must reduce the number of outstanding items. Termination
        // follows from the frozen input, without an arbitrary round ceiling.
        if self.phase == PipelinePhase::Merge
            && self.next.len() >= self.groups.iter().map(Vec::len).sum::<usize>()
        {
            return Ok((
                AiSingleStepOutcome::Failed {
                    error: "summary merge made no progress".into(),
                },
                self,
                None,
            ));
        }

        self.phase = PipelinePhase::Merge;
        self.merge_round = self.merge_round.saturating_add(1);
        self.groups =
            split_chunks_by_token_limit(self.next.as_slice(), self.effective_token_limit, false);
        self.cursor = 0;
        self.next.clear();
        if self.groups.iter().all(|group| group.len() <= 1) {
            return Ok((
                AiSingleStepOutcome::Failed {
                    error: "context_length_exceeded: merge chunks are individually oversized"
                        .to_string(),
                },
                self,
                None,
            ));
        }
        Ok((continuation_outcome("summary_merge_round"), self, None))
    }

    fn repack_remaining_groups(&mut self) -> Result<(), String> {
        let pending = self.groups[self.cursor..]
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let mut groups = self.groups[..self.cursor].to_vec();
        groups.extend(split_chunks_by_token_limit(
            &pending,
            self.effective_token_limit,
            self.phase == PipelinePhase::Leaf && self.spec.split_oversized_items,
        ));
        self.groups = groups;
        self.validate_group_capacity()?;
        self.chunk_count = self.chunk_count.max(self.groups.len());
        Ok(())
    }

    fn normalize_progress(&mut self) -> Result<(), String> {
        if self.groups.is_empty() {
            return Err("summary pipeline has no active groups".to_string());
        }
        Ok(())
    }

    fn validate_group_capacity(&self) -> Result<(), String> {
        if self.groups[self.cursor..].iter().any(|group| {
            group
                .iter()
                .map(|item| super::estimate_tokens_text(item))
                .sum::<i64>()
                > self.effective_token_limit
        }) {
            return Err(
                "model input has insufficient room for a source chunk and its envelope".into(),
            );
        }
        Ok(())
    }

    fn split_after_exhausted_transient(&mut self, group: &[String]) -> Result<bool, String> {
        let next_limit = self.effective_token_limit / 2;
        let group_tokens = group
            .iter()
            .map(|item| super::estimate_tokens_text(item))
            .sum::<i64>();
        if next_limit <= 0
            || next_limit >= self.effective_token_limit
            || group_tokens <= next_limit
            || self.transient_split_count >= MAX_OVERFLOW_RETRIES
        {
            return Ok(false);
        }
        self.transient_split_count = self.transient_split_count.saturating_add(1);
        self.effective_token_limit = next_limit;
        self.reset_leaf_groups()?;
        Ok(true)
    }

    fn payload_contract_for_group(
        &self,
        group: &[String],
    ) -> Result<crate::models::structured_payload::MemoryPayloadContract, String> {
        let source_refs = self
            .payload_contract
            .source_refs
            .iter()
            .filter(|source_ref| {
                group
                    .iter()
                    .any(|item| contains_source_ref(item, source_ref.as_str()))
            })
            .cloned()
            .collect::<Vec<_>>();
        if source_refs.is_empty() {
            return Err(
                "summary pipeline group contains no frozen source reference identifier".to_string(),
            );
        }
        let mut contract = self.payload_contract.clone();
        contract.source_refs = source_refs;
        contract.validate()?;
        Ok(contract)
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
        self.validate_group_capacity()?;
        Ok(())
    }
}

fn continuation_outcome(reason: &str) -> AiSingleStepOutcome {
    AiSingleStepOutcome::Continue {
        response: AiRuntimeResult {
            content: String::new(),
            reasoning: None,
            tool_calls: None,
            finish_reason: Some(reason.to_string()),
            usage: None,
            response_id: None,
            response_output_items: Vec::new(),
            request_input_items: Vec::new(),
        },
        input_items: Vec::new(),
        reason: reason.to_string(),
    }
}

fn final_outcome(content: String) -> AiSingleStepOutcome {
    AiSingleStepOutcome::Final(AiRuntimeResult {
        content,
        reasoning: None,
        tool_calls: None,
        finish_reason: Some("stop".to_string()),
        usage: Some(json!({"pipeline": "memory_engine_summary"})),
        response_id: None,
        response_output_items: Vec::new(),
        request_input_items: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::super::CHUNK_TEST_CAPACITY;
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
                    .route("/responses", post(model_response))
                    .with_state(state),
            )
            .await
            .expect("test model server");
        });
        AiClient::for_test(format!("http://{address}"), max_retries)
    }

    fn success(text: &str) -> TestResponse {
        payload_response(serde_json::json!({
            "schema_version":"memory_payload_v1",
            "scope":{
                "kind":"thread",
                "subject_id":"agent:test",
                "project_partition_key":"__none__"
            },
            "current_goal":null,
            "completed":[],
            "decisions":[],
            "open_items":[{
                "fact_id":"model-local",
                "fact_key":"memory.test",
                "value":text,
                "status":"active",
                "supersedes":[],
                "source_refs":["source-1"],
                "observed_at":null,
                "confidence":"model_inference"
            }],
            "blockers":[],
            "artifacts":[],
            "constraints":[],
            "stable_preferences":[],
            "uncertainties":[]
        }))
    }

    fn empty_success() -> TestResponse {
        payload_response(serde_json::json!({
            "schema_version":"memory_payload_v1",
            "scope":{
                "kind":"thread",
                "subject_id":"agent:test",
                "project_partition_key":"__none__"
            },
            "current_goal":null,
            "completed":[],
            "decisions":[],
            "open_items":[],
            "blockers":[],
            "artifacts":[],
            "constraints":[],
            "stable_preferences":[],
            "uncertainties":[]
        }))
    }

    fn payload_response(payload: serde_json::Value) -> TestResponse {
        let payload = payload.to_string();
        TestResponse {
            status: StatusCode::OK,
            body: format!(
                "data: {{\"type\":\"response.output_text.delta\",\"delta\":{text}}}\n\ndata: {{\"type\":\"response.completed\",\"response\":{{\"status\":\"completed\",\"output\":[]}}}}\n\n",
                text = serde_json::to_string(&payload).expect("encode response text")
            ),
        }
    }

    fn error(status: StatusCode, message: &str) -> TestResponse {
        TestResponse {
            status,
            body: serde_json::json!({"error": {"message": message}}).to_string(),
        }
    }

    fn spec(items: Vec<String>, token_limit: i64) -> CloudSummaryPipelineSpec {
        CloudSummaryPipelineSpec {
            pipeline_revision: "test-pipeline-v1".to_string(),
            prompt_title: "test".to_string(),
            managed_prompt: Some("prompt".to_string()),
            leaf_directive: "leaf".to_string(),
            merge_directive: "merge".to_string(),
            token_limit,
            split_oversized_items: true,
            log_label: "test".to_string(),
            items: items
                .into_iter()
                .map(|item| format!("[source_ref=source-1]\n{item}"))
                .collect(),
            resume: Value::Null,
        }
    }

    fn test_state(
        spec: CloudSummaryPipelineSpec,
        contract: crate::models::structured_payload::MemoryPayloadContract,
    ) -> Result<CloudSummaryPipelineState, String> {
        CloudSummaryPipelineState::new(
            spec,
            contract,
            crate::services::model_budget::test_budget(100_000),
        )
    }

    fn contract() -> crate::models::structured_payload::MemoryPayloadContract {
        crate::models::structured_payload::MemoryPayloadContract {
            generation_id: "generation-1".to_string(),
            scope_kind: "thread".to_string(),
            subject_id: "agent:test".to_string(),
            project_partition_key: Some("__none__".to_string()),
            source_refs: vec!["source-1".to_string()],
            supersession_candidates: Vec::new(),
            allow_empty_facts: false,
        }
    }

    #[tokio::test]
    async fn single_chunk_finishes_in_one_model_step() {
        let ai = test_ai(vec![success("final summary")], 2).await;
        let state = test_state(spec(vec!["short input".to_string()], 512), contract())
            .expect("pipeline state");
        let (outcome, state, result) = state.execute_one(&ai, 1).await.expect("model step");

        assert!(matches!(outcome, AiSingleStepOutcome::Final(_)));
        assert_eq!(
            result.expect("summary result").text,
            "memory.test: final summary"
        );
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
        let items = vec!["a".repeat(1_500), "b".repeat(1_500)];
        let mut state = test_state(spec(items, 512), contract()).expect("pipeline state");

        for expected_reason in ["summary_pipeline_progress", "summary_merge_round"] {
            let (outcome, next, result) = state.execute_one(&ai, 1).await.expect("model step");
            assert!(
                matches!(outcome, AiSingleStepOutcome::Continue { ref reason, .. } if reason == expected_reason)
            );
            assert!(result.is_none());
            state = next;
        }
        let (outcome, state, result) = state.execute_one(&ai, 1).await.expect("merge step");
        assert!(matches!(outcome, AiSingleStepOutcome::Final(_)));
        assert_eq!(result.expect("merged result").text, "memory.test: merged");
        assert_eq!(state.merge_round, 1);
    }

    #[tokio::test]
    async fn one_oversized_tool_output_completes_all_chunks_and_merge_rounds() {
        let responses = (0..128)
            .map(|index| success(format!("summary-{index}").as_str()))
            .collect::<Vec<_>>();
        let ai = test_ai(responses, 2).await;
        let oversized = format!(
            "[tool_result]\n{}",
            serde_json::json!({
                "call_id": "call-oversized",
                "detail": "large output ".repeat(1_200),
                "conclusion": "unique conclusion"
            })
        );
        let mut state = test_state(spec(vec![oversized], 512), contract()).expect("pipeline state");
        assert!(state.groups.len() > 1);

        for _ in 0..128 {
            let (outcome, next, result) = state.execute_one(&ai, 1).await.expect("model step");
            state = next;
            match outcome {
                AiSingleStepOutcome::Final(_) => {
                    assert!(result.expect("oversized result").text.contains("summary-"));
                    assert!(state.merge_round > 0);
                    return;
                }
                AiSingleStepOutcome::Continue { .. } => {}
                other => panic!("oversized pipeline did not complete: {other:?}"),
            }
        }
        panic!("oversized pipeline exceeded the bounded test step budget");
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
        let state =
            test_state(spec(vec!["input".to_string()], 512), contract()).expect("pipeline state");
        let (outcome, _, result) = state.execute_one(&ai, 1).await.expect("model step");

        assert!(matches!(
            outcome,
            AiSingleStepOutcome::Retry {
                next_model_attempt: 2,
                ..
            }
        ));
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn exhausted_transient_retries_split_large_input_before_failing_job() {
        let ai = test_ai(
            vec![error(
                StatusCode::SERVICE_UNAVAILABLE,
                "stream closed before response.completed",
            )],
            2,
        )
        .await;
        let state =
            test_state(spec(vec!["x".repeat(1_800)], 512), contract()).expect("pipeline state");
        let (outcome, state, result) = state.execute_one(&ai, 3).await.expect("model step");

        assert!(
            matches!(outcome, AiSingleStepOutcome::Continue { ref reason, .. } if reason == "summary_transient_split")
        );
        assert_eq!(state.effective_token_limit, 256);
        assert!(state.chunk_count > 1);
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn exhausted_transient_retries_do_not_split_small_input() {
        let ai = test_ai(
            vec![error(
                StatusCode::SERVICE_UNAVAILABLE,
                "service unavailable",
            )],
            2,
        )
        .await;
        let state =
            test_state(spec(vec!["input".to_string()], 512), contract()).expect("pipeline state");
        let (outcome, _, result) = state.execute_one(&ai, 3).await.expect("model step");

        assert!(matches!(outcome, AiSingleStepOutcome::Failed { .. }));
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn trivial_input_can_complete_with_no_durable_facts() {
        let ai = test_ai(vec![empty_success()], 2).await;
        let mut payload_contract = contract();
        payload_contract.allow_empty_facts = true;
        let state = test_state(
            spec(vec!["[role=system]\ntest".to_string()], 512),
            payload_contract,
        )
        .expect("pipeline state");

        let (outcome, _, result) = state.execute_one(&ai, 1).await.expect("model step");

        assert!(matches!(outcome, AiSingleStepOutcome::Final(_)));
        let result = result.expect("no-op summary result");
        assert!(result.text.is_empty());
        assert_eq!(
            crate::models::structured_payload::durable_fact_count(&result.structured_payload),
            Ok(0)
        );
    }

    #[tokio::test]
    async fn malformed_or_forbidden_empty_payload_uses_bounded_model_retry() {
        let ai = test_ai(vec![empty_success()], 2).await;
        let state =
            test_state(spec(vec!["input".to_string()], 512), contract()).expect("pipeline state");

        let (outcome, _, result) = state.execute_one(&ai, 1).await.expect("model step");

        assert!(matches!(
            outcome,
            AiSingleStepOutcome::Retry {
                ref retry_kind,
                next_model_attempt: 2,
                ..
            } if retry_kind == "memory_payload_validation"
        ));
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn malformed_payload_fails_after_validation_retry_budget_is_exhausted() {
        let ai = test_ai(vec![empty_success()], 2).await;
        let state =
            test_state(spec(vec!["input".to_string()], 512), contract()).expect("pipeline state");

        let (outcome, _, result) = state.execute_one(&ai, 3).await.expect("model step");

        assert!(matches!(
            outcome,
            AiSingleStepOutcome::Failed { ref error }
                if error.contains("validation exhausted 2 retries")
        ));
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn context_overflow_halves_limit_and_persists_continuation() {
        let ai = test_ai(
            vec![error(StatusCode::BAD_REQUEST, "context_length_exceeded")],
            2,
        )
        .await;
        let state =
            test_state(spec(vec!["x".repeat(600)], 512), contract()).expect("pipeline state");
        let (outcome, state, result) = state.execute_one(&ai, 1).await.expect("model step");

        assert!(
            matches!(outcome, AiSingleStepOutcome::Continue { ref reason, .. } if reason == "summary_context_overflow")
        );
        assert_eq!(state.effective_token_limit, 256);
        assert_eq!(state.overflow_retry_count, 1);
        assert!(result.is_none());
    }

    #[test]
    fn merge_without_progress_fails_without_another_model_request() {
        let mut state =
            test_state(spec(vec!["input".to_string()], 512), contract()).expect("pipeline state");
        state.phase = PipelinePhase::Merge;
        state.groups = vec![vec!["a".to_string(), "b".to_string()]];
        state.cursor = 1;
        state.next = vec!["a".repeat(600), "b".repeat(600)];
        state.groups = vec![vec!["a".into()], vec!["b".into()]];
        state.cursor = state.groups.len();

        let (outcome, _, result) = state
            .continue_or_finish("test")
            .expect("terminal pipeline result");
        assert!(
            matches!(outcome, AiSingleStepOutcome::Failed { ref error } if error.contains("made no progress"))
        );
        assert!(result.is_none());
    }

    #[test]
    fn state_survives_serde_roundtrip() {
        let state = test_state(
            spec(vec!["a".repeat(300), "b".repeat(300)], CHUNK_TEST_CAPACITY),
            contract(),
        )
        .expect("pipeline state");
        let encoded = serde_json::to_value(&state).expect("encode state");
        let decoded =
            serde_json::from_value::<CloudSummaryPipelineState>(encoded).expect("decode state");
        assert_eq!(decoded, state);

        let mut legacy = serde_json::to_value(&state).expect("encode legacy state");
        legacy
            .as_object_mut()
            .expect("state object")
            .remove("transient_split_count");
        let decoded_legacy = serde_json::from_value::<CloudSummaryPipelineState>(legacy)
            .expect("decode legacy state");
        assert_eq!(decoded_legacy, state);
    }

    #[test]
    fn every_oversized_leaf_chunk_keeps_a_frozen_source_ref() {
        let state = test_state(
            spec(vec!["超大工具输出".repeat(400)], CHUNK_TEST_CAPACITY),
            contract(),
        )
        .expect("pipeline state");

        assert!(state.groups.len() > 1);
        for group in &state.groups {
            assert!(group.iter().all(|item| item.contains("source-1")));
            let group_contract = state
                .payload_contract_for_group(group)
                .expect("every chunk remains attributable");
            assert_eq!(group_contract.source_refs, vec!["source-1"]);
        }
    }
}
