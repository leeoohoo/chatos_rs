use std::collections::HashSet;
use std::sync::Arc;

use serde_json::{json, Value};
use tracing::info;
use tracing::warn;

use crate::services::ai_common::{
    build_aborted_tool_results, build_tool_stream_callback, completion_failed_error,
};
use crate::services::user_settings::AiClientSettings;
use crate::services::v3::ai_request_handler::{AiRequestHandler, StreamCallbacks};
use crate::services::v3::mcp_tool_execute::McpToolExecute;
use crate::services::v3::message_manager::MessageManager;
use crate::utils::abort_registry;

mod compat;
mod input_transform;
mod prev_context;
mod recovery_policy;
mod stateless_context;
mod tool_plan;

use self::compat::{
    cap_tool_output_for_input, log_usage_snapshot, rewrite_system_messages_to_user,
    truncate_function_call_outputs_in_input,
};
use self::input_transform::{
    build_current_input_items, extract_raw_input, normalize_input_for_provider,
    normalize_input_to_text_value, to_message_item,
};
use self::prev_context::{
    base_url_allows_prev, base_url_disallows_system_messages, should_use_prev_id_for_next_turn,
};
use self::tool_plan::{
    build_tool_call_execution_plan, build_tool_call_items, expand_tool_results_with_aliases,
};

#[derive(Clone)]
pub struct AiClientCallbacks {
    pub on_chunk: Option<Arc<dyn Fn(String) + Send + Sync>>,
    pub on_thinking: Option<Arc<dyn Fn(String) + Send + Sync>>,
    pub on_tools_start: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_tools_stream: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_tools_end: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_context_summarized_start: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_context_summarized_stream: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    pub on_context_summarized_end: Option<Arc<dyn Fn(Value) + Send + Sync>>,
}

#[derive(Default)]
pub struct ProcessOptions {
    pub model: Option<String>,
    pub provider: Option<String>,
    pub thinking_level: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i64>,
    pub reasoning_enabled: Option<bool>,
    pub system_prompt: Option<String>,
    pub history_limit: Option<i64>,
    pub purpose: Option<String>,
    pub conversation_turn_id: Option<String>,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub sub_agent_run_id: Option<String>,
    pub callbacks: Option<AiClientCallbacks>,
}

pub struct AiClient {
    ai_request_handler: AiRequestHandler,
    mcp_tool_execute: McpToolExecute,
    message_manager: MessageManager,
    max_iterations: i64,
    history_limit: i64,
    system_prompt: Option<String>,
    prev_response_id_disabled_sessions: HashSet<String>,
    force_text_content_sessions: HashSet<String>,
    no_system_message_sessions: HashSet<String>,
}

impl AiClient {
    pub fn new(
        ai_request_handler: AiRequestHandler,
        mcp_tool_execute: McpToolExecute,
        message_manager: MessageManager,
    ) -> Self {
        Self {
            ai_request_handler,
            mcp_tool_execute,
            message_manager,
            max_iterations: 25,
            history_limit: 20,
            system_prompt: None,
            prev_response_id_disabled_sessions: HashSet::new(),
            force_text_content_sessions: HashSet::new(),
            no_system_message_sessions: HashSet::new(),
        }
    }

    pub fn set_system_prompt(&mut self, prompt: Option<String>) {
        self.system_prompt = prompt;
    }

