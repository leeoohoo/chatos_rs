use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::memory_context::{MemoryContextComposer, MemoryScope};
use crate::runtime::{
    AiRuntime, AiRuntimeOptions, AiRuntimeResult, AiTurnReport, IterativeContextRefresh,
    MemoryContextOverflowRecovery,
};
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

    pub fn with_context_overflow_recovery(
        mut self,
        context_overflow_recovery: Option<MemoryContextOverflowRecovery>,
    ) -> Self {
        self.context_overflow_recovery = context_overflow_recovery;
        self
    }

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
        )
        .await?;
        let iterative_context_refresh = self.build_iterative_context_refresh(
            &runtime_options,
            memory_scope.as_ref(),
            prefixed_input_items.as_slice(),
            current_input_items.as_slice(),
            &model_request.input,
            user_record.is_some(),
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
        user_record_is_persisted: bool,
    ) -> Option<IterativeContextRefresh> {
        if self.memory_composer.is_none()
            || memory_scope.is_none()
            || !self.runtime.has_record_writer()
            || !runtime_options.record_options.persist_assistant_records
            || !runtime_options.record_options.persist_tool_records
        {
            return None;
        }

        let sticky_input_items = if user_record_is_persisted {
            Vec::new()
        } else if current_input_items.is_empty() {
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
) -> Result<Value, String> {
    let mut items = Vec::new();
    items.extend(prefixed_input_items.iter().cloned());

    if let (Some(composer), Some(scope)) = (memory_composer, memory_scope) {
        items.extend(composer.compose_input_items(scope).await?);
    }

    if current_input_items.is_empty() {
        items.extend(input_value_to_items(fallback_input));
    } else {
        items.extend(current_input_items.iter().cloned());
    }

    Ok(Value::Array(items))
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
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
        let input = build_contextual_input(None, None, &[], &[], json!("fallback"))
            .await
            .expect("contextual input");

        let items = input.as_array().expect("items");
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].get("content").and_then(Value::as_str),
            Some("fallback")
        );
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
            true,
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
            true,
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
