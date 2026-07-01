// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

use chatos_mcp_runtime::ToolCallerModelRuntime;

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
    pub supports_images: Option<bool>,
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

    pub fn with_images_support(mut self, supports_images: Option<bool>) -> Self {
        self.supports_images = supports_images;
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

    pub fn to_tool_caller_model_runtime(&self) -> ToolCallerModelRuntime {
        ToolCallerModelRuntime::openai_compatible(
            self.base_url.clone(),
            self.api_key.clone(),
            self.model.clone(),
            self.provider.clone(),
        )
        .with_responses_support(self.supports_responses)
        .with_images_support(self.supports_images)
        .with_thinking_level(self.thinking_level.clone())
        .with_temperature(self.temperature)
        .with_instructions(self.instructions.clone())
        .with_max_output_tokens(self.max_output_tokens)
        .with_request_body_limit_bytes(self.request_body_limit_bytes)
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
