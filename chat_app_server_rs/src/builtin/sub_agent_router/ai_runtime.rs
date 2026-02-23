use super::*;

pub(super) fn run_ai_task_with_system_messages(
    ctx: &BoundContext,
    system_messages: Vec<String>,
    task: &str,
    requested_model: Option<&str>,
    on_stream_chunk: Option<ToolStreamChunkCallback>,
) -> Result<AiTaskResult, String> {
    let user_id = ctx.user_id.clone();
    let requested = requested_model.map(|v| v.trim().to_string());
    let task_text = task.to_string();
    let timeout_ms = ctx.ai_timeout_ms;
    let normalized_system_messages = system_messages
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    trace_router_node(
        "run_ai_task",
        "start",
        None,
        None,
        None,
        Some(json!({
            "task": truncate_for_event(task_text.as_str(), 2_000),
            "requested_model": requested.clone(),
            "timeout_ms": timeout_ms,
            "system_message_count": normalized_system_messages.len(),
        })),
    );

    let result = block_on_result(async move {
        let model = resolve_model_config(user_id, requested).await?;
        trace_router_node(
            "run_ai_task",
            "model_resolved",
            None,
            None,
            None,
            Some(json!({
                "model_id": model.id.clone(),
                "model_name": model.name.clone(),
                "provider": model.provider.clone(),
                "model": model.model.clone(),
                "supports_responses": model.supports_responses,
            })),
        );
        if model.api_key.trim().is_empty() {
            trace_router_node("run_ai_task", "model_missing_key", None, None, None, None);
            return Err(
                "No usable AI API key found in model configs or OPENAI_API_KEY".to_string(),
            );
        }

        let (response_text, reasoning, finish_reason) = if model.supports_responses {
            let message_manager = MessageManager::new();
            let handler = AiRequestHandler::new(
                model.api_key.clone(),
                model.base_url.clone(),
                message_manager,
            );

            let mut input = normalized_system_messages
                .iter()
                .map(|content| {
                    json!({
                        "role": "system",
                        "content": [
                            { "type": "input_text", "text": content }
                        ]
                    })
                })
                .collect::<Vec<_>>();
            input.push(json!({
                "role": "user",
                "content": [
                    { "type": "input_text", "text": task_text }
                ]
            }));

            let req = handler.handle_request(
                Value::Array(input),
                model.model.clone(),
                None,
                None,
                None,
                Some(0.2),
                None,
                StreamCallbacks {
                    on_chunk: on_stream_chunk.clone(),
                    on_thinking: None,
                },
                Some(model.provider.clone()),
                model.thinking_level.clone(),
                None,
                true,
                "sub_agent_router",
            );

            let response = crate::core::ai_response::run_with_timeout(timeout_ms, req).await?;

            let content = crate::core::ai_response::normalize_non_empty_content(&response.content);

            (content, response.reasoning, response.finish_reason)
        } else {
            let message_manager = LegacyMessageManager::new();
            let handler = LegacyAiRequestHandler::new(
                model.api_key.clone(),
                model.base_url.clone(),
                message_manager,
            );

            let mut messages = normalized_system_messages
                .iter()
                .map(|content| {
                    json!({
                        "role": "system",
                        "content": content,
                    })
                })
                .collect::<Vec<_>>();
            messages.push(json!({
                "role": "user",
                "content": task_text,
            }));

            let req = handler.handle_request(
                messages,
                None,
                model.model.clone(),
                Some(0.2),
                None,
                crate::services::v2::ai_request_handler::StreamCallbacks {
                    on_chunk: on_stream_chunk.clone(),
                    on_thinking: None,
                },
                true,
                Some(model.provider.clone()),
                model.thinking_level.clone(),
                None,
                true,
                "sub_agent_router",
            );

            let response = crate::core::ai_response::run_with_timeout(timeout_ms, req).await?;

            let content = crate::core::ai_response::normalize_non_empty_content(&response.content);

            (content, response.reasoning, response.finish_reason)
        };

        trace_router_node(
            "run_ai_task",
            "finish",
            None,
            None,
            None,
            Some(json!({
                "model_id": model.id.clone(),
                "model_name": model.name.clone(),
                "provider": model.provider.clone(),
                "model": model.model.clone(),
                "response_preview": truncate_for_event(response_text.as_str(), 2_000),
                "reasoning_preview": truncate_for_event(reasoning.as_deref().unwrap_or_default(), 2_000),
                "finish_reason": finish_reason.clone(),
            })),
        );
        Ok(AiTaskResult {
            response: response_text,
            reasoning,
            finish_reason,
            model_id: model.id,
            model_name: model.name,
            provider: model.provider,
            model: model.model,
        })
    });

    if let Err(err) = &result {
        trace_router_node(
            "run_ai_task",
            "error",
            None,
            None,
            None,
            Some(json!({
                "error": truncate_for_event(err.as_str(), 2_000),
            })),
        );
    }

    result
}

