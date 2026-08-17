// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::memory_context::{MemoryContextComposer, MemoryScope};
use crate::runtime::{
    AiRuntime, AiRuntimeOptions, AiSingleStepOutcome, AiSingleStepRequest, IterativeContextRefresh,
    MemoryContextOverflowRecovery,
};
#[cfg(feature = "local-agent-loop")]
use crate::runtime::{AiRuntimeResult, AiTurnReport};
use crate::traits::{ModelRequest, ModelRuntimeConfig, RuntimeRecordOptions, SaveRecordInput};

pub struct ContextualTurnRunner {
    runtime: AiRuntime,
    memory_composer: Option<MemoryContextComposer>,
    context_overflow_recovery: Option<MemoryContextOverflowRecovery>,
}

#[derive(Clone)]
pub struct ContextualTurnRequest {
    pub model_request: ModelRequest,
    pub runtime_options: AiRuntimeOptions,
    pub memory_scope: Option<MemoryScope>,
    pub prefixed_input_items: Vec<Value>,
    pub current_input_items: Vec<Value>,
    pub user_record: Option<SaveRecordInput>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextualTurnExecutionOptions {
    pub force_non_stream: bool,
    pub force_identity_encoding: bool,
    pub thinking_level_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeTurnSpec {
    pub model_config: ModelRuntimeConfig,
    pub conversation_id: String,
    pub conversation_turn_id: Option<String>,
    pub caller_model: Option<String>,
    pub record_options: RuntimeRecordOptions,
    pub memory_scope: Option<MemoryScope>,
    pub prefixed_input_items: Vec<Value>,
    pub current_input_items: Vec<Value>,
    pub user_record: Option<SaveRecordInput>,
    pub tools: Vec<Value>,
}

impl ContextualTurnRunner {
    pub fn new(runtime: AiRuntime, memory_composer: Option<MemoryContextComposer>) -> Self {
        Self {
            runtime,
            memory_composer,
            context_overflow_recovery: None,
        }
    }

    pub fn runtime(&self) -> &AiRuntime {
        &self.runtime
    }

    pub async fn persist_external_tool_results(
        &self,
        runtime_options: &AiRuntimeOptions,
        tool_results: &[chatos_mcp_runtime::ToolResult],
    ) -> Result<(), String> {
        self.runtime
            .persist_external_tool_results(runtime_options, tool_results)
            .await
    }

    pub fn with_context_overflow_recovery(
        mut self,
        context_overflow_recovery: Option<MemoryContextOverflowRecovery>,
    ) -> Self {
        self.context_overflow_recovery = context_overflow_recovery;
        self
    }

    #[cfg(feature = "local-agent-loop")]
    pub async fn run_turn(
        &self,
        request: ContextualTurnRequest,
    ) -> Result<AiRuntimeResult, String> {
        let ContextualTurnRequest {
            mut model_request,
            runtime_options,
            memory_scope,
            prefixed_input_items,
            current_input_items,
            mut user_record,
        } = request;
        let contextual_input = build_contextual_input(
            self.memory_composer.as_ref(),
            memory_scope.as_ref(),
            prefixed_input_items.as_slice(),
            current_input_items.as_slice(),
            model_request.input.clone(),
            runtime_options.conversation_turn_id.as_deref(),
        )
        .await?;
        let iterative_context_refresh = self.build_iterative_context_refresh(
            &runtime_options,
            memory_scope.as_ref(),
            prefixed_input_items.as_slice(),
            current_input_items.as_slice(),
            &model_request.input,
        );

        if let Some(user_record) = user_record.take() {
            self.runtime.save_record(user_record).await?;
        }

        model_request.input = contextual_input;
        self.runtime
            .run_turn(
                model_request,
                runtime_options.with_iterative_context_refresh(iterative_context_refresh),
            )
            .await
    }

    pub async fn execute_once(
        &self,
        request: ContextualTurnRequest,
        iteration: usize,
        reason: impl Into<String>,
        model_attempt: usize,
    ) -> Result<AiSingleStepOutcome, String> {
        self.execute_once_with_options(
            request,
            iteration,
            reason,
            model_attempt,
            ContextualTurnExecutionOptions::default(),
        )
        .await
    }

    pub async fn execute_once_with_options(
        &self,
        request: ContextualTurnRequest,
        iteration: usize,
        reason: impl Into<String>,
        model_attempt: usize,
        execution_options: ContextualTurnExecutionOptions,
    ) -> Result<AiSingleStepOutcome, String> {
        let ContextualTurnRequest {
            mut model_request,
            runtime_options,
            memory_scope,
            prefixed_input_items,
            current_input_items,
            user_record,
        } = request;
        if let Some(thinking_level) = execution_options.thinking_level_override.clone() {
            model_request.thinking_level = Some(thinking_level);
        }
        let contextual_input = build_contextual_input(
            self.memory_composer.as_ref(),
            memory_scope.as_ref(),
            prefixed_input_items.as_slice(),
            current_input_items.as_slice(),
            model_request.input.clone(),
            runtime_options.conversation_turn_id.as_deref(),
        )
        .await?;
        let iterative_context_refresh = self.build_iterative_context_refresh(
            &runtime_options,
            memory_scope.as_ref(),
            prefixed_input_items.as_slice(),
            current_input_items.as_slice(),
            &model_request.input,
        );
        if let Some(user_record) = user_record {
            self.runtime.save_record(user_record).await?;
        }
        model_request.input = contextual_input;
        let single_step = AiSingleStepRequest {
            model_request,
            runtime_options: runtime_options
                .with_iterative_context_refresh(iterative_context_refresh),
            iteration,
            reason: reason.into(),
            model_attempt,
            force_non_stream: execution_options.force_non_stream,
            force_identity_encoding: execution_options.force_identity_encoding,
        };
        self.runtime.execute_once(single_step).await
    }

    #[cfg(feature = "local-agent-loop")]
    pub async fn run_turn_report(&self, request: ContextualTurnRequest) -> AiTurnReport {
        match self.run_turn(request).await {
            Ok(result) => result.into_report(),
            Err(err) => AiTurnReport::failed(err),
        }
    }
}

impl ContextualTurnRunner {
    fn build_iterative_context_refresh(
        &self,
        runtime_options: &AiRuntimeOptions,
        memory_scope: Option<&MemoryScope>,
        prefixed_input_items: &[Value],
        current_input_items: &[Value],
        fallback_input: &Value,
    ) -> Option<IterativeContextRefresh> {
        if self.memory_composer.is_none()
            || memory_scope.is_none()
            || !self.runtime.has_record_writer()
            || !runtime_options.record_options.persist_assistant_records
            || !runtime_options.record_options.persist_tool_records
        {
            return None;
        }

        // The current turn is the authoritative task contract. Memory context is
        // supplemental and can be empty or summarized, so never rely on
        // recomposition to restore the current task.
        let sticky_input_items = if current_input_items.is_empty() {
            input_value_to_items(fallback_input.clone())
        } else {
            current_input_items.to_vec()
        };

        Some(
            IterativeContextRefresh::new(
                self.memory_composer.clone(),
                memory_scope.cloned(),
                prefixed_input_items.to_vec(),
            )
            .with_sticky_input_items(sticky_input_items)
            .with_tool_result_model_budget_limits(runtime_options.tool_result_model_budget_limits)
            .with_context_overflow_recovery(self.context_overflow_recovery.clone()),
        )
    }
}

impl RuntimeTurnSpec {
    pub fn new(model_config: ModelRuntimeConfig, conversation_id: impl Into<String>) -> Self {
        Self {
            model_config,
            conversation_id: conversation_id.into(),
            conversation_turn_id: None,
            caller_model: None,
            record_options: RuntimeRecordOptions::default(),
            memory_scope: None,
            prefixed_input_items: Vec::new(),
            current_input_items: Vec::new(),
            user_record: None,
            tools: Vec::new(),
        }
    }

    pub fn for_user_text(
        model_config: ModelRuntimeConfig,
        conversation_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self::new(model_config, conversation_id)
            .with_current_input_items(vec![user_text_item(content)])
    }

    pub fn with_conversation_turn_id(mut self, conversation_turn_id: impl Into<String>) -> Self {
        self.conversation_turn_id = Some(conversation_turn_id.into());
        self
    }

    pub fn with_caller_model(mut self, caller_model: impl Into<String>) -> Self {
        self.caller_model = Some(caller_model.into());
        self
    }

    pub fn with_record_options(mut self, record_options: RuntimeRecordOptions) -> Self {
        self.record_options = record_options;
        self
    }

    pub fn with_memory_scope(mut self, memory_scope: Option<MemoryScope>) -> Self {
        self.memory_scope = memory_scope;
        self
    }

    pub fn with_prefixed_input_items(mut self, items: Vec<Value>) -> Self {
        self.prefixed_input_items = items;
        self
    }

    pub fn with_current_input_items(mut self, items: Vec<Value>) -> Self {
        self.current_input_items = items;
        self
    }

    pub fn with_user_record(mut self, user_record: Option<SaveRecordInput>) -> Self {
        self.user_record = user_record;
        self
    }

    pub fn with_tools(mut self, tools: Vec<Value>) -> Self {
        self.tools = tools;
        self
    }

    pub fn runtime_options(&self) -> AiRuntimeOptions {
        AiRuntimeOptions::new(
            Some(self.conversation_id.clone()),
            self.conversation_turn_id.clone(),
        )
        .with_caller_model(self.caller_model.clone())
        .with_caller_model_runtime(Some(self.model_config.to_tool_caller_model_runtime()))
        .with_record_options(self.record_options.clone())
    }

    pub fn into_contextual_turn_request(self) -> ContextualTurnRequest {
        let model_request = self
            .model_config
            .to_model_request(Value::Null, self.tools.clone());
        ContextualTurnRequest {
            model_request,
            runtime_options: self.runtime_options(),
            memory_scope: self.memory_scope,
            prefixed_input_items: self.prefixed_input_items,
            current_input_items: self.current_input_items,
            user_record: self.user_record,
        }
    }
}

impl ContextualTurnRequest {
    pub fn new(
        model_request: ModelRequest,
        runtime_options: AiRuntimeOptions,
        current_input_items: Vec<Value>,
    ) -> Self {
        Self {
            model_request,
            runtime_options,
            memory_scope: None,
            prefixed_input_items: Vec::new(),
            current_input_items,
            user_record: None,
        }
    }

    pub fn from_model_config(
        model_config: &ModelRuntimeConfig,
        runtime_options: AiRuntimeOptions,
        current_input_items: Vec<Value>,
    ) -> Self {
        Self::new(
            model_config.to_model_request(Value::Null, Vec::new()),
            runtime_options,
            current_input_items,
        )
    }

    pub fn for_user_text(
        model_config: &ModelRuntimeConfig,
        runtime_options: AiRuntimeOptions,
        content: impl Into<String>,
    ) -> Self {
        Self::from_model_config(model_config, runtime_options, vec![user_text_item(content)])
    }

    pub fn with_memory_scope(mut self, memory_scope: Option<MemoryScope>) -> Self {
        self.memory_scope = memory_scope;
        self
    }

    pub fn with_current_input_items(mut self, items: Vec<Value>) -> Self {
        self.current_input_items = items;
        self
    }

    pub fn with_prefixed_input_items(mut self, items: Vec<Value>) -> Self {
        self.prefixed_input_items = items;
        self
    }

    pub fn with_user_record(mut self, user_record: Option<SaveRecordInput>) -> Self {
        self.user_record = user_record;
        self
    }
}

pub async fn build_contextual_input(
    memory_composer: Option<&MemoryContextComposer>,
    memory_scope: Option<&MemoryScope>,
    prefixed_input_items: &[Value],
    current_input_items: &[Value],
    fallback_input: Value,
    excluded_memory_turn_id: Option<&str>,
) -> Result<Value, String> {
    let current_items = if current_input_items.is_empty() {
        input_value_to_items(fallback_input)
    } else {
        current_input_items.to_vec()
    };
    let has_durable_response_history = current_items.iter().any(is_responses_output_or_result_item);
    let memory_items = if let (Some(composer), Some(scope)) = (memory_composer, memory_scope) {
        composer
            .compose_input_items_excluding_turn(scope, excluded_memory_turn_id, None)
            .await?
    } else {
        Vec::new()
    };

    let mut items = if has_durable_response_history {
        // The first request's exact input is now an immutable cacheable prefix.
        // Memory Engine is still composed on every turn; only genuinely new
        // items are appended so prior prompt-cache prefixes are never rewritten.
        current_items
    } else {
        let mut initial = prefixed_input_items.to_vec();
        initial.extend(memory_items.iter().cloned());
        initial.extend(current_items);
        initial
    };
    if has_durable_response_history {
        for item in prefixed_input_items.iter().chain(memory_items.iter()) {
            if !items.contains(item) {
                items.push(item.clone());
            }
        }
    }

    Ok(Value::Array(items))
}

fn is_responses_output_or_result_item(item: &Value) -> bool {
    matches!(
        item.get("type").and_then(Value::as_str),
        Some("reasoning")
            | Some("reasoning_summary")
            | Some("function_call")
            | Some("function_call_output")
            | Some("computer_call")
            | Some("computer_call_output")
            | Some("web_search_call")
            | Some("file_search_call")
    ) || (item.get("type").and_then(Value::as_str) == Some("message") && item.get("id").is_some())
}

pub fn input_value_to_items(input: Value) -> Vec<Value> {
    match input {
        Value::Array(items) => items,
        Value::String(text) => vec![message_item("user", Value::String(text))],
        Value::Null => Vec::new(),
        other => vec![message_item("user", other)],
    }
}

pub fn user_text_item(content: impl Into<String>) -> Value {
    message_item("user", Value::String(content.into()))
}

pub fn message_item(role: &str, content: Value) -> Value {
    json!({
        "role": role,
        "content": content
    })
}

#[cfg(all(test, feature = "local-agent-loop"))]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
    use axum::extract::State;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::{json, Value};

    use super::{
        build_contextual_input, input_value_to_items, user_text_item, ContextualTurnRequest,
        RuntimeTurnSpec,
    };
    use crate::{
        AiRuntime, AiRuntimeOptions, AiTurnStatus, MemoryContextComposer, MemoryScope,
        ModelRuntimeConfig, RuntimeRecordOptions, SaveRecordInput, SaveToolRecordInput,
    };

    #[derive(Clone)]
    struct NoopRecordWriter;

    #[async_trait]
    impl crate::MemoryRecordWriter for NoopRecordWriter {
        async fn save_record(&self, _input: SaveRecordInput) -> Result<(), String> {
            Ok(())
        }

        async fn save_tool_record(&self, _input: SaveToolRecordInput) -> Result<(), String> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn build_contextual_input_orders_prefix_memory_and_current_items() {
        let input = build_contextual_input(
            None,
            None,
            &[json!({"role":"system","content":"prefix"})],
            &[json!({"role":"user","content":"current"})],
            json!("fallback"),
            None,
        )
        .await
        .expect("contextual input");

        let items = input.as_array().expect("items");
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].get("content").and_then(Value::as_str),
            Some("prefix")
        );
        assert_eq!(
            items[1].get("content").and_then(Value::as_str),
            Some("current")
        );
    }

