use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use chatos_mcp_runtime::{ToolCallContext, ToolResult, ToolResultCallback};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeMessage {
    pub role: String,
    pub content: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelRuntimeConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub provider: String,
    pub supports_responses: bool,
    pub instructions: Option<String>,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub thinking_level: Option<String>,
    pub prompt_cache_key: Option<String>,
    pub request_cwd: Option<String>,
    pub include_prompt_cache_retention: bool,
    pub request_body_limit_bytes: Option<usize>,
}

impl ModelRuntimeConfig {
    pub fn openai_compatible(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
        provider: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            provider: provider.into(),
            ..Self::default()
        }
    }

    pub fn with_responses_support(mut self, supports_responses: bool) -> Self {
        self.supports_responses = supports_responses;
        self
    }

    pub fn with_instructions(mut self, instructions: Option<String>) -> Self {
        self.instructions = instructions;
        self
    }

    pub fn with_temperature(mut self, temperature: Option<f64>) -> Self {
        self.temperature = temperature;
        self
    }

    pub fn with_max_output_tokens(mut self, max_output_tokens: Option<i64>) -> Self {
        self.max_output_tokens = max_output_tokens;
        self
    }

    pub fn with_thinking_level(mut self, thinking_level: Option<String>) -> Self {
        self.thinking_level = thinking_level;
        self
    }

    pub fn with_prompt_cache_key(mut self, prompt_cache_key: Option<String>) -> Self {
        self.prompt_cache_key = prompt_cache_key;
        self
    }

    pub fn with_request_cwd(mut self, request_cwd: Option<String>) -> Self {
        self.request_cwd = request_cwd;
        self
    }

    pub fn with_prompt_cache_retention(mut self, include_prompt_cache_retention: bool) -> Self {
        self.include_prompt_cache_retention = include_prompt_cache_retention;
        self
    }

    pub fn with_request_body_limit_bytes(
        mut self,
        request_body_limit_bytes: Option<usize>,
    ) -> Self {
        self.request_body_limit_bytes = request_body_limit_bytes;
        self
    }

    pub fn to_model_request(&self, input: Value, tools: Vec<Value>) -> ModelRequest {
        ModelRequest {
            input,
            model: self.model.clone(),
            provider: self.provider.clone(),
            base_url: self.base_url.clone(),
            api_key: self.api_key.clone(),
            supports_responses: self.supports_responses,
            instructions: self.instructions.clone(),
            tools,
            temperature: self.temperature,
            max_output_tokens: self.max_output_tokens,
            thinking_level: self.thinking_level.clone(),
            prompt_cache_key: self.prompt_cache_key.clone(),
            request_cwd: self.request_cwd.clone(),
            include_prompt_cache_retention: self.include_prompt_cache_retention,
            request_body_limit_bytes: self.request_body_limit_bytes,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub input: Value,
    pub model: String,
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub supports_responses: bool,
    pub instructions: Option<String>,
    pub tools: Vec<Value>,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub thinking_level: Option<String>,
    pub prompt_cache_key: Option<String>,
    pub request_cwd: Option<String>,
    pub include_prompt_cache_retention: bool,
    pub request_body_limit_bytes: Option<usize>,
}

impl ModelRequest {
    pub fn from_runtime_config(
        config: &ModelRuntimeConfig,
        input: Value,
        tools: Vec<Value>,
    ) -> Self {
        config.to_model_request(input, tools)
    }

    pub fn openai_compatible(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
        provider: impl Into<String>,
        input: Value,
    ) -> Self {
        Self {
            input,
            model: model.into(),
            provider: provider.into(),
            base_url: base_url.into(),
            api_key: api_key.into(),
            supports_responses: false,
            instructions: None,
            tools: Vec::new(),
            temperature: None,
            max_output_tokens: None,
            thinking_level: None,
            prompt_cache_key: None,
            request_cwd: None,
            include_prompt_cache_retention: false,
            request_body_limit_bytes: None,
        }
    }

    pub fn with_responses_support(mut self, supports_responses: bool) -> Self {
        self.supports_responses = supports_responses;
        self
    }

    pub fn with_instructions(mut self, instructions: Option<String>) -> Self {
        self.instructions = instructions;
        self
    }

    pub fn with_tools(mut self, tools: Vec<Value>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_temperature(mut self, temperature: Option<f64>) -> Self {
        self.temperature = temperature;
        self
    }

    pub fn with_max_output_tokens(mut self, max_output_tokens: Option<i64>) -> Self {
        self.max_output_tokens = max_output_tokens;
        self
    }

    pub fn with_thinking_level(mut self, thinking_level: Option<String>) -> Self {
        self.thinking_level = thinking_level;
        self
    }

    pub fn with_prompt_cache_key(mut self, prompt_cache_key: Option<String>) -> Self {
        self.prompt_cache_key = prompt_cache_key;
        self
    }

    pub fn with_request_cwd(mut self, request_cwd: Option<String>) -> Self {
        self.request_cwd = request_cwd;
        self
    }

    pub fn with_prompt_cache_retention(mut self, include_prompt_cache_retention: bool) -> Self {
        self.include_prompt_cache_retention = include_prompt_cache_retention;
        self
    }

    pub fn with_request_body_limit_bytes(
        mut self,
        request_body_limit_bytes: Option<usize>,
    ) -> Self {
        self.request_body_limit_bytes = request_body_limit_bytes;
        self
    }
}

#[derive(Clone, Default)]
pub struct RuntimeCallbacks {
    pub on_chunk: Option<std::sync::Arc<dyn Fn(String) + Send + Sync>>,
    pub on_thinking: Option<std::sync::Arc<dyn Fn(String) + Send + Sync>>,
    pub on_tools_start: Option<std::sync::Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_tools_stream: Option<std::sync::Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_tools_end: Option<std::sync::Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_before_model_request: Option<std::sync::Arc<dyn Fn(Value) + Send + Sync>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeRecordOptions {
    pub persist_assistant_records: bool,
    pub persist_tool_records: bool,
    pub assistant_message_mode: Option<String>,
    pub assistant_message_source: Option<String>,
    pub assistant_metadata: Option<Value>,
    pub tool_message_mode: Option<String>,
    pub tool_message_source: Option<String>,
    pub tool_metadata: Option<Value>,
}

impl RuntimeRecordOptions {
    pub fn persist_all() -> Self {
        Self {
            persist_assistant_records: true,
            persist_tool_records: true,
            ..Self::default()
        }
    }

    pub fn with_persist_assistant_records(mut self, persist: bool) -> Self {
        self.persist_assistant_records = persist;
        self
    }

    pub fn with_persist_tool_records(mut self, persist: bool) -> Self {
        self.persist_tool_records = persist;
        self
    }

    pub fn with_assistant_message_mode(mut self, mode: impl Into<String>) -> Self {
        self.assistant_message_mode = Some(mode.into());
        self
    }

    pub fn with_assistant_message_source(mut self, source: impl Into<String>) -> Self {
        self.assistant_message_source = Some(source.into());
        self
    }

    pub fn with_assistant_metadata(mut self, metadata: Value) -> Self {
        self.assistant_metadata = Some(metadata);
        self
    }

    pub fn with_tool_message_mode(mut self, mode: impl Into<String>) -> Self {
        self.tool_message_mode = Some(mode.into());
        self
    }

    pub fn with_tool_message_source(mut self, source: impl Into<String>) -> Self {
        self.tool_message_source = Some(source.into());
        self
    }

    pub fn with_tool_metadata(mut self, metadata: Value) -> Self {
        self.tool_metadata = Some(metadata);
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SaveRecordInput {
    pub conversation_id: String,
    pub conversation_turn_id: Option<String>,
    pub message_id: Option<String>,
    pub role: String,
    pub content: String,
    pub structured_payload: Option<Value>,
    pub metadata: Option<Value>,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub response_id: Option<String>,
    pub response_status: Option<String>,
    pub summary_status: Option<String>,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: Option<String>,
}

impl SaveRecordInput {
    pub fn message(
        conversation_id: impl Into<String>,
        role: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            conversation_id: conversation_id.into(),
            role: role.into(),
            content: content.into(),
            ..Self::default()
        }
    }

    pub fn user_message(conversation_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::message(conversation_id, "user", content)
    }

    pub fn assistant_message(
        conversation_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self::message(conversation_id, "assistant", content)
    }

    pub fn with_conversation_turn_id(mut self, conversation_turn_id: impl Into<String>) -> Self {
        self.conversation_turn_id = Some(conversation_turn_id.into());
        self
    }

    pub fn with_message_id(mut self, message_id: impl Into<String>) -> Self {
        self.message_id = Some(message_id.into());
        self
    }

    pub fn with_structured_payload(mut self, structured_payload: Value) -> Self {
        self.structured_payload = Some(structured_payload);
        self
    }

    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn with_message_mode(mut self, message_mode: impl Into<String>) -> Self {
        self.message_mode = Some(message_mode.into());
        self
    }

    pub fn with_message_source(mut self, message_source: impl Into<String>) -> Self {
        self.message_source = Some(message_source.into());
        self
    }

    pub fn with_created_at(mut self, created_at: impl Into<String>) -> Self {
        self.created_at = Some(created_at.into());
        self
    }

    pub fn packed_metadata(&self) -> Option<Value> {
        let mut map = metadata_object(self.metadata.clone());
        insert_non_empty(&mut map, "conversation_turn_id", &self.conversation_turn_id);
        insert_non_empty(&mut map, "message_mode", &self.message_mode);
        insert_non_empty(&mut map, "message_source", &self.message_source);
        insert_value(&mut map, "tool_calls", self.tool_calls.clone());
        insert_non_empty(&mut map, "tool_call_id", &self.tool_call_id);
        insert_non_empty(&mut map, "reasoning", &self.reasoning);
        insert_non_empty(&mut map, "response_id", &self.response_id);
        insert_non_empty(&mut map, "response_status", &self.response_status);
        if map.is_empty() {
            None
        } else {
            Some(Value::Object(map))
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SaveAssistantRecordInput {
    pub conversation_id: String,
    pub conversation_turn_id: Option<String>,
    pub message_id: Option<String>,
    pub content: String,
    pub reasoning: Option<String>,
    pub structured_payload: Option<Value>,
    pub metadata: Option<Value>,
    pub tool_calls: Option<Value>,
    pub response_id: Option<String>,
    pub response_status: Option<String>,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub summary_status: Option<String>,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: Option<String>,
}

impl From<SaveAssistantRecordInput> for SaveRecordInput {
    fn from(input: SaveAssistantRecordInput) -> Self {
        Self {
            conversation_id: input.conversation_id,
            conversation_turn_id: input.conversation_turn_id,
            message_id: input.message_id,
            role: "assistant".to_string(),
            content: input.content,
            structured_payload: input.structured_payload,
            metadata: input.metadata,
            message_mode: input.message_mode,
            message_source: input.message_source,
            tool_calls: input.tool_calls,
            tool_call_id: None,
            reasoning: input.reasoning,
            response_id: input.response_id,
            response_status: input.response_status,
            summary_status: input.summary_status,
            summary_id: input.summary_id,
            summarized_at: input.summarized_at,
            created_at: input.created_at,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SaveToolRecordInput {
    pub conversation_id: String,
    pub conversation_turn_id: Option<String>,
    pub message_id: Option<String>,
    pub tool_call_id: String,
    pub tool_name: String,
    pub content: String,
    pub success: bool,
    pub is_error: bool,
    pub is_stream: bool,
    pub structured_result: Option<Value>,
    pub metadata: Option<Value>,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub summary_status: Option<String>,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: Option<String>,
}

impl SaveToolRecordInput {
    pub fn from_tool_result(
        conversation_id: String,
        default_turn_id: Option<String>,
        result: &ToolResult,
    ) -> Self {
        Self {
            conversation_id,
            conversation_turn_id: result.conversation_turn_id.clone().or(default_turn_id),
            message_id: None,
            tool_call_id: result.tool_call_id.clone(),
            tool_name: result.name.clone(),
            content: result.content.clone(),
            success: result.success,
            is_error: result.is_error,
            is_stream: result.is_stream,
            structured_result: result.result.clone(),
            metadata: None,
            message_mode: None,
            message_source: None,
            summary_status: None,
            summary_id: None,
            summarized_at: None,
            created_at: None,
        }
    }
}

impl From<SaveToolRecordInput> for SaveRecordInput {
    fn from(input: SaveToolRecordInput) -> Self {
        let mut metadata = metadata_object(input.metadata);
        insert_non_empty(&mut metadata, "toolName", &Some(input.tool_name));
        metadata.insert("success".to_string(), Value::Bool(input.success));
        metadata.insert("isError".to_string(), Value::Bool(input.is_error));
        metadata.insert("isStream".to_string(), Value::Bool(input.is_stream));
        insert_value(
            &mut metadata,
            "structured_result",
            input.structured_result.clone(),
        );

        Self {
            conversation_id: input.conversation_id,
            conversation_turn_id: input.conversation_turn_id,
            message_id: input.message_id,
            role: "tool".to_string(),
            content: input.content,
            structured_payload: input.structured_result,
            metadata: if metadata.is_empty() {
                None
            } else {
                Some(Value::Object(metadata))
            },
            message_mode: input.message_mode,
            message_source: input.message_source,
            tool_calls: None,
            tool_call_id: Some(input.tool_call_id),
            reasoning: None,
            response_id: None,
            response_status: None,
            summary_status: input.summary_status,
            summary_id: input.summary_id,
            summarized_at: input.summarized_at,
            created_at: input.created_at,
        }
    }
}

#[async_trait]
pub trait MemoryRecordWriter: Send + Sync {
    async fn save_record(&self, input: SaveRecordInput) -> Result<(), String>;

    async fn save_assistant_record(&self, input: SaveAssistantRecordInput) -> Result<(), String> {
        self.save_record(input.into()).await
    }

    async fn save_tool_record(&self, input: SaveToolRecordInput) -> Result<(), String> {
        self.save_record(input.into()).await
    }

    async fn save_tool_records(&self, inputs: Vec<SaveToolRecordInput>) -> Result<(), String> {
        for input in inputs {
            self.save_tool_record(input).await?;
        }
        Ok(())
    }
}

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    fn available_tools(&self) -> Vec<Value>;

    async fn execute_tools_stream(
        &self,
        tool_calls: &[Value],
        context: ToolCallContext,
        on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult>;
}

fn metadata_object(metadata: Option<Value>) -> Map<String, Value> {
    match metadata {
        Some(Value::Object(map)) => map,
        Some(other) => {
            let mut map = Map::new();
            map.insert("metadata".to_string(), other);
            map
        }
        None => Map::new(),
    }
}

fn insert_non_empty(map: &mut Map<String, Value>, key: &str, value: &Option<String>) {
    if let Some(value) = value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn insert_value(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        map.insert(key.to_string(), value);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        ModelRequest, ModelRuntimeConfig, RuntimeRecordOptions, SaveAssistantRecordInput,
        SaveRecordInput,
    };

    #[test]
    fn model_runtime_config_builds_model_request() {
        let config = ModelRuntimeConfig::openai_compatible(
            "http://127.0.0.1:8080/v1",
            "secret",
            "gpt-test",
            "openai",
        )
        .with_responses_support(true)
        .with_instructions(Some("system prompt".to_string()))
        .with_temperature(Some(0.2))
        .with_max_output_tokens(Some(1024))
        .with_thinking_level(Some("medium".to_string()))
        .with_prompt_cache_key(Some("task-1".to_string()))
        .with_request_cwd(Some("/tmp/work".to_string()))
        .with_prompt_cache_retention(true)
        .with_request_body_limit_bytes(Some(2048));

        let request =
            ModelRequest::from_runtime_config(&config, json!("hello"), vec![json!({"name":"t"})]);

        assert_eq!(request.base_url, "http://127.0.0.1:8080/v1");
        assert_eq!(request.api_key, "secret");
        assert_eq!(request.model, "gpt-test");
        assert_eq!(request.provider, "openai");
        assert!(request.supports_responses);
        assert_eq!(request.instructions.as_deref(), Some("system prompt"));
        assert_eq!(request.temperature, Some(0.2));
        assert_eq!(request.max_output_tokens, Some(1024));
        assert_eq!(request.thinking_level.as_deref(), Some("medium"));
        assert_eq!(request.prompt_cache_key.as_deref(), Some("task-1"));
        assert_eq!(request.request_cwd.as_deref(), Some("/tmp/work"));
        assert!(request.include_prompt_cache_retention);
        assert_eq!(request.request_body_limit_bytes, Some(2048));
        assert_eq!(request.tools.len(), 1);
    }

    #[test]
    fn save_record_input_builders_pack_runtime_metadata() {
        let input = SaveRecordInput::user_message("task_1", "hello")
            .with_conversation_turn_id("run_1")
            .with_message_id("message_1")
            .with_message_mode("task")
            .with_message_source("task_runner")
            .with_metadata(json!({"task_id": "task_1"}));

        assert_eq!(input.role, "user");
        assert_eq!(input.content, "hello");
        assert_eq!(input.message_id.as_deref(), Some("message_1"));

        let metadata = input.packed_metadata().expect("metadata");
        assert_eq!(metadata["task_id"].as_str(), Some("task_1"));
        assert_eq!(metadata["conversation_turn_id"].as_str(), Some("run_1"));
        assert_eq!(metadata["message_mode"].as_str(), Some("task"));
        assert_eq!(metadata["message_source"].as_str(), Some("task_runner"));
    }

    #[test]
    fn runtime_record_options_builders_configure_persistence() {
        let options = RuntimeRecordOptions::default()
            .with_persist_assistant_records(true)
            .with_persist_tool_records(true)
            .with_assistant_message_mode("task_assistant")
            .with_assistant_message_source("task_runner")
            .with_assistant_metadata(json!({"kind": "assistant"}))
            .with_tool_message_mode("task_tool")
            .with_tool_message_source("task_runner")
            .with_tool_metadata(json!({"kind": "tool"}));

        assert!(options.persist_assistant_records);
        assert!(options.persist_tool_records);
        assert_eq!(
            options.assistant_message_mode.as_deref(),
            Some("task_assistant")
        );
        assert_eq!(options.tool_message_mode.as_deref(), Some("task_tool"));
        assert_eq!(
            options
                .assistant_metadata
                .as_ref()
                .and_then(|v| v["kind"].as_str()),
            Some("assistant")
        );
        assert_eq!(
            options
                .tool_metadata
                .as_ref()
                .and_then(|v| v["kind"].as_str()),
            Some("tool")
        );
    }

    #[test]
    fn assistant_record_input_preserves_structured_payload_and_tool_calls() {
        let tool_calls = json!([{
            "id": "call_1",
            "type": "function",
            "function": {
                "name": "demo.search",
                "arguments": "{\"q\":\"rust\"}"
            }
        }]);
        let record: SaveRecordInput = SaveAssistantRecordInput {
            conversation_id: "task_1".to_string(),
            conversation_turn_id: Some("run_1".to_string()),
            message_id: Some("message_1".to_string()),
            content: "calling tool".to_string(),
            reasoning: Some("need data".to_string()),
            structured_payload: Some(tool_calls.clone()),
            metadata: Some(json!({"task_id": "task_1"})),
            tool_calls: Some(tool_calls.clone()),
            response_id: Some("resp_1".to_string()),
            response_status: Some("tool_calls".to_string()),
            message_mode: Some("task_run".to_string()),
            message_source: Some("task_runner".to_string()),
            summary_status: None,
            summary_id: None,
            summarized_at: None,
            created_at: None,
        }
        .into();

        assert_eq!(record.role, "assistant");
        assert_eq!(record.structured_payload, Some(tool_calls.clone()));
        assert_eq!(record.tool_calls, Some(tool_calls));
        let metadata = record.packed_metadata().expect("metadata");
        assert_eq!(metadata["response_status"].as_str(), Some("tool_calls"));
    }
}