    pub async fn process_request(
        &mut self,
        messages: Vec<Value>,
        session_id: Option<String>,
        options: ProcessOptions,
    ) -> Result<Value, String> {
        let model = options.model.unwrap_or_else(|| "gpt-4o".to_string());
        let provider = options.provider.unwrap_or_else(|| "gpt".to_string());
        let thinking_level = options.thinking_level.clone();
        let temperature = options.temperature.unwrap_or(0.7);
        let max_tokens = options.max_tokens;
        let reasoning_enabled = options.reasoning_enabled.unwrap_or(true);
        let system_prompt = options.system_prompt.or_else(|| self.system_prompt.clone());
        let history_limit = options.history_limit.unwrap_or(self.history_limit);
        let purpose = options.purpose.unwrap_or_else(|| "chat".to_string());
        let message_mode = options.message_mode;
        let message_source = options.message_source;
        let sub_agent_run_id = options
            .sub_agent_run_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());
        let turn_id = options
            .conversation_turn_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());
        let callbacks = options.callbacks.unwrap_or_else(|| AiClientCallbacks {
            on_chunk: None,
            on_thinking: None,
            on_tools_start: None,
            on_tools_stream: None,
            on_tools_end: None,
            on_context_summarized_start: None,
            on_context_summarized_stream: None,
            on_context_summarized_end: None,
        });
        let stable_prefix_mode = purpose == "chat" || sub_agent_run_id.is_some();

        // Chat mode favors a stable stateless prefix so provider-side prompt caching can reuse
        // a bounded recent window even when async summary jobs are updating session summaries.
        let prefer_stateless = if purpose == "chat" || sub_agent_run_id.is_some() {
            true
        } else {
            history_limit != 0
        };
        let mut previous_response_id: Option<String> = None;
        if !prefer_stateless {
            if let Some(sid) = session_id.as_ref() {
                let limit = if history_limit > 0 {
                    Some(history_limit)
                } else {
                    None
                };
                previous_response_id = self
                    .message_manager
                    .get_last_response_id(sid, limit.unwrap_or(50))
                    .await;
            }
        }

        let raw_input = extract_raw_input(&messages);
        let force_text_content = session_id
            .as_ref()
            .map(|s| self.force_text_content_sessions.contains(s))
            .unwrap_or(false);
        let available_tools = self.mcp_tool_execute.get_available_tools();
        let include_tool_items = !available_tools.is_empty();

        let allow_prev_id = session_id
            .as_ref()
            .map(|s| !self.prev_response_id_disabled_sessions.contains(s))
            .unwrap_or(true);
        let provider_allows_prev =
            provider == "gpt" && base_url_allows_prev(self.ai_request_handler.base_url());
        let can_use_prev_id = allow_prev_id && provider_allows_prev;
        let use_prev_id = !prefer_stateless && previous_response_id.is_some() && can_use_prev_id;
        let stateless_history_limit = if !use_prev_id && history_limit == 0 {
            warn!("[AI_V3] history_limit=0 with stateless mode; fallback to 20");
            20
        } else {
            history_limit
        };
        info!(
            "[AI_V3] context mode: use_prev_id={}, can_use_prev_id={}, provider={}, history_limit={}, has_prev_id={}, stable_prefix_mode={}",
            use_prev_id,
            can_use_prev_id,
            provider,
            stateless_history_limit,
            previous_response_id.is_some(),
            stable_prefix_mode
        );
        let initial_input = if use_prev_id {
            normalize_input_for_provider(&raw_input, force_text_content)
        } else {
            let current_items = build_current_input_items(&raw_input, force_text_content);
            Value::Array(
                self.build_stateless_items(
                    session_id.clone(),
                    stateless_history_limit,
                    stable_prefix_mode,
                    force_text_content,
                    &current_items,
                    include_tool_items,
                    sub_agent_run_id.clone(),
                )
                .await,
            )
        };

        let result = self
            .process_with_tools(
                initial_input,
                previous_response_id,
                available_tools,
                session_id,
                turn_id,
                model,
                provider,
                thinking_level,
                temperature,
                max_tokens,
                callbacks,
                reasoning_enabled,
                system_prompt,
                &purpose,
                0,
                use_prev_id,
                can_use_prev_id,
                raw_input,
                stateless_history_limit,
                stable_prefix_mode,
                force_text_content,
                prefer_stateless,
                message_mode,
                message_source,
                sub_agent_run_id,
            )
            .await;

        result
    }

    async fn process_with_tools(
        &mut self,
        input: Value,
        previous_response_id: Option<String>,
        tools: Vec<Value>,
        session_id: Option<String>,
        turn_id: Option<String>,
        model: String,
        provider: String,
        thinking_level: Option<String>,
        temperature: f64,
        max_tokens: Option<i64>,
        callbacks: AiClientCallbacks,
        reasoning_enabled: bool,
        system_prompt: Option<String>,
        purpose: &str,
        iteration: i64,
        use_prev_id: bool,
        can_use_prev_id: bool,
        raw_input: Value,
        history_limit: i64,
        stable_prefix_mode: bool,
        force_text_content: bool,
        prefer_stateless: bool,
        message_mode: Option<String>,
        message_source: Option<String>,
        sub_agent_run_id: Option<String>,
    ) -> Result<Value, String> {
        let include_tool_items = !tools.is_empty();
        let persist_tool_messages = purpose != "sub_agent_router";
        let mut input = input;
        let mut previous_response_id = previous_response_id;
        let mut use_prev_id = use_prev_id;
        let mut can_use_prev_id = can_use_prev_id;
        let mut force_text_content = force_text_content;
        let mut adaptive_history_limit = history_limit;
        let mut iteration = iteration;
        let mut pending_tool_outputs: Option<Vec<Value>> = None;
        let mut pending_tool_calls: Option<Vec<Value>> = None;
        let mut no_system_messages =
            base_url_disallows_system_messages(self.ai_request_handler.base_url())
                || session_id
                    .as_ref()
                    .map(|sid| self.no_system_message_sessions.contains(sid))
                    .unwrap_or(false);
        let mut stateless_context_items = if !use_prev_id {
            input.as_array().cloned()
        } else {
            None
        };

        loop {
            if let Some(sid) = session_id.as_ref() {
                if abort_registry::is_aborted(sid) {
                    return Err("aborted".to_string());
                }
            }
            if iteration >= self.max_iterations {
                return Err("达到最大迭代次数".to_string());
            }

            info!("AI_V3 request iteration {}", iteration);

            // In chat/stateless mode, refresh context from persisted summary+pending messages
            // before each model request so newly generated summaries are reflected immediately.
            self.maybe_refresh_stateless_context(
                session_id.as_deref(),
                sub_agent_run_id.as_deref(),
                stable_prefix_mode,
                use_prev_id,
                &raw_input,
                force_text_content,
                adaptive_history_limit,
                include_tool_items,
                &mut stateless_context_items,
                &mut input,
            )
            .await;

            let mut ai_response = None;
            let mut last_error: Option<String> = None;

            for _attempt in 0..3 {
                let request_input = if no_system_messages {
                    rewrite_system_messages_to_user(&input, force_text_content)
                } else {
                    input.clone()
                };
                let req = self
                    .ai_request_handler
                    .handle_request(
                        request_input,
                        model.clone(),
                        system_prompt.clone(),
                        if use_prev_id {
                            previous_response_id.clone()
                        } else {
                            None
                        },
                        if tools.is_empty() {
                            None
                        } else {
                            Some(tools.clone())
                        },
                        Some(temperature),
                        max_tokens,
                        StreamCallbacks {
                            on_chunk: callbacks.on_chunk.clone(),
                            on_thinking: if reasoning_enabled {
                                callbacks.on_thinking.clone()
                            } else {
                                None
                            },
                        },
                        Some(provider.clone()),
                        thinking_level.clone(),
                        session_id.clone(),
                        turn_id.clone(),
                        callbacks.on_chunk.is_some() || callbacks.on_thinking.is_some(),
                        message_mode.clone(),
                        message_source.clone(),
                        purpose,
                    )
                    .await;

                match req {
                    Ok(resp) => {
                        ai_response = Some(resp);
                        last_error = None;
                        break;
                    }
                    Err(err) => {
                        let err_msg = err.clone();
                        last_error = Some(err_msg.clone());
                        if self
                            .try_recover_from_request_error(
                                err_msg.as_str(),
                                session_id.as_ref(),
                                sub_agent_run_id.as_ref(),
                                &raw_input,
                                stable_prefix_mode,
                                include_tool_items,
                                pending_tool_calls.as_ref(),
                                pending_tool_outputs.as_ref(),
                                &mut use_prev_id,
                                &mut can_use_prev_id,
                                &mut force_text_content,
                                &mut adaptive_history_limit,
                                &mut previous_response_id,
                                &mut no_system_messages,
                                &mut stateless_context_items,
                                &mut input,
                            )
                            .await
                        {
                            continue;
                        }
                        break;
                    }
                }
            }

            let ai_response = match ai_response {
                Some(resp) => resp,
                None => return Err(last_error.unwrap_or_else(|| "request failed".to_string())),
            };
            log_usage_snapshot(purpose, ai_response.usage.as_ref());

            if let Some(err) = completion_failed_error(
                ai_response.finish_reason.as_deref(),
                ai_response.content.as_str(),
                ai_response.reasoning.as_deref(),
                ai_response.provider_error.as_ref(),
            ) {
                if self
                    .try_recover_from_completion_error(
                        err.as_str(),
                        session_id.as_ref(),
                        sub_agent_run_id.as_ref(),
                        &raw_input,
                        stable_prefix_mode,
                        include_tool_items,
                        pending_tool_calls.as_ref(),
                        pending_tool_outputs.as_ref(),
                        force_text_content,
                        &mut adaptive_history_limit,
                        &mut use_prev_id,
                        &mut can_use_prev_id,
                        &mut previous_response_id,
                        &mut stateless_context_items,
                        &mut input,
                    )
                    .await
                {
                    continue;
                }
                return Err(err);
            }

            if let Some(sid) = session_id.as_ref() {
                if abort_registry::is_aborted(sid) {
                    return Err("aborted".to_string());
                }
            }

            let tool_calls = ai_response.tool_calls.clone();
            if tool_calls
                .as_ref()
                .and_then(|v| v.as_array())
                .map(|a| a.is_empty())
                .unwrap_or(true)
            {
                return Ok(json!({
                    "success": true,
                    "content": ai_response.content,
                    "reasoning": ai_response.reasoning,
                    "tool_calls": Value::Null,
                    "finish_reason": ai_response.finish_reason,
                    "iteration": iteration
                }));
            }

            let raw_tool_calls = tool_calls.unwrap_or(Value::Array(vec![]));
            let tool_calls_arr = raw_tool_calls.as_array().cloned().unwrap_or_default();
            let execution_plan = build_tool_call_execution_plan(&tool_calls_arr);
            let display_tool_calls = Value::Array(execution_plan.display_calls.clone());

            if let Some(cb) = &callbacks.on_tools_start {
                cb(display_tool_calls);
            }
            let tool_call_items = build_tool_call_items(&tool_calls_arr);

            if let Some(sid) = session_id.as_ref() {
                if abort_registry::is_aborted(sid) {
                    if persist_tool_messages {
                        let aborted_results = build_aborted_tool_results(&tool_calls_arr, None);
                        self.message_manager
                            .save_tool_results(sid, aborted_results.as_slice())
                            .await;
                    }
                    return Err("aborted".to_string());
                }
            }

            let on_tools_stream_cb =
                build_tool_stream_callback(callbacks.on_tools_stream.clone(), session_id.clone());

            let tool_results = self
                .mcp_tool_execute
                .execute_tools_stream(
                    &execution_plan.execute_calls,
                    session_id.as_deref(),
                    turn_id.as_deref(),
                    Some(model.as_str()),
                    on_tools_stream_cb,
                )
                .await;
            let expanded_tool_results = expand_tool_results_with_aliases(
                tool_results.as_slice(),
                &execution_plan.alias_map,
            );

            if let Some(sid) = session_id.as_ref() {
                if abort_registry::is_aborted(sid) {
                    if persist_tool_messages {
                        let aborted_results = build_aborted_tool_results(
                            &tool_calls_arr,
                            Some(expanded_tool_results.as_slice()),
                        );
                        self.message_manager
                            .save_tool_results(sid, aborted_results.as_slice())
                            .await;
                    }
                    return Err("aborted".to_string());
                }
            }

            if let Some(cb) = &callbacks.on_tools_end {
                cb(json!({"tool_results": tool_results.clone()}));
            }

            if persist_tool_messages {
                if let Some(sid) = session_id.as_ref() {
                    self.message_manager
                        .save_tool_results(sid, expanded_tool_results.as_slice())
                        .await;
                }
            }

            let tool_outputs: Vec<Value> = expanded_tool_results
                .iter()
                .map(|r| {
                    json!({
                        "type": "function_call_output",
                        "call_id": r.tool_call_id,
                        "output": cap_tool_output_for_input(r.content.as_str())
                    })
                })
                .collect();
            pending_tool_outputs = Some(tool_outputs.clone());
            pending_tool_calls = Some(tool_call_items.clone());

            let assistant_item = if !ai_response.content.is_empty() {
                Some(to_message_item(
                    "assistant",
                    &Value::String(ai_response.content.clone()),
                    force_text_content,
                ))
            } else {
                None
            };

            if let Some(items) = stateless_context_items.as_mut() {
                if let Some(item) = assistant_item.clone() {
                    items.push(item);
                }
                if include_tool_items {
                    items.extend(tool_call_items.clone());
                    items.extend(tool_outputs.clone());
                }
            }

            let mut next_input = Value::Array(tool_outputs.clone());
            let mut next_prev_id = ai_response.response_id.clone();
            let mut next_use_prev_id = should_use_prev_id_for_next_turn(
                prefer_stateless,
                can_use_prev_id,
                next_prev_id.is_some(),
            );
            if use_prev_id && next_prev_id.is_none() {
                warn!("[AI_V3] missing response_id for tool call; fallback to stateless input");
                if let Some(sid) = session_id.as_ref() {
                    self.prev_response_id_disabled_sessions.insert(sid.clone());
                }
                can_use_prev_id = false;
                next_use_prev_id = false;
            }

            if !next_use_prev_id {
                let mut stateless = if let Some(items) = stateless_context_items.clone() {
                    items
                } else {
                    let current_items = build_current_input_items(&raw_input, force_text_content);
                    self.build_stateless_items(
                        session_id.clone(),
                        adaptive_history_limit,
                        stable_prefix_mode,
                        force_text_content,
                        &current_items,
                        include_tool_items,
                        sub_agent_run_id.clone(),
                    )
                    .await
                };

                if stateless_context_items.is_none() {
                    if let Some(item) = assistant_item {
                        stateless.push(item);
                    }
                    if include_tool_items {
                        stateless.extend(tool_call_items.clone());
                        stateless.extend(tool_outputs.clone());
                    }
                    stateless_context_items = Some(stateless.clone());
                }

                next_input = Value::Array(stateless);
                next_prev_id = None;
            }

            input = next_input;
            previous_response_id = next_prev_id;
            use_prev_id = next_use_prev_id;
            iteration += 1;
        }
    }
}