    #[tokio::test]
    async fn build_contextual_input_uses_fallback_when_current_is_empty() {
        let input = build_contextual_input(None, None, &[], &[], json!("fallback"), None)
            .await
            .expect("contextual input");

        let items = input.as_array().expect("items");
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].get("content").and_then(Value::as_str),
            Some("fallback")
        );
    }

    #[tokio::test]
    async fn durable_responses_history_remains_an_immutable_cache_prefix() {
        let original = json!({"role":"user","content":"implement inventory cli"});
        let reasoning = json!({"type":"reasoning","id":"rs-1","summary":[]});
        let call = json!({"type":"function_call","id":"fc-1","call_id":"call-1","name":"read_file","arguments":"{}"});
        let output = json!({"type":"function_call_output","call_id":"call-1","output":"README"});
        let durable = vec![
            original.clone(),
            reasoning.clone(),
            call.clone(),
            output.clone(),
        ];

        let input = build_contextual_input(
            None,
            None,
            &[json!({"role":"system","content":"stable prompt"})],
            durable.as_slice(),
            Value::Null,
            Some("run-1"),
        )
        .await
        .expect("contextual input");
        let items = input.as_array().expect("items");

        assert_eq!(&items[..durable.len()], durable.as_slice());
        assert_eq!(items.last().unwrap()["content"], "stable prompt");
    }

    #[tokio::test]
    async fn every_model_input_composition_fetches_latest_memory_engine_context() {
        async fn compose(State(calls): State<Arc<AtomicUsize>>) -> Json<Value> {
            let call = calls.fetch_add(1, Ordering::SeqCst) + 1;
            Json(json!({
                "thread_id": "thread-1",
                "blocks": [{"block_type": "memory", "text": format!("memory-{call}")}],
                "recent_records": [],
                "meta": {"summary_count": 1, "recent_record_count": 0}
            }))
        }

        let calls = Arc::new(AtomicUsize::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind memory engine mock");
        let address = listener.local_addr().expect("memory engine mock address");
        let server_calls = Arc::clone(&calls);
        let server = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                Router::new()
                    .route("/api/memory-engine/v1/context/compose", post(compose))
                    .with_state(server_calls),
            )
            .await;
        });
        let composer = MemoryContextComposer::new_direct(
            format!("http://{address}"),
            Duration::from_secs(1),
            "task_runner",
        )
        .expect("memory composer");
        let scope = MemoryScope::thread("tenant-1", "task_runner", "thread-1");

        let first = build_contextual_input(
            Some(&composer),
            Some(&scope),
            &[],
            &[user_text_item("first")],
            Value::Null,
            Some("run-1"),
        )
        .await
        .expect("first model input");
        let second = build_contextual_input(
            Some(&composer),
            Some(&scope),
            &[],
            &[user_text_item("second")],
            Value::Null,
            Some("run-1"),
        )
        .await
        .expect("second model input");
        server.abort();

        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(first.to_string().contains("memory-1"));
        assert!(second.to_string().contains("memory-2"));
    }

    #[test]
    fn input_value_to_items_wraps_text_as_user_message() {
        let items = input_value_to_items(json!("hello"));
        assert_eq!(items, vec![user_text_item("hello")]);
    }

    #[test]
    fn contextual_turn_request_builds_from_model_config_and_user_text() {
        let config = ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:8080/v1",
            "secret",
            "gpt-test",
            "openai",
        );
        let runtime_options =
            AiRuntimeOptions::for_conversation("task_1").with_conversation_turn_id("run_1");

        let request =
            ContextualTurnRequest::for_user_text(&config, runtime_options, "run this task")
                .with_user_record(Some(
                    SaveRecordInput::user_message("task_1", "run this task")
                        .with_conversation_turn_id("run_1"),
                ));

        assert_eq!(request.model_request.model, "gpt-test");
        assert_eq!(
            request.runtime_options.conversation_id.as_deref(),
            Some("task_1")
        );
        assert_eq!(
            request.runtime_options.conversation_turn_id.as_deref(),
            Some("run_1")
        );
        assert_eq!(
            request.current_input_items,
            vec![user_text_item("run this task")]
        );
        assert!(request.user_record.is_some());
    }

    #[tokio::test]
    async fn contextual_turn_runner_report_captures_aborted_runtime() {
        let config = ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:1/v1",
            "secret",
            "gpt-test",
            "openai",
        );
        let runtime_options = AiRuntimeOptions::for_conversation("task_1")
            .with_abort_checker(Some(std::sync::Arc::new(|_| true)));
        let request =
            ContextualTurnRequest::for_user_text(&config, runtime_options, "run this task");
        let runner = super::ContextualTurnRunner::new(AiRuntime::new(None), None);

        let report = runner.run_turn_report(request).await;

        assert_eq!(report.status, AiTurnStatus::Aborted);
        assert_eq!(report.error.as_deref(), Some("aborted"));
    }

    #[test]
    fn contextual_turn_runner_enables_iterative_refresh_with_memory_and_records() {
        let runtime = AiRuntime::new(None).with_record_writer(Some(Arc::new(NoopRecordWriter)));
        let composer = MemoryContextComposer::new_direct(
            "http://127.0.0.1:1",
            Duration::from_millis(100),
            "task_runner",
        )
        .expect("composer");
        let runner = super::ContextualTurnRunner::new(runtime, Some(composer));
        let runtime_options = AiRuntimeOptions::for_conversation("task_1")
            .with_record_options(RuntimeRecordOptions::persist_all());
        let refresh = runner.build_iterative_context_refresh(
            &runtime_options,
            Some(&MemoryScope::thread("tenant_1", "task_runner", "task_1")),
            &[json!({"role":"system","content":"prefix"})],
            &[json!({"role":"user","content":"current"})],
            &Value::Null,
        );

        assert!(refresh.is_some());
    }

    #[test]
    fn contextual_turn_runner_skips_iterative_refresh_without_record_writer() {
        let composer = MemoryContextComposer::new_direct(
            "http://127.0.0.1:1",
            Duration::from_millis(100),
            "task_runner",
        )
        .expect("composer");
        let runner = super::ContextualTurnRunner::new(AiRuntime::new(None), Some(composer));
        let runtime_options = AiRuntimeOptions::for_conversation("task_1")
            .with_record_options(RuntimeRecordOptions::persist_all());
        let refresh = runner.build_iterative_context_refresh(
            &runtime_options,
            Some(&MemoryScope::thread("tenant_1", "task_runner", "task_1")),
            &[json!({"role":"system","content":"prefix"})],
            &[json!({"role":"user","content":"current"})],
            &Value::Null,
        );

        assert!(refresh.is_none());
    }

    #[test]
    fn runtime_turn_spec_roundtrips_and_builds_contextual_request() {
        let config = ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:8080/v1",
            "secret",
            "gpt-test",
            "openai",
        )
        .with_responses_support(true);
        let spec = RuntimeTurnSpec::for_user_text(config, "task_1", "run this task")
            .with_conversation_turn_id("run_1")
            .with_caller_model("gpt-test")
            .with_record_options(RuntimeRecordOptions::persist_all())
            .with_memory_scope(Some(
                MemoryScope::thread("tenant_1", "task_runner", "task_1")
                    .with_subject_id("contact_1"),
            ))
            .with_prefixed_input_items(vec![json!({"role":"system","content":"prefix"})])
            .with_user_record(Some(
                SaveRecordInput::user_message("task_1", "run this task")
                    .with_conversation_turn_id("run_1"),
            ))
            .with_tools(vec![json!({"type":"function","name":"tool_1"})]);

        let encoded = serde_json::to_string(&spec).expect("serialize spec");
        let decoded: RuntimeTurnSpec =
            serde_json::from_str(encoded.as_str()).expect("deserialize spec");
        let request = decoded.into_contextual_turn_request();

        assert_eq!(request.model_request.model, "gpt-test");
        assert!(request.model_request.supports_responses);
        assert_eq!(request.model_request.tools.len(), 1);
        assert_eq!(
            request.runtime_options.conversation_id.as_deref(),
            Some("task_1")
        );
        assert_eq!(
            request.runtime_options.conversation_turn_id.as_deref(),
            Some("run_1")
        );
        assert!(
            request
                .runtime_options
                .record_options
                .persist_assistant_records
        );
        assert_eq!(
            request
                .memory_scope
                .as_ref()
                .and_then(|scope| scope.subject_id.as_deref()),
            Some("contact_1")
        );
        assert_eq!(
            request.prefixed_input_items[0]["content"].as_str(),
            Some("prefix")
        );
        assert_eq!(
            request.current_input_items,
            vec![user_text_item("run this task")]
        );
        assert!(request.user_record.is_some());
    }
}