pub(super) async fn resolve_effective_mcp_selection(
    user_id: Option<String>,
) -> Result<EffectiveMcpSelection, String> {
    let mut configured = false;
    let mut ids = Vec::new();

    if let Ok(saved) = settings::load_mcp_permissions() {
        configured = saved
            .get("configured")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        ids = parse_string_array(saved.get("enabled_mcp_ids")).unwrap_or_default();
    }

    ids.retain(|id| !id.eq_ignore_ascii_case(SUB_AGENT_ROUTER_MCP_ID));
    let ids = crate::core::mcp_runtime::normalize_mcp_ids(&ids);

    if configured {
        return Ok(EffectiveMcpSelection { configured, ids });
    }

    let mut all_ids = list_builtin_mcp_configs()
        .into_iter()
        .map(|cfg| cfg.id)
        .collect::<Vec<_>>();

    let mut custom = mcp_configs::list_mcp_configs(user_id.clone()).await?;
    if custom.is_empty() && user_id.is_some() {
        custom = mcp_configs::list_mcp_configs(None).await?;
    }

    all_ids.extend(custom.into_iter().map(|cfg| cfg.id));

    all_ids.retain(|value| !value.eq_ignore_ascii_case(SUB_AGENT_ROUTER_MCP_ID));
    let ids = crate::core::mcp_runtime::normalize_mcp_ids(&all_ids);

    Ok(EffectiveMcpSelection {
        configured: false,
        ids,
    })
}

pub(super) async fn resolve_selected_system_context_prompt(
    user_id: Option<String>,
) -> Result<Option<SelectedSystemContextPrompt>, String> {
    let saved = settings::load_mcp_permissions()?;
    let selected_context_id = saved
        .get("selected_system_context_id")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let Some(context_id) = selected_context_id else {
        return Ok(None);
    };

    let Some(context) = crate::repositories::system_contexts::get_system_context_by_id(context_id.as_str()).await? else {
        return Ok(None);
    };

    if let Some(uid) = user_id.as_deref() {
        if !context.user_id.trim().is_empty() && context.user_id.trim() != uid {
            return Ok(None);
        }
    }

    let content = context
        .content
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);

    let Some(content) = content else {
        return Ok(None);
    };

    let context_name = context.name.trim();

    Ok(Some(SelectedSystemContextPrompt {
        context_id,
        context_name: if context_name.is_empty() {
            "(unnamed)".to_string()
        } else {
            context_name.to_string()
        },
        content,
    }))
}

pub(super) fn filter_tools_by_prefixes(
    mcp_execute: &mut McpToolExecute,
    allow_prefixes: &[String],
) -> (usize, usize) {
    filter_tools_by_prefixes_impl(mcp_execute, allow_prefixes)
}

pub(super) fn filter_legacy_tools_by_prefixes(
    mcp_execute: &mut LegacyMcpToolExecute,
    allow_prefixes: &[String],
) -> (usize, usize) {
    filter_tools_by_prefixes_impl(mcp_execute, allow_prefixes)
}