impl AiClientSettings for AiClient {
    fn apply_settings(&mut self, effective: &Value) {
        if let Some(v) = effective.get("MAX_ITERATIONS").and_then(|v| v.as_i64()) {
            self.max_iterations = v;
        }
        if let Some(v) = effective.get("HISTORY_LIMIT").and_then(|v| v.as_i64()) {
            self.history_limit = v.max(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Arc;

    use axum::{
        extract::State,
        http::{header, HeaderValue, StatusCode},
        routing::post,
        Json, Router,
    };
    use serde_json::{json, Value};
    use tokio::sync::Mutex;

    use super::{AiClient, AiClientCallbacks};
    use crate::services::v3::ai_request_handler::AiRequestHandler;
    use crate::services::v3::mcp_tool_execute::McpToolExecute;
    use crate::services::v3::message_manager::MessageManager;

    #[derive(Clone)]
    struct MockProviderState {
        steps: Arc<Mutex<VecDeque<MockProviderStep>>>,
        captured_payloads: Arc<Mutex<Vec<Value>>>,
    }

    #[derive(Clone)]
    struct MockProviderStep {
        status: StatusCode,
        content_type: &'static str,
        body: String,
    }

    impl MockProviderStep {
        fn text(status: StatusCode, body: impl Into<String>) -> Self {
            Self {
                status,
                content_type: "text/plain; charset=utf-8",
                body: body.into(),
            }
        }

        fn json(status: StatusCode, body: Value) -> Self {
            Self {
                status,
                content_type: "application/json",
                body: body.to_string(),
            }
        }

        fn sse(events: Vec<Value>) -> Self {
            let mut body = String::new();
            for event in events {
                body.push_str("data: ");
                body.push_str(event.to_string().as_str());
                body.push_str("\n\n");
            }
            body.push_str("data: [DONE]\n\n");
            Self {
                status: StatusCode::OK,
                content_type: "text/event-stream",
                body,
            }
        }
    }

    async fn mock_provider_handler(
        State(state): State<MockProviderState>,
        Json(payload): Json<Value>,
    ) -> (StatusCode, [(header::HeaderName, HeaderValue); 1], String) {
        state.captured_payloads.lock().await.push(payload);
        let next = state.steps.lock().await.pop_front().unwrap_or_else(|| {
            MockProviderStep::json(
                StatusCode::OK,
                json!({
                    "id": "mock-default",
                    "status": "completed",
                    "output_text": "ok"
                }),
            )
        });
        (
            next.status,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static(next.content_type),
            )],
            next.body,
        )
    }

    async fn start_mock_provider(
        steps: Vec<MockProviderStep>,
    ) -> (String, Arc<Mutex<Vec<Value>>>, tokio::task::JoinHandle<()>) {
        let state = MockProviderState {
            steps: Arc::new(Mutex::new(steps.into_iter().collect())),
            captured_payloads: Arc::new(Mutex::new(Vec::new())),
        };
        let captured = state.captured_payloads.clone();
        let app = Router::new()
            .route("/responses", post(mock_provider_handler))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock provider");
        let addr = listener.local_addr().expect("read mock provider addr");
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{addr}"), captured, handle)
    }

    fn build_test_client(base_url: String) -> AiClient {
        let message_manager = MessageManager::new();
        AiClient::new(
            AiRequestHandler::new("test-key".to_string(), base_url, message_manager.clone()),
            McpToolExecute::new(vec![], vec![], vec![]),
            message_manager,
        )
    }

    fn empty_callbacks() -> AiClientCallbacks {
        AiClientCallbacks {
            on_chunk: None,
            on_thinking: None,
            on_tools_start: None,
            on_tools_stream: None,
            on_tools_end: None,
            on_context_summarized_start: None,
            on_context_summarized_stream: None,
            on_context_summarized_end: None,
        }
    }

    #[tokio::test]
    async fn recovers_prev_id_then_completion_overflow_and_succeeds() {
        let steps = vec![
            MockProviderStep::text(
                StatusCode::BAD_REQUEST,
                "unsupported parameter: previous_response_id",
            ),
            MockProviderStep::json(
                StatusCode::OK,
                json!({
                    "id": "resp_failed",
                    "status": "failed",
                    "error": { "message": "context_length_exceeded: input exceeds the context window" }
                }),
            ),
            MockProviderStep::json(
                StatusCode::OK,
                json!({
                    "id": "resp_ok",
                    "status": "completed",
                    "output_text": "final answer"
                }),
            ),
        ];
        let (base_url, captured, server) = start_mock_provider(steps).await;
        let mut client = build_test_client(base_url);

        let result = client
            .process_with_tools(
                Value::String("hello".to_string()),
                Some("prev_resp_1".to_string()),
                vec![],
                None,
                None,
                "gpt-4o".to_string(),
                "gpt".to_string(),
                None,
                0.7,
                None,
                empty_callbacks(),
                false,
                None,
                "agent",
                0,
                true,
                true,
                Value::String("hello".to_string()),
                8,
                false,
                false,
                false,
                None,
                None,
                None,
            )
            .await
            .expect("process should recover and succeed");
        server.abort();

        assert_eq!(
            result.get("content").and_then(|value| value.as_str()),
            Some("final answer")
        );

        let requests = captured.lock().await.clone();
        assert_eq!(requests.len(), 3);
        assert!(requests[0].get("previous_response_id").is_some());
        assert!(requests[1].get("previous_response_id").is_none());
        assert!(requests[2].get("previous_response_id").is_none());
        assert!(requests[1]
            .get("input")
            .map(|value| value.is_array())
            .unwrap_or(false));
        assert!(requests[2]
            .get("input")
            .map(|value| value.is_array())
            .unwrap_or(false));
    }

    #[tokio::test]
    async fn recovers_input_must_be_list_and_retries_with_list_payload() {
        let steps = vec![
            MockProviderStep::text(StatusCode::BAD_REQUEST, "input must be a list"),
            MockProviderStep::json(
                StatusCode::OK,
                json!({
                    "id": "resp_ok",
                    "status": "completed",
                    "output_text": "list retry success"
                }),
            ),
        ];
        let (base_url, captured, server) = start_mock_provider(steps).await;
        let mut client = build_test_client(base_url);

        let result = client
            .process_with_tools(
                Value::String("hello".to_string()),
                None,
                vec![],
                None,
                None,
                "gpt-4o".to_string(),
                "gpt".to_string(),
                None,
                0.7,
                None,
                empty_callbacks(),
                false,
                None,
                "agent",
                0,
                false,
                false,
                Value::String("hello".to_string()),
                8,
                false,
                false,
                false,
                None,
                None,
                None,
            )
            .await
            .expect("process should recover input list constraint");
        server.abort();

        assert_eq!(
            result.get("content").and_then(|value| value.as_str()),
            Some("list retry success")
        );

        let requests = captured.lock().await.clone();
        assert_eq!(requests.len(), 2);
        assert!(requests[0]
            .get("input")
            .map(|value| value.is_string())
            .unwrap_or(false));
        assert!(requests[1]
            .get("input")
            .map(|value| value.is_array())
            .unwrap_or(false));
    }

    #[tokio::test]
    async fn recovers_missing_tool_call_output_with_pending_tool_items_merged() {
        let steps = vec![
            MockProviderStep::json(
                StatusCode::OK,
                json!({
                    "id": "resp_tool_1",
                    "status": "completed",
                    "output": [{
                        "type": "function_call",
                        "call_id": "call_tool_1",
                        "name": "demo_echo",
                        "arguments": "{\"text\":\"hello\"}"
                    }]
                }),
            ),
            MockProviderStep::text(
                StatusCode::BAD_REQUEST,
                "No tool call found for function_call_output item",
            ),
            MockProviderStep::json(
                StatusCode::OK,
                json!({
                    "id": "resp_tool_done",
                    "status": "completed",
                    "output_text": "tool recovery success"
                }),
            ),
        ];
        let (base_url, captured, server) = start_mock_provider(steps).await;
        let mut client = build_test_client(base_url);

        let result = client
            .process_with_tools(
                Value::String("hello".to_string()),
                Some("prev_resp_seed".to_string()),
                vec![json!({
                    "type": "function",
                    "name": "demo_echo",
                    "description": "demo echo",
                    "parameters": {
                        "type": "object",
                        "properties": { "text": { "type": "string" } },
                        "required": ["text"],
                        "additionalProperties": false
                    }
                })],
                None,
                None,
                "gpt-4o".to_string(),
                "gpt".to_string(),
                None,
                0.7,
                None,
                empty_callbacks(),
                false,
                None,
                "agent",
                0,
                true,
                true,
                Value::String("hello".to_string()),
                8,
                false,
                false,
                false,
                None,
                None,
                None,
            )
            .await
            .expect("process should recover missing tool-call context");
        server.abort();

        assert_eq!(
            result.get("content").and_then(|value| value.as_str()),
            Some("tool recovery success")
        );

        let requests = captured.lock().await.clone();
        assert_eq!(requests.len(), 3);

        assert_eq!(
            requests[0]
                .get("previous_response_id")
                .and_then(|value| value.as_str()),
            Some("prev_resp_seed")
        );
        assert_eq!(
            requests[1]
                .get("previous_response_id")
                .and_then(|value| value.as_str()),
            Some("resp_tool_1")
        );
        assert!(requests[1]
            .get("input")
            .and_then(|value| value.as_array())
            .map(|items| {
                items.iter().all(|item| {
                    item.get("type").and_then(|value| value.as_str())
                        == Some("function_call_output")
                })
            })
            .unwrap_or(false));

        assert!(requests[2].get("previous_response_id").is_none());
        assert!(requests[2]
            .get("input")
            .and_then(|value| value.as_array())
            .map(|items| {
                let has_call = items.iter().any(|item| {
                    item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                        && item.get("call_id").and_then(|value| value.as_str())
                            == Some("call_tool_1")
                });
                let has_output = items.iter().any(|item| {
                    item.get("type").and_then(|value| value.as_str())
                        == Some("function_call_output")
                        && item.get("call_id").and_then(|value| value.as_str())
                            == Some("call_tool_1")
                });
                has_call && has_output
            })
            .unwrap_or(false));
    }

    #[tokio::test]
    async fn recovers_missing_tool_call_output_in_stream_mode_with_pending_items_merged() {
        let first_stream_events = vec![json!({
            "type": "response.completed",
            "response": {
                "id": "resp_stream_tool_1",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "call_id": "call_stream_tool_1",
                    "name": "demo_echo",
                    "arguments": "{\"text\":\"hello\"}"
                }]
            }
        })];
        let third_stream_events = vec![
            json!({ "type": "response.output_text.delta", "delta": "stream " }),
            json!({ "type": "response.output_text.delta", "delta": "tool recovery success" }),
            json!({
                "type": "response.completed",
                "response": {
                    "id": "resp_stream_tool_done",
                    "status": "completed",
                    "output_text": "stream tool recovery success"
                }
            }),
        ];
        let steps = vec![
            MockProviderStep::sse(first_stream_events),
            MockProviderStep::text(
                StatusCode::BAD_REQUEST,
                "No tool call found for function_call_output item",
            ),
            MockProviderStep::sse(third_stream_events),
        ];
        let (base_url, captured, server) = start_mock_provider(steps).await;
        let mut client = build_test_client(base_url);