trait ToolFilterTarget {
    fn tools_mut(&mut self) -> &mut Vec<Value>;
    fn retain_tool_metadata_by_names(&mut self, names: &HashSet<String>);
}

impl ToolFilterTarget for McpToolExecute {
    fn tools_mut(&mut self) -> &mut Vec<Value> {
        &mut self.tools
    }

    fn retain_tool_metadata_by_names(&mut self, names: &HashSet<String>) {
        self.tool_metadata.retain(|name, _| names.contains(name));
    }
}

impl ToolFilterTarget for LegacyMcpToolExecute {
    fn tools_mut(&mut self) -> &mut Vec<Value> {
        &mut self.tools
    }

    fn retain_tool_metadata_by_names(&mut self, names: &HashSet<String>) {
        self.tool_metadata.retain(|name, _| names.contains(name));
    }
}

fn normalize_allow_prefixes(allow_prefixes: &[String]) -> Vec<String> {
    unique_strings(
        allow_prefixes
            .iter()
            .map(|value| value.trim().to_lowercase())
            .filter(|value| !value.is_empty()),
    )
}

fn filter_tools_by_prefixes_impl<T>(target: &mut T, allow_prefixes: &[String]) -> (usize, usize)
where
    T: ToolFilterTarget,
{
    let before = target.tools_mut().len();
    let prefixes = normalize_allow_prefixes(allow_prefixes);

    if prefixes.is_empty() {
        target.tools_mut().clear();
        target.retain_tool_metadata_by_names(&HashSet::new());
        return (before, 0);
    }

    let mut kept_tool_names = HashSet::new();
    target.tools_mut().retain(|tool| {
        let Some(name) = extract_tool_name_from_schema(tool) else {
            return false;
        };

        let keep = prefixes
            .iter()
            .any(|prefix| tool_matches_allowed_prefix(name, prefix.as_str()));

        if keep {
            kept_tool_names.insert(name.to_string());
        }

        keep
    });

    target.retain_tool_metadata_by_names(&kept_tool_names);
    (before, kept_tool_names.len())
}

fn extract_tool_name_from_schema(tool: &Value) -> Option<&str> {
    tool.get("name")
        .and_then(|value| value.as_str())
        .or_else(|| {
            tool.get("function")
                .and_then(|func| func.get("name"))
                .and_then(|value| value.as_str())
        })
}

fn tool_matches_allowed_prefix(tool_name: &str, prefix: &str) -> bool {
    let tool = tool_name.trim().to_lowercase();
    let prefix = prefix.trim().to_lowercase();

    if tool.is_empty() || prefix.is_empty() {
        return false;
    }

    tool == prefix || tool.starts_with(format!("{}_", prefix).as_str())
}

pub(super) fn summarize_tool_calls_for_event(tool_calls: &Value) -> Value {
    let Some(arr) = tool_calls.as_array() else {
        return tool_calls.clone();
    };

    Value::Array(
        arr.iter()
            .map(|item| {
                let tool_call_id = item
                    .get("id")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                let name = item
                    .get("function")
                    .and_then(|func| func.get("name"))
                    .and_then(|value| value.as_str())
                    .or_else(|| item.get("name").and_then(|value| value.as_str()))
                    .unwrap_or_default();

                let arguments_value = item
                    .get("function")
                    .and_then(|func| func.get("arguments"))
                    .or_else(|| item.get("arguments"));
                let arguments_preview = arguments_value
                    .map(|value| value_to_preview(value, 2_000))
                    .unwrap_or_default();

                json!({
                    "tool_call_id": tool_call_id,
                    "name": name,
                    "arguments_preview": arguments_preview,
                })
            })
            .collect(),
    )
}