        let callbacks = AiClientCallbacks {
            on_chunk: Some(Arc::new(|_chunk: String| {})),
            on_thinking: None,
            on_tools_start: None,
            on_tools_stream: None,
            on_tools_end: None,
            on_context_summarized_start: None,
            on_context_summarized_stream: None,
            on_context_summarized_end: None,
        };

        let result = client
            .process_with_tools(
                Value::String("hello".to_string()),
                Some("prev_resp_stream_seed".to_string()),
                vec![json!({
                    "type": "function",
                    "name": "demo_echo",
                    "description": "demo echo",
                    "parameters": {
                        "type": "object",
                        "properties": { "text": { "type": "string" } },
                        "required": ["text"],
                        "additionalProperties": false
                    }
                })],
                None,
                None,
                "gpt-4o".to_string(),
                "gpt".to_string(),
                None,
                0.7,
                None,
                callbacks,
                false,
                None,
                "agent",
                0,
                true,
                true,
                Value::String("hello".to_string()),
                8,
                false,
                false,
                false,
                None,
                None,
                None,
            )
            .await
            .expect("stream mode should recover missing tool-call context");
        server.abort();

        assert_eq!(
            result.get("content").and_then(|value| value.as_str()),
            Some("stream tool recovery success")
        );

        let requests = captured.lock().await.clone();
        assert_eq!(requests.len(), 3);