pub(super) fn summarize_tool_results_for_event(tool_results: &Value) -> Value {
    let arr = tool_results
        .get("tool_results")
        .and_then(|value| value.as_array())
        .or_else(|| tool_results.as_array());

    let Some(arr) = arr else {
        return summarize_single_tool_result_for_event(tool_results);
    };

    let summarized = arr
        .iter()
        .map(summarize_single_tool_result_for_event)
        .collect::<Vec<_>>();

    json!({ "tool_results": summarized })
}

pub(super) fn summarize_single_tool_result_for_event(result: &Value) -> Value {
    let tool_call_id = result
        .get("tool_call_id")
        .or_else(|| result.get("toolCallId"))
        .or_else(|| result.get("id"))
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    let name = result
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    let success = result
        .get("success")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let is_error = result
        .get("is_error")
        .or_else(|| result.get("isError"))
        .and_then(|value| value.as_bool())
        .unwrap_or(!success);

    let content_preview = result
        .get("content")
        .or_else(|| result.get("result"))
        .or_else(|| result.get("output"))
        .map(|value| value_to_preview(value, 4_000))
        .unwrap_or_default();

    json!({
        "tool_call_id": tool_call_id,
        "name": name,
        "success": success,
        "is_error": is_error,
        "content_preview": content_preview,
    })
}

fn value_to_preview(value: &Value, max_chars: usize) -> String {
    let raw = if let Some(text) = value.as_str() {
        text.to_string()
    } else {
        value.to_string()
    };

    truncate_for_event(raw.as_str(), max_chars)
}

pub(super) async fn resolve_model_config(
    user_id: Option<String>,
    requested: Option<String>,
) -> Result<ResolvedModel, String> {
    let mut models = ai_model_configs::list_ai_model_configs(user_id.clone()).await?;
    if models.is_empty() && user_id.is_some() {
        models = ai_model_configs::list_ai_model_configs(None).await?;
    }

    let enabled_models: Vec<_> = models.into_iter().filter(|m| m.enabled).collect();
    let requested_norm = requested
        .as_deref()
        .map(|v| v.trim().to_lowercase())
        .filter(|v| !v.is_empty());

    if let Some(ref needle) = requested_norm {
        if let Some(found) = enabled_models
            .iter()
            .find(|cfg| model_matches(cfg, needle.as_str()))
        {
            return Ok(to_resolved_model(found.clone()));
        }
        return Err(format!(
            "Requested model is not enabled or not configured: {}",
            needle
        ));
    }

    if let Some(first) = enabled_models.into_iter().next() {
        return Ok(to_resolved_model(first));
    }

    let cfg = Config::get();
    let fallback_model = requested
        .as_deref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "gpt-4o-mini".to_string());

    Ok(ResolvedModel {
        id: "env_default".to_string(),
        name: "Environment Default".to_string(),
        provider: "gpt".to_string(),
        model: fallback_model,
        thinking_level: None,
        supports_responses: true,
        api_key: cfg.openai_api_key.clone(),
        base_url: cfg.openai_base_url.clone(),
    })
}

fn model_matches(cfg: &crate::models::ai_model_config::AiModelConfig, needle: &str) -> bool {
    cfg.id.trim().eq_ignore_ascii_case(needle)
        || cfg.name.trim().eq_ignore_ascii_case(needle)
        || cfg.model.trim().eq_ignore_ascii_case(needle)
}

fn to_resolved_model(cfg: crate::models::ai_model_config::AiModelConfig) -> ResolvedModel {
    let env_cfg = Config::get();
    ResolvedModel {
        id: cfg.id,
        name: cfg.name,
        provider: normalize_provider(cfg.provider.as_str()),
        model: cfg.model,
        thinking_level: cfg.thinking_level,
        supports_responses: cfg.supports_responses,
        api_key: cfg
            .api_key
            .as_deref()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| env_cfg.openai_api_key.clone()),
        base_url: cfg
            .base_url
            .as_deref()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| env_cfg.openai_base_url.clone()),
    }
}