        assert_eq!(
            requests[0]
                .get("previous_response_id")
                .and_then(|value| value.as_str()),
            Some("prev_resp_stream_seed")
        );
        assert_eq!(
            requests[1]
                .get("previous_response_id")
                .and_then(|value| value.as_str()),
            Some("resp_stream_tool_1")
        );
        assert!(requests[0]
            .get("stream")
            .and_then(|value| value.as_bool())
            .unwrap_or(false));
        assert!(requests[1]
            .get("stream")
            .and_then(|value| value.as_bool())
            .unwrap_or(false));
        assert!(requests[2]
            .get("stream")
            .and_then(|value| value.as_bool())
            .unwrap_or(false));

        assert!(requests[2].get("previous_response_id").is_none());
        assert!(requests[2]
            .get("input")
            .and_then(|value| value.as_array())
            .map(|items| {
                let has_call = items.iter().any(|item| {
                    item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                        && item.get("call_id").and_then(|value| value.as_str())
                            == Some("call_stream_tool_1")
                });
                let has_output = items.iter().any(|item| {
                    item.get("type").and_then(|value| value.as_str())
                        == Some("function_call_output")
                        && item.get("call_id").and_then(|value| value.as_str())
                            == Some("call_stream_tool_1")
                });
                has_call && has_output
            })
            .unwrap_or(false));
    }

    #[tokio::test]
    async fn recovers_stream_response_failed_missing_tool_call_without_completed_event() {
        let first_stream_events = vec![json!({
            "type": "response.completed",
            "response": {
                "id": "resp_stream_failed_seed",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "call_id": "call_stream_failed_1",
                    "name": "demo_echo",
                    "arguments": "{\"text\":\"hello\"}"
                }]
            }
        })];
        let second_stream_events = vec![json!({
            "type": "response.failed",
            "response": {
                "id": "resp_stream_failed_mid",
                "status": "failed",
                "error": {
                    "message": "No tool call found for function_call_output item"
                }
            }
        })];
        let third_stream_events = vec![json!({
            "type": "response.completed",
            "response": {
                "id": "resp_stream_failed_done",
                "status": "completed",
                "output_text": "stream failed recovery success"
            }
        })];
        let steps = vec![
            MockProviderStep::sse(first_stream_events),
            MockProviderStep::sse(second_stream_events),
            MockProviderStep::sse(third_stream_events),
        ];
        let (base_url, captured, server) = start_mock_provider(steps).await;
        let mut client = build_test_client(base_url);

        let callbacks = AiClientCallbacks {
            on_chunk: Some(Arc::new(|_chunk: String| {})),
            on_thinking: None,
            on_tools_start: None,
            on_tools_stream: None,
            on_tools_end: None,
            on_context_summarized_start: None,
            on_context_summarized_stream: None,
            on_context_summarized_end: None,
        };

        let result = client
            .process_with_tools(
                Value::String("hello".to_string()),
                Some("prev_resp_stream_failed".to_string()),
                vec![json!({
                    "type": "function",
                    "name": "demo_echo",
                    "description": "demo echo",
                    "parameters": {
                        "type": "object",
                        "properties": { "text": { "type": "string" } },
                        "required": ["text"],
                        "additionalProperties": false
                    }
                })],
                None,
                None,
                "gpt-4o".to_string(),
                "gpt".to_string(),
                None,
                0.7,
                None,
                callbacks,
                false,
                None,
                "agent",
                0,
                true,
                true,
                Value::String("hello".to_string()),
                8,
                false,
                false,
                false,
                None,
                None,
                None,
            )
            .await
            .expect("stream failed branch should recover missing tool-call context");
        server.abort();

        assert_eq!(
            result.get("content").and_then(|value| value.as_str()),
            Some("stream failed recovery success")
        );

        let requests = captured.lock().await.clone();
        assert_eq!(requests.len(), 3);

        assert_eq!(
            requests[0]
                .get("previous_response_id")
                .and_then(|value| value.as_str()),
            Some("prev_resp_stream_failed")
        );
        assert_eq!(
            requests[1]
                .get("previous_response_id")
                .and_then(|value| value.as_str()),
            Some("resp_stream_failed_seed")
        );
        assert!(requests[2].get("previous_response_id").is_none());

        assert!(requests[1]
            .get("input")
            .and_then(|value| value.as_array())
            .map(|items| {
                items.iter().all(|item| {
                    item.get("type").and_then(|value| value.as_str())
                        == Some("function_call_output")
                })
            })
            .unwrap_or(false));
        assert!(requests[2]
            .get("input")
            .and_then(|value| value.as_array())
            .map(|items| {
                let has_call = items.iter().any(|item| {
                    item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                        && item.get("call_id").and_then(|value| value.as_str())
                            == Some("call_stream_failed_1")
                });
                let has_output = items.iter().any(|item| {
                    item.get("type").and_then(|value| value.as_str())
                        == Some("function_call_output")
                        && item.get("call_id").and_then(|value| value.as_str())
                            == Some("call_stream_failed_1")
                });
                has_call && has_output
            })
            .unwrap_or(false));
    }

    #[tokio::test]
    async fn recovers_stream_error_and_failed_without_status_with_pending_items() {
        let first_stream_events = vec![json!({
            "type": "response.completed",
            "response": {
                "id": "resp_stream_mix_seed",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "call_id": "call_stream_mix_1",
                    "name": "demo_echo",
                    "arguments": "{\"text\":\"hello\"}"
                }]
            }
        })];
        let second_stream_events = vec![
            json!({
                "type": "error",
                "error": {
                    "message": "No tool call found for function_call_output item"
                }
            }),
            json!({
                "type": "response.failed",
                "response": {
                    "id": "resp_stream_mix_mid"
                }
            }),
        ];
        let third_stream_events = vec![json!({
            "type": "response.completed",
            "response": {
                "id": "resp_stream_mix_done",
                "status": "completed",
                "output_text": "stream mixed failure recovery success"
            }
        })];
        let steps = vec![
            MockProviderStep::sse(first_stream_events),
            MockProviderStep::sse(second_stream_events),
            MockProviderStep::sse(third_stream_events),
        ];
        let (base_url, captured, server) = start_mock_provider(steps).await;
        let mut client = build_test_client(base_url);

        let callbacks = AiClientCallbacks {
            on_chunk: Some(Arc::new(|_chunk: String| {})),
            on_thinking: None,
            on_tools_start: None,
            on_tools_stream: None,
            on_tools_end: None,
            on_context_summarized_start: None,
            on_context_summarized_stream: None,
            on_context_summarized_end: None,
        };

        let result = client
            .process_with_tools(
                Value::String("hello".to_string()),
                Some("prev_resp_stream_mix".to_string()),
                vec![json!({
                    "type": "function",
                    "name": "demo_echo",
                    "description": "demo echo",
                    "parameters": {
                        "type": "object",
                        "properties": { "text": { "type": "string" } },
                        "required": ["text"],
                        "additionalProperties": false
                    }
                })],
                None,
                None,
                "gpt-4o".to_string(),
                "gpt".to_string(),
                None,
                0.7,
                None,
                callbacks,
                false,
                None,
                "agent",
                0,
                true,
                true,
                Value::String("hello".to_string()),
                8,
                false,
                false,
                false,
                None,
                None,
                None,
            )
            .await
            .expect("stream mixed failure branch should recover");
        server.abort();

        assert_eq!(
            result.get("content").and_then(|value| value.as_str()),
            Some("stream mixed failure recovery success")
        );

        let requests = captured.lock().await.clone();
        assert_eq!(requests.len(), 3);

        assert_eq!(
            requests[0]
                .get("previous_response_id")
                .and_then(|value| value.as_str()),
            Some("prev_resp_stream_mix")
        );
        assert_eq!(
            requests[1]
                .get("previous_response_id")
                .and_then(|value| value.as_str()),
            Some("resp_stream_mix_seed")
        );
        assert!(requests[2].get("previous_response_id").is_none());
        assert!(requests[2]
            .get("input")
            .and_then(|value| value.as_array())
            .map(|items| {
                let has_call = items.iter().any(|item| {
                    item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                        && item.get("call_id").and_then(|value| value.as_str())
                            == Some("call_stream_mix_1")
                });
                let has_output = items.iter().any(|item| {
                    item.get("type").and_then(|value| value.as_str())
                        == Some("function_call_output")
                        && item.get("call_id").and_then(|value| value.as_str())
                            == Some("call_stream_mix_1")
                });
                has_call && has_output
            })
            .unwrap_or(false));
    }

    #[tokio::test]
    async fn recovers_stream_with_second_tool_call_without_pending_duplication() {
        let first_stream_events = vec![json!({
            "type": "response.completed",
            "response": {
                "id": "resp_stream_round_1",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "call_id": "call_stream_round_1",
                    "name": "demo_echo",
                    "arguments": "{\"text\":\"hello\"}"
                }]
            }
        })];
        let second_stream_events = vec![
            json!({
                "type": "error",
                "error": {
                    "message": "No tool call found for function_call_output item"
                }
            }),
            json!({
                "type": "response.failed",
                "response": {
                    "id": "resp_stream_round_fail"
                }
            }),
        ];
        let third_stream_events = vec![json!({
            "type": "response.completed",
            "response": {
                "id": "resp_stream_round_2",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "call_id": "call_stream_round_2",
                    "name": "demo_echo",
                    "arguments": "{\"text\":\"again\"}"
                }]
            }
        })];
        let fourth_stream_events = vec![json!({
            "type": "response.completed",
            "response": {
                "id": "resp_stream_round_done",
                "status": "completed",
                "output_text": "stream round-trip success"
            }
        })];
        let steps = vec![
            MockProviderStep::sse(first_stream_events),
            MockProviderStep::sse(second_stream_events),
            MockProviderStep::sse(third_stream_events),
            MockProviderStep::sse(fourth_stream_events),
        ];
        let (base_url, captured, server) = start_mock_provider(steps).await;
        let mut client = build_test_client(base_url);

        let callbacks = AiClientCallbacks {
            on_chunk: Some(Arc::new(|_chunk: String| {})),
            on_thinking: None,
            on_tools_start: None,
            on_tools_stream: None,
            on_tools_end: None,
            on_context_summarized_start: None,
            on_context_summarized_stream: None,
            on_context_summarized_end: None,
        };

        let result = client
            .process_with_tools(
                Value::String("hello".to_string()),
                Some("prev_resp_stream_round_seed".to_string()),
                vec![json!({
                    "type": "function",
                    "name": "demo_echo",
                    "description": "demo echo",
                    "parameters": {
                        "type": "object",
                        "properties": { "text": { "type": "string" } },
                        "required": ["text"],
                        "additionalProperties": false
                    }
                })],
                None,
                None,
                "gpt-4o".to_string(),
                "gpt".to_string(),
                None,
                0.7,
                None,
                callbacks,
                false,
                None,
                "agent",
                0,
                true,
                true,
                Value::String("hello".to_string()),
                8,
                false,
                false,
                false,
                None,
                None,
                None,
            )
            .await
            .expect("stream should recover and continue with second tool call");
        server.abort();

        assert_eq!(
            result.get("content").and_then(|value| value.as_str()),
            Some("stream round-trip success")
        );

        let requests = captured.lock().await.clone();
        assert_eq!(requests.len(), 4);

        assert_eq!(
            requests[1]
                .get("previous_response_id")
                .and_then(|value| value.as_str()),
            Some("resp_stream_round_1")
        );
        assert!(requests[2].get("previous_response_id").is_none());
        assert!(requests[3].get("previous_response_id").is_none());

        assert!(requests[2]
            .get("input")
            .and_then(|value| value.as_array())
            .map(|items| {
                let call_1 = items
                    .iter()
                    .filter(|item| {
                        item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                            && item.get("call_id").and_then(|value| value.as_str())
                                == Some("call_stream_round_1")
                    })
                    .count();
                let output_1 = items
                    .iter()
                    .filter(|item| {
                        item.get("type").and_then(|value| value.as_str())
                            == Some("function_call_output")
                            && item.get("call_id").and_then(|value| value.as_str())
                                == Some("call_stream_round_1")
                    })
                    .count();
                call_1 == 1 && output_1 == 1
            })
            .unwrap_or(false));

        assert!(requests[3]
            .get("input")
            .and_then(|value| value.as_array())
            .map(|items| {
                let call_1 = items
                    .iter()
                    .filter(|item| {
                        item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                            && item.get("call_id").and_then(|value| value.as_str())
                                == Some("call_stream_round_1")
                    })
                    .count();
                let output_1 = items
                    .iter()
                    .filter(|item| {
                        item.get("type").and_then(|value| value.as_str())
                            == Some("function_call_output")
                            && item.get("call_id").and_then(|value| value.as_str())
                                == Some("call_stream_round_1")
                    })
                    .count();
                let call_2 = items
                    .iter()
                    .filter(|item| {
                        item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                            && item.get("call_id").and_then(|value| value.as_str())
                                == Some("call_stream_round_2")
                    })
                    .count();
                let output_2 = items
                    .iter()
                    .filter(|item| {
                        item.get("type").and_then(|value| value.as_str())
                            == Some("function_call_output")
                            && item.get("call_id").and_then(|value| value.as_str())
                                == Some("call_stream_round_2")
                    })
                    .count();
                call_1 == 1 && output_1 == 1 && call_2 == 1 && output_2 == 1
            })
            .unwrap_or(false));
    }
}
